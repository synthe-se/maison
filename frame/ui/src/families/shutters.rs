//! Roller shutters over Matter (ShuttersGroup, ShutterTile, shutters/words.ts and sun.ts): Open ·
//! Stop · Close, the position as « how open » (sent on release), the next scheduled move and
//! skipping it once (« Ne pas fermer ce soir », then « Fermeture de 18:42 sautée · Annuler »).
//! Out of reach, the buttons stay, not operable, the state line saying why. The frame leaves
//! renaming, the sun schedule and removal to the web app.

use chrono::{DateTime, Duration as Days, Utc};
use maison_api::Tz;
use maison_api::matter::{Cover, CoverMotion, PositionRequest, SkipRequest, SunEvent};

use super::{Cx, SHUTTERS, loaded, outcome, stale};
use crate::commands::{Order, Outcome, Post, Say, Toast, json, limit};
use crate::i18n::{Arg, Words, count, k};
use crate::live::View;
use crate::paths;
use crate::ui::icon::{self, Icon};
use crate::ui::model::{Act, Body, Button, Group, Lead, Part, Slider, Tile};

/// « Fermé », « Ouvert à 40 % », « Ouvert ».
pub fn at(w: &Words, percent: u8) -> String {
    match percent {
        100.. => w.s(k::SHUTTERS_OPEN),
        0 => w.s(k::SHUTTERS_CLOSED),
        p => w.t(
            k::SHUTTERS_OPEN_PERCENT,
            &[("percent", Arg::Count(f64::from(p)))],
        ),
    }
}

/// Line 2 of a shutter's tile.
pub fn describe(w: &Words, c: &Cover) -> String {
    if !c.online {
        return w.s(k::STATE_UNREACHABLE);
    }
    if let (true, Some(open), Some(target)) = (c.moving(), c.open_percent, c.target_open_percent)
        && open != target
    {
        return format!(
            "{} · {}",
            at(w, open),
            w.t(
                k::SHUTTERS_GOING_TO,
                &[("percent", Arg::Count(f64::from(target)))]
            )
        );
    }
    match (c.motion, c.open_percent) {
        (Some(CoverMotion::Opening), _) => w.s(k::SHUTTERS_OPENING),
        (Some(CoverMotion::Closing), _) => w.s(k::SHUTTERS_CLOSING),
        (_, None) => w.s(k::SHUTTERS_UNCALIBRATED),
        (_, Some(p)) => at(w, p),
    }
}

/// The next scheduled move: which, when, whether it will be skipped once.
pub fn next_event(c: &Cover) -> Option<(SunEvent, DateTime<Utc>, bool)> {
    [
        c.next_open.map(|t| (SunEvent::Open, t, c.skip_next_open)),
        c.next_close
            .map(|t| (SunEvent::Close, t, c.skip_next_close)),
    ]
    .into_iter()
    .flatten()
    .min_by_key(|(_, t, _)| *t)
}

/// « Fermeture 20:24 », or « Fermeture de 20:24 sautée ».
pub fn next_move(w: &Words, c: &Cover) -> Option<String> {
    let (event, t, skipped) = next_event(c)?;
    let time = w.clock(t.with_timezone(&maison_api::HOUSE_TZ).time());
    let key = match (event, skipped) {
        (SunEvent::Open, true) => k::SHUTTERS_OPEN_SKIPPED,
        (SunEvent::Close, true) => k::SHUTTERS_CLOSE_SKIPPED,
        (SunEvent::Open, false) => k::SHUTTERS_FACT_OPEN,
        (SunEvent::Close, false) => k::SHUTTERS_FACT_CLOSE,
    };
    Some(w.t(key, &[("time", time.into())]))
}

/// « Ne pas fermer ce soir », « Ne pas ouvrir demain matin »: skipping the next move once.
pub fn skip_label(w: &Words, event: SunEvent, t: DateTime<Utc>, now: &DateTime<Tz>) -> String {
    let day = t.with_timezone(&now.timezone()).date_naive();
    let today = now.date_naive();
    let tomorrow = today + Days::days(1);
    w.s(match event {
        SunEvent::Close if day == today => k::SHUTTERS_SKIP_CLOSE_TONIGHT,
        SunEvent::Close if day == tomorrow => k::SHUTTERS_SKIP_CLOSE_TOMORROW,
        SunEvent::Close => k::SHUTTERS_SKIP_CLOSE,
        SunEvent::Open if day == today => k::SHUTTERS_SKIP_OPEN_TODAY,
        SunEvent::Open if day == tomorrow => k::SHUTTERS_SKIP_OPEN_TOMORROW,
        SunEvent::Open => k::SHUTTERS_SKIP_OPEN,
    })
}

fn cover_path(c: &Cover, what: &str) -> String {
    paths::of(paths::COVERS, &c.id, what)
}

