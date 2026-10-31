//! The rabbit on the dashboard (NabaztagGroup): reachable or not, whether it shows Tempo, its
//! tai chi (the remote's « Taichi » key plays it too) and a way to push today's colour again.
//! Out of reach, both buttons stay, out of reach. No group at all once the server says no
//! rabbit is set.

use maison_api::nabaztag::{CommandRequest, TAICHI, TempoPushRequest};
use serde_json::Value;

use super::{Cx, NABAZTAG, loaded, stale};
use crate::commands::{Order, Post, Say};
use crate::i18n::{Words, k};
use crate::paths;
use crate::ui::icon;
use crate::ui::model::{Act, Body, Button, Group, Lead, Part, Tile};

/// The rabbit's tile, from `/nabaztag` (`{config: {host, tempoEnabled}, reachable}`).
pub fn tile(w: &Words, status: &Value) -> Tile {
    let reachable = status["reachable"].as_bool().unwrap_or(false);
    let synced = status["config"]["tempoEnabled"].as_bool().unwrap_or(false);
    let state = match (reachable, synced) {
        (false, _) => w.s(k::STATE_UNREACHABLE),
        (true, true) => w.s(k::NABAZTAG_REACHABLE_SYNCED),
        (true, false) => w.s(k::NABAZTAG_REACHABLE),
    };
    let name = w.s(k::NABAZTAG_RABBIT);
    let button = |icon, label: String, order: Order| {
        Button::new(Some(icon), label, Act::Order(order)).unavailable(!reachable)
    };
    let dance = Order::one(
        format!("{NABAZTAG}:taichi"),
        name.clone(),
        Post::json(
            paths::NABAZTAG_CTL,
            &CommandRequest {
                command: TAICHI.into(),
            },
        ),
    )
    .said(Say::Done(k::NABAZTAG_TAICHI_DONE));
    let push = Order::one(
        format!("{NABAZTAG}:tempo"),
        name.clone(),
        Post::json(paths::NABAZTAG_TEMPO_PUSH, &TempoPushRequest::default()),
    )
    .then_refresh(&[NABAZTAG])
    .said(Say::Done(k::NABAZTAG_TEMPO_PUSHED));
    Tile::new(
        name,
        state,
        Lead::Glyph {
            icon: icon::RABBIT,
            on: reachable,
        },
    )
    .warn(!reachable)
    .parts(vec![Part::Row(vec![
        button(icon::RABBIT, w.s(k::NABAZTAG_TAICHI), dance),
        button(icon::REFRESH_CW, w.s(k::NABAZTAG_PUSH_TEMPO), push),
    ])])
}

pub fn group(cx: &mut Cx) -> Option<Group> {
    let status = cx.live.want(&cx.sources.rabbit);
    // the server says no rabbit is set: no group
    if status
        .data
        .as_deref()
        .is_some_and(|s| s["config"]["host"].is_null())
    {
        return None;
    }
    let w = cx.words;
    let body = loaded(w, &status, &[NABAZTAG], ("", ""), |s| {
        Some(Body::Tiles(vec![tile(w, s)]))
    });
    Some(Group::new(w.s(k::NABAZTAG_NAME), body).stale(stale(cx, &status, &[NABAZTAG])))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;
    use maison_api::Answer;

    fn status(json: &str) -> Value {
        serde_json::from_str::<Answer<Value>>(json).unwrap().body
    }

    #[test]
    fn reachable_it_dances_and_pushes_tempo() {
        let w = Words::new(Lang::Fr);
        let t = tile(
            &w,
            &status(
                r#"{"success":true,"config":{"host":"192.168.1.20","tempoEnabled":true},"reachable":true}"#,
            ),
        );
        assert_eq!(t.name, "Le lapin");
        assert_eq!(t.state, "Joignable · Tempo synchronisé");
        let [Part::Row(b)] = &t.parts[..] else {
            panic!()
        };
        let Act::Order(dance) = &b[0].act else {
            panic!()
        };
        assert_eq!(dance.posts[0].1.path, "/nabaztag/ctl");
        assert_eq!(
            dance.posts[0].1.body.as_ref().unwrap()["command"],
            "chor taichi"
        );
        assert!(b.iter().all(|b| !b.unavailable));
    }

    #[test]
    fn out_of_reach_its_buttons_stay_out_of_reach() {
        let w = Words::new(Lang::Fr);
        let t = tile(
            &w,
            &status(
                r#"{"success":true,"config":{"host":"192.168.1.20","tempoEnabled":true},"reachable":false}"#,
            ),
        );
        assert!(t.warn);
        assert_eq!(t.state, "Injoignable");
        let [Part::Row(b)] = &t.parts[..] else {
            panic!()
        };
        assert!(b.iter().all(|b| b.unavailable));
    }
}