fn mv(c: &Cover, what: &'static str, key: &str) -> Order {
    Order::one(
        format!("{SHUTTERS}:{}:{key}", c.id),
        c.name.clone(),
        Post::empty(cover_path(c, what)),
    )
    .within(limit::DEVICE)
    .then_refresh(&[SHUTTERS])
}

/// The position slider's body (0 closed – 100 open).
fn position(value: f64) -> serde_json::Value {
    json(&PositionRequest {
        open_percent: value.round().clamp(0.0, 100.0) as u8,
    })
}

pub fn tile(w: &Words, c: &Cover, now: &DateTime<Tz>) -> Tile {
    let off = !c.online;
    let button = |icon: Icon, label: &str, order: Order| {
        Button::new(Some(icon), label, Act::Order(order)).unavailable(off)
    };
    let mut parts = vec![Part::Row(vec![
        button(
            icon::ARROW_UP,
            &w.s(k::SHUTTERS_OPEN_ACTION),
            mv(c, "open", "move"),
        ),
        // Stop always goes, even while another order travels: its own key
        button(icon::SQUARE, &w.s(k::COMMON_STOP), mv(c, "stop", "stop")).primary(c.moving()),
        button(
            icon::ARROW_DOWN,
            &w.s(k::SHUTTERS_CLOSE_ACTION),
            mv(c, "close", "move"),
        ),
    ])];
    if let (Some((event, t, skipped)), Some(text)) = (next_event(c), next_move(w, c)) {
        let label = if skipped {
            w.s(k::SHUTTERS_SKIP_UNDO)
        } else {
            skip_label(w, event, t, now)
        };
        let skip = Order::one(
            format!("{SHUTTERS}:{}:skip", c.id),
            c.name.clone(),
            Post::json(
                cover_path(c, "skip"),
                &SkipRequest {
                    event,
                    skip: !skipped,
                },
            ),
        )
        .then_refresh(&[SHUTTERS]);
        parts.push(Part::Note {
            text,
            link: Button::new(None, label, Act::Order(skip)),
            skipped,
        });
    }
    match (c.online, c.open_percent) {
        (true, Some(open)) => parts.push(Part::Slider(Slider {
            key: format!("{SHUTTERS}:{}:position", c.id),
            value: f64::from(c.target_open_percent.unwrap_or(open)),
            min: 0.0,
            max: 100.0,
            step: 5.0,
            mark: c.moving().then_some(f64::from(open)),
            live: false,
            // Range's `near = step`: a reading within a step lets the held value go
            near: 5.0,
            ends: Some((w.s(k::SHUTTERS_CLOSED), w.s(k::SHUTTERS_OPEN))),
            path: cover_path(c, "position"),
            body: position,
            refresh: SHUTTERS,
            name: c.name.clone(),
        })),
        (true, None) => parts.push(Part::Hint(w.s(k::SHUTTERS_CALIBRATE_HINT))),
        _ => {}
    }
    Tile {
        open: None,
        name: c.name.clone(),
        state: describe(w, c),
        warn: off,
        lead: Lead::Glyph {
            icon: icon::BLINDS,
            on: c.online && c.open_percent.unwrap_or(0) > 0,
        },
        fact: None,
        parts,
    }
}

fn all_opening(w: &Words, o: &Outcome) -> Option<Toast> {
    Some(outcome(w, o, |n| w.t(k::SHUTTERS_ALL_OPENING, &[count(n)])))
}

fn all_closing(w: &Words, o: &Outcome) -> Option<Toast> {
    Some(outcome(w, o, |n| w.t(k::SHUTTERS_ALL_CLOSING, &[count(n)])))
}

pub fn shutters_group(
    w: &Words,
    v: &View<Vec<Cover>>,
    now: &DateTime<Tz>,
    stale: Option<(String, Button)>,
) -> Group {
    let covers = v.data.as_deref().map(Vec::as_slice).unwrap_or_default();
    let online: Vec<&Cover> = covers.iter().filter(|c| c.online).collect();
    let mut actions = Vec::new();
    if !online.is_empty() {
        let all = |what: &'static str, say: Say| {
            let posts = online
                .iter()
                .map(|c| (c.name.clone(), Post::empty(cover_path(c, what))))
                .collect();
            Order::many(format!("{SHUTTERS}:all-{what}"), posts)
                .within(limit::DEVICE)
                .then_refresh(&[SHUTTERS])
                .said(say)
        };
        actions.push(Button::new(
            Some(icon::ARROW_UP),
            w.s(k::SHUTTERS_ALL_OPEN),
            Act::Order(all("open", all_opening)),
        ));
        actions.push(Button::new(
            Some(icon::ARROW_DOWN),
            w.s(k::SHUTTERS_ALL_CLOSE),
            Act::Order(all("close", all_closing)),
        ));
    }
    let body = loaded(
        w,
        v,
        &[SHUTTERS],
        (k::SHUTTERS_NONE, k::SHUTTERS_NONE_HINT),
        |covers| {
            (!covers.is_empty())
                .then(|| Body::Tiles(covers.iter().map(|c| tile(w, c, now)).collect()))
        },
    );
    Group {
        title: w.s(k::SHUTTERS_TITLE),
        fact: None,
        actions,
        stale,
        body,
    }
}

pub fn group(cx: &mut Cx) -> Option<Group> {
    let v = cx.live.want(&cx.sources.covers).map(|d| d.covers.clone());
    let now = cx.house_now();
    let old = stale(cx, &v, &[SHUTTERS]);
    Some(shutters_group(cx.words, &v, &now, old))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;
    use chrono::TimeZone;
    use maison_api::matter::Covers;
    use maison_api::{Answer, HOUSE_TZ};

    const COVERS: &str = r#"{"success":true,"covers":[{"id":"c1","name":"Salon","endpoint":1,"online":true,"openPercent":40,"targetOpenPercent":100,"motion":"opening","vendorId":4874,"productId":1,"schedule":{"openAtSunrise":true,"closeAtSunset":true,"sunriseOffsetMin":0,"sunsetOffsetMin":60},"nextOpen":"2026-11-13T06:12:00Z","nextClose":"2026-11-12T17:42:00Z","skipNextOpen":false,"skipNextClose":false},{"id":"c2","name":"Chambre","endpoint":1,"online":false,"openPercent":null,"targetOpenPercent":null,"motion":null,"vendorId":null,"productId":null,"schedule":{"openAtSunrise":false,"closeAtSunset":false,"sunriseOffsetMin":0,"sunsetOffsetMin":0},"nextOpen":null,"nextClose":null,"skipNextOpen":false,"skipNextClose":false}]}"#;

    fn covers() -> Vec<Cover> {
        serde_json::from_str::<Answer<Covers>>(COVERS)
            .unwrap()
            .body
            .covers
    }

    fn now() -> DateTime<Tz> {
        HOUSE_TZ.with_ymd_and_hms(2026, 11, 12, 15, 0, 0).unwrap()
    }

    #[test]
    fn a_shutter_in_words() {
        let w = Words::new(Lang::Fr);
        let cs = covers();
        assert_eq!(
            describe(&w, &cs[0]),
            "Ouvert à 40\u{202f}% · va à 100\u{202f}%"
        );
        assert_eq!(describe(&w, &cs[1]), "Injoignable");
        assert_eq!(at(&w, 0), "Fermé");
        assert_eq!(at(&w, 100), "Ouvert");
        // the closing comes first: 17:42 UTC is 18:42 at the house
        assert_eq!(next_move(&w, &cs[0]).unwrap(), "Fermeture 18:42");
        let (e, t, _) = next_event(&cs[0]).unwrap();
        assert_eq!(skip_label(&w, e, t, &now()), "Ne pas fermer ce soir");
    }

    #[test]
    fn a_tile_moves_skips_and_places() {
        let w = Words::new(Lang::Fr);
        let cs = covers();
        let t = tile(&w, &cs[0], &now());
        let Part::Row(b) = &t.parts[0] else {
            panic!("row")
        };
        assert_eq!(b.len(), 3);
        assert!(b[1].primary, "stop stands out while a motor runs");
        let Act::Order(open) = &b[0].act else {
            panic!()
        };
        assert_eq!(open.posts[0].1.path, "/matter/covers/c1/open");
        let Part::Note {
            text,
            link,
            skipped,
        } = &t.parts[1]
        else {
            panic!("note")
        };
        assert_eq!(text, "Fermeture 18:42");
        assert!(!skipped);
        let Act::Order(skip) = &link.act else {
            panic!()
        };
        assert_eq!(
            skip.posts[0].1.body,
            Some(serde_json::json!({"event": "close", "skip": true}))
        );
        let Part::Slider(s) = &t.parts[2] else {
            panic!("slider")
        };
        assert_eq!(
            (s.value, s.mark, s.step, s.near),
            (100.0, Some(40.0), 5.0, 5.0)
        );
        assert_eq!((s.body)(35.0), serde_json::json!({"openPercent": 35}));
        assert!(!s.live);
        // unreachable: the buttons stay, not operable; no slider
        let off = tile(&w, &cs[1], &now());
        let Part::Row(b) = &off.parts[0] else {
            panic!("row")
        };
        assert!(b.iter().all(|b| b.unavailable));
        assert_eq!(off.parts.len(), 1);
        assert!(off.warn);
    }

    #[test]
    fn the_group_moves_every_shutter_that_answers() {
        let w = Words::new(Lang::Fr);
        let v = View::ready(covers());
        let g = shutters_group(&w, &v, &now(), None);
        assert_eq!(g.title, "Volets");
        assert_eq!(g.actions.len(), 2);
        let Act::Order(o) = &g.actions[1].act else {
            panic!()
        };
        assert_eq!(o.posts.len(), 1, "the unreachable one is left out");
        assert_eq!(o.posts[0].1.path, "/matter/covers/c1/close");
    }
}
