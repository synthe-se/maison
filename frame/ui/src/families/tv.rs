//! « Télé » on the dashboard (TvGroup, TvTile, BoxTile, tv/words.ts): the Philips TV and the
//! Android box in one list. The TV's icon (and its row) turns it on or off over infrared; up but
//! JointSPACE silent, its state is assumed (docs/ux.md § 2, case 3): « Allumer » and
//! « Éteindre » instead. The box's icon wakes it or puts it to sleep (the TV follows over CEC),
//! its app chips launch an app, the TV turned on first. Both read as slowly as the TV allows.
//! Each row opens its page (TvPage, BoxPage): the pad and the media keys, each key its own order
//! that shows nothing in flight and repeats while held (docs/ux.md § 7); the TV's volume, its
//! input, Ambilight. The TV out of deep standby takes ~20 s: a bar on the elapsed time. Assumed,
//! the last order is said under the two buttons. Pairing, the settings and APKs stay the web
//! app's.

use std::time::Duration;

use chrono::DateTime;
use maison_api::androidtv::{self, LaunchRequest, SHORTCUTS};
use maison_api::remote::{AndroidKey, SwitchState, TvKey};
use maison_api::tv::{AmbilightRequest, KeyRequest, PowerRequest, VolumeRequest};
use maison_api::{Tz, paces};
use serde::Serialize;
use serde_json::Value;

use super::{BOX, Cx, TV, loaded, stale};
use crate::commands::{Order, Outcome, Post, Say, Toast, json, limit};
use crate::i18n::{Arg, Words, k};
use crate::paths;
use crate::ui::icon::{self, Icon};
use crate::ui::model::{
    Act, Body, Button, Group, Lead, Pad, Part, Screen, Slider, Tile, View as Page,
};

/// The keys the pad both remotes share knows, each remote mapping them to its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PadKey {
    Up,
    Down,
    Left,
    Right,
    Ok,
    Back,
    Home,
    Menu,
}

/// An arrow repeats ten times a second, what the pad is held for.
const ARROW_REPEAT: Duration = Duration::from_millis(paces::KEY_REPEAT_MS);
const VOLUME_REPEAT: Duration = Duration::from_millis(paces::VOLUME_REPEAT_MS);

/// The key the TV's last power order is kept under.
fn power_key() -> String {
    format!("{TV}:power")
}

/// A remote's key: one post, its own key, nothing read after (a key changes no status, and the
/// TV's status is four calls).
fn key_order(
    family: &str,
    name: &str,
    path: &str,
    which: &impl Serialize,
    body: &impl Serialize,
) -> Order {
    let which = json(which);
    Order::one(
        format!("{family}:key:{}", which.as_str().unwrap_or_default()),
        name,
        Post::json(path, body),
    )
}

fn tv_key(name: &str, key: TvKey) -> Order {
    key_order(TV, name, paths::TV_KEY, &key, &KeyRequest { key })
}

fn box_key(name: &str, key: AndroidKey) -> Order {
    key_order(
        BOX,
        name,
        paths::BOX_KEY,
        &key,
        &androidtv::KeyRequest { key },
    )
}

/// A key of a remote; `every`: how often it repeats while held.
fn key_button(icon: Icon, label: String, order: Order, every: Option<Duration>) -> Button {
    Button::new(Some(icon), label, Act::Key(order, every))
}

/// The pad: four arrows around OK (words would not fit a 56 px key), `under` it with theirs.
fn pad(w: &Words, off: bool, send: &dyn Fn(PadKey) -> Order, under: &[PadKey]) -> Part {
    let arrow =
        |key, icon| key_button(icon, String::new(), send(key), Some(ARROW_REPEAT)).unavailable(off);
    let word =
        |key, icon, label: &str| key_button(icon, w.s(label), send(key), None).unavailable(off);
    Part::Pad(Box::new(Pad {
        up: arrow(PadKey::Up, icon::CHEVRON_UP),
        left: arrow(PadKey::Left, icon::CHEVRON_LEFT),
        ok: key_button(icon::CIRCLE, String::new(), send(PadKey::Ok), None).unavailable(off),
        right: arrow(PadKey::Right, icon::CHEVRON_RIGHT),
        down: arrow(PadKey::Down, icon::CHEVRON_DOWN),
        under: under
            .iter()
            .map(|k| match k {
                PadKey::Back => word(*k, icon::CORNER_UP_LEFT, k::KEY_BACK),
                PadKey::Home => word(*k, icon::HOUSE, k::NAV_HOME),
                _ => word(*k, icon::MENU, k::KEY_MENU),
            })
            .collect(),
    }))
}

/// Up, JointSPACE silent: the backend says « on » and reads nothing else (`assumedOn`).
pub fn assumed_on(status: &Value) -> bool {
    status["power"] == "on" && status["volume"].is_null()
}

/// The TV's power order: `on` or `off`, the box's input taken when it comes on; out of deep
/// standby, timed (`waking`).
fn power(name: &str, on: bool, waking: bool) -> Order {
    Order::one(
        power_key(),
        name,
        Post::json(
            paths::TV_POWER,
            &PowerRequest {
                state: if on {
                    SwitchState::On
                } else {
                    SwitchState::Off
                },
                switch_to_box: true,
            },
        ),
    )
    .toggle(on)
    .within(limit::TV)
    .then_refresh(&[TV, BOX])
    .timed(on && waking)
}

/// The volume a step down and a step up, each button saying the level it sets (TvTile's
/// − / +); none while the TV is not on or says no volume.
fn volume_steps(w: &Words, name: &str, volume: &Value) -> Option<Part> {
    let (level, min, max) = (
        volume["current"].as_i64()?,
        volume["min"].as_i64().unwrap_or(0),
        volume["max"].as_i64().unwrap_or(i64::MAX),
    );
    let step = |delta: i64, icon| {
        // never `clamp`: a server's bounds, even crossed, are no reason to panic while drawing
        let to = level
            .saturating_add(delta)
            .min(max)
            .max(min)
            .clamp(0, i64::from(u8::MAX));
        let order = Order::one(
            format!("{TV}:volume"),
            name,
            Post::put(
                paths::TV_VOLUME,
                &VolumeRequest {
                    level: Some(to as u8),
                    muted: None,
                },
            ),
        )
        .then_refresh(&[TV]);
        Button::new(
            Some(icon),
            w.t(k::TV_VOLUME_FACT, &[("level", w.num(to as f64, 0).into())]),
            Act::Order(order),
        )
        .unavailable(to == level)
    };
    Some(Part::Row(vec![step(-1, icon::MINUS), step(1, icon::PLUS)]))
}

/// The bar of the TV coming out of deep standby: only while it does.
fn waking(w: &Words) -> Part {
    Part::Progress {
        key: power_key(),
        label: w.s(k::TV_WAKING),
        elapsed: k::TV_WAKING_ELAPSED,
        limit: limit::TV,
    }
}

/// « Dernier ordre : allumer, à 19:05 », what an assumed state can say.
fn last_order(w: &Words, (on, at): (bool, DateTime<Tz>)) -> String {
    let key = if on {
        k::TV_LAST_ORDER_ON
    } else {
        k::TV_LAST_ORDER_OFF
    };
    w.t(key, &[("time", w.clock(at.time()).into())])
}

/// The TV's row, from `/tv` (`{config, status}`) and its last order from here, if any.
pub fn tv_tile(w: &Words, data: &Value, last: Option<(bool, DateTime<Tz>)>) -> Tile {
    let s = &data["status"];
    let configured = s["configured"].as_bool().unwrap_or(false);
    let assumed = assumed_on(s);
    let name = s["name"]
        .as_str()
        .filter(|n| !n.is_empty())
        .map_or_else(|| w.s(k::TV_DEFAULT_NAME), str::to_string);
    let state = if !configured {
        w.s(k::TV_NOT_CONFIGURED)
    } else if assumed {
        w.s(k::TV_ASSUMED)
    } else {
        w.s(match s["power"].as_str() {
            Some("on") => k::STATE_ON,
            Some("deep_standby") => k::TV_DEEP_STANDBY,
            _ => k::STATE_STANDBY,
        })
    };
    let volume = &s["volume"];
    let fact = (s["power"] == "on" && volume.is_object()).then(|| {
        if volume["muted"].as_bool().unwrap_or(false) {
            w.s(k::TV_MUTED)
        } else {
            w.t(
                k::TV_VOLUME_FACT,
                &[(
                    "level",
                    w.num(volume["current"].as_f64().unwrap_or(0.0), 0).into(),
                )],
            )
        }
    });
    let on = s["power"] == "on";
    let deep = s["power"] == "deep_standby";
    let (lead, parts) = if configured && !assumed {
        let mut parts = vec![waking(w)];
        parts.extend(volume_steps(w, &name, volume));
        (
            Lead::Toggle {
                icon: icon::TV,
                on,
                offline: false,
                turn_on: Box::new(power(&name, true, deep)),
                turn_off: Box::new(power(&name, false, false)),
            },
            parts,
        )
    } else {
        let mut parts = if configured {
            vec![Part::Row(vec![
                Button::new(
                    Some(icon::POWER),
                    w.s(k::ACTION_TURN_ON),
                    Act::Order(power(&name, true, false)),
                ),
                Button::new(
                    Some(icon::POWER),
                    w.s(k::ACTION_TURN_OFF),
                    Act::Order(power(&name, false, false)),
                ),
            ])]
        } else {
            Vec::new()
        };
        if configured {
            parts.extend(last.map(|l| Part::Hint(last_order(w, l))));
        }
        (
            Lead::Glyph {
                icon: icon::TV,
                on: false,
            },
            parts,
        )
    };
    Tile::new(name, state, lead)
        .warn(assumed)
        .fact(fact)
        .parts(parts)
        .open(configured.then_some(Act::Open(Page::Tv)))
}

fn launched_said(w: &Words, o: &Outcome) -> Option<Toast> {
    o.failed.is_empty().then(|| Toast {
        text: w.t(
            k::ANDROID_TV_LAUNCHED,
            &[("name", o.done.join(", ").into())],
        ),
        warn: false,
    })
}

/// The box's shortcuts: its configured favourites, else Maison's (`boxApps`).
pub fn shortcuts(config: &Value) -> Vec<(String, String)> {
    let favourites: Vec<(String, String)> = config["favoriteApps"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| Some((a["package"].as_str()?.into(), a["label"].as_str()?.into())))
        .collect();
    if favourites.is_empty() {
        SHORTCUTS
            .iter()
            .map(|(p, l)| ((*p).into(), (*l).into()))
            .collect()
    } else {
        favourites
    }
}

/// The box's row, from `/androidtv` (`{config, status}`).
pub fn box_tile(w: &Words, data: &Value) -> Tile {
    let s = &data["status"];
    let configured = s["configured"].as_bool().unwrap_or(false);
    let reachable = s["reachable"].as_bool().unwrap_or(false);
    let awake = s["awake"].as_bool().unwrap_or(false);
    let name = s["model"]
        .as_str()
        .filter(|n| !n.is_empty())
        .map_or_else(|| w.s(k::ANDROID_TV_BOX), str::to_string);
    let state = w.s(if !configured {
        k::ANDROID_TV_NOT_CONFIGURED
    } else if !reachable {
        k::STATE_UNREACHABLE
    } else if awake {
        k::ANDROID_TV_AWAKE
    } else {
        k::ANDROID_TV_ASLEEP
    });
    let wake = |on: bool| {
        Order::one(
            format!("{BOX}:awake"),
            name.clone(),
            Post::empty(if on {
                paths::BOX_WAKE
            } else {
                paths::BOX_SLEEP
            }),
        )
        .toggle(on)
        .within(limit::TV)
        .then_refresh(&[BOX, TV])
    };
    let current = s["currentApp"].as_str();
    let apps = shortcuts(&data["config"])
        .into_iter()
        .map(|(package, label)| {
            let launch = Order::one(
                format!("{BOX}:launch"),
                label.clone(),
                Post::json(
                    paths::BOX_LAUNCH,
                    &LaunchRequest {
                        package: package.clone(),
                        ensure_tv_on: true,
                    },
                ),
            )
            .within(limit::TV)
            .then_refresh(&[BOX, TV])
            .said(Say::With(launched_said));
            Button::new(None, label, Act::Order(launch))
                .primary(current == Some(package.as_str()))
                .unavailable(!reachable)
        })
        .collect();
    let lead = if configured {
        Lead::Toggle {
            icon: icon::PLAY,
            on: awake,
            offline: !reachable,
            turn_on: Box::new(wake(true)),
            turn_off: Box::new(wake(false)),
        }
    } else {
        Lead::Glyph {
            icon: icon::PLAY,
            on: false,
        }
    };
    Tile::new(name.clone(), state, lead)
        .parts(if configured {
            vec![Part::Row(apps)]
        } else {
            Vec::new()
        })
        .open(configured.then_some(Act::Open(Page::Box)))
}

/// The TV's last power order from here, on the house's clock.
fn last(cx: &Cx) -> Option<(bool, DateTime<Tz>)> {
    let now = cx.house_now();
    let (on, at) = cx.commands.last(&power_key())?;
    Some((on, at.with_timezone(&now.timezone())))
}

/// One volume level (PUT `{level}`, on release: the TV's server is not to be chased).
fn level(value: f64) -> Value {
    json(&VolumeRequest {
        level: Some(value.round().clamp(0.0, f64::from(u8::MAX)) as u8),
        muted: None,
    })
}

/// The TV's volume on its page: − and +, mute, the slider.
fn volume_tile(w: &Words, name: &str, volume: &Value) -> Option<Tile> {
    let current = volume["current"].as_f64()?;
    let (min, max) = (
        volume["min"].as_f64().unwrap_or(0.0),
        volume["max"].as_f64().unwrap_or(100.0),
    );
    let muted = volume["muted"].as_bool().unwrap_or(false);
    let mute = Order::one(
        format!("{TV}:volume"),
        name,
        Post::put(
            paths::TV_VOLUME,
            &VolumeRequest {
                level: None,
                muted: Some(!muted),
            },
        ),
    )
    .then_refresh(&[TV]);
    let mut parts = vec![Part::Slider(Slider {
        key: format!("{TV}:level"),
        value: current,
        min,
        max: max.max(min + 1.0),
        step: 1.0,
        mark: None,
        live: false,
        put: true,
        off: false,
        near: 1.0,
        ends: None,
        path: paths::TV_VOLUME.into(),
        body: level,
        refresh: TV,
        name: name.to_string(),
    })];
    let mut row = match volume_steps(w, name, volume) {
        Some(Part::Row(steps)) => steps,
        _ => Vec::new(),
    };
    row.push(Button::new(Some(icon::VOLUME_X), w.s(k::KEY_MUTE), Act::Order(mute)).primary(muted));
    parts.push(Part::Row(row));
    Some(
        Tile::new(
            w.t(
                k::TV_VOLUME_VALUE,
                &[("level", Arg::Count(current)), ("max", Arg::Count(max))],
            ),
            if muted {
                w.s(k::TV_MUTED)
            } else {
                String::new()
            },
            Lead::Glyph {
                icon: if muted {
                    icon::VOLUME_X
                } else {
                    icon::VOLUME_2
                },
                on: !muted,
            },
        )
        .parts(parts),
    )
}

/// What the TV's page shows, from `/tv`: its tile; once it answers its volume and its pad; its
/// input, and Ambilight while the set says it (`lounge` drifted: the way back to the image).
pub fn tv_groups(w: &Words, data: &Value, last: Option<(bool, DateTime<Tz>)>) -> Vec<Group> {
    let s = &data["status"];
    let mut head = tv_tile(w, data, last).open(None);
    let name = head.name.clone();
    if !assumed_on(s) {
        // the page has its own volume
        head.parts.retain(|p| !matches!(p, Part::Row(_)));
    }
    if !s["configured"].as_bool().unwrap_or(false) {
        return vec![Group::new(w.s(k::TV_TITLE), Body::Tiles(vec![head]))];
    }
    if assumed_on(s) {
        head.parts.push(Part::Hint(w.s(k::TV_ASSUMED_HINT)));
    } else if s["power"] == "deep_standby" {
        head.parts.push(Part::Hint(w.s(k::TV_DEEP_STANDBY_HINT)));
    }
    let mut groups = vec![Group::new(w.s(k::TV_TITLE), Body::Tiles(vec![head]))];
    let volume = Some(&s["volume"]).filter(|v| s["power"] == "on" && v.is_object());
    let mut actions = vec![Button::new(
        Some(icon::HOUSE),
        w.s(k::TV_SWITCH_TO_BOX),
        Act::Order(
            Order::one(
                format!("{TV}:box"),
                name.clone(),
                Post::empty(paths::TV_SOURCE_BOX),
            )
            .within(limit::TV)
            .then_refresh(&[TV])
            .said(Say::Done(k::TV_SWITCHED_TO_BOX)),
        ),
    )];
    if let Some(v) = volume {
        groups.extend(
            volume_tile(w, &name, v).map(|t| Group::new(w.s(k::TV_VOLUME), Body::Tiles(vec![t]))),
        );
        let send = |key: PadKey| {
            tv_key(
                &name,
                match key {
                    PadKey::Up => TvKey::CursorUp,
                    PadKey::Down => TvKey::CursorDown,
                    PadKey::Left => TvKey::CursorLeft,
                    PadKey::Right => TvKey::CursorRight,
                    PadKey::Ok => TvKey::Confirm,
                    PadKey::Back => TvKey::Back,
                    PadKey::Home => TvKey::Home,
                    PadKey::Menu => TvKey::Options,
                },
            )
        };
        groups.push(Group::new(
            w.t(k::TV_PAD, &[("name", name.as_str().into())]),
            Body::Parts(vec![
                pad(w, false, &send, &[PadKey::Back, PadKey::Home]),
                Part::Row(vec![
                    key_button(
                        icon::MONITOR,
                        w.s(k::KEY_SOURCE),
                        tv_key(&name, TvKey::Source),
                        None,
                    ),
                    key_button(
                        icon::PLAY,
                        w.s(k::KEY_PLAY_PAUSE),
                        tv_key(&name, TvKey::PlayPause),
                        None,
                    ),
                ]),
            ]),
        ));
        let glow = &s["ambilight"];
        let lit = glow["power"].as_bool().unwrap_or(false);
        actions.push(
            Button::new(
                Some(icon::SPARKLES),
                w.s(k::TV_AMBILIGHT),
                Act::Order(
                    Order::one(
                        format!("{TV}:ambilight"),
                        name.clone(),
                        Post::json(
                            paths::TV_AMBILIGHT,
                            &AmbilightRequest {
                                state: SwitchState::Toggle,
                            },
                        ),
                    )
                    .then_refresh(&[TV]),
                ),
            )
            .primary(lit),
        );
        if lit && glow["style"] != "FOLLOW_VIDEO" {
            actions.push(Button::new(
                Some(icon::SPARKLES),
                w.s(k::TV_AMBILIGHT_FOLLOW_VIDEO),
                Act::Order(
                    Order::one(
                        format!("{TV}:follow"),
                        name.clone(),
                        Post::empty(paths::TV_AMBILIGHT_FOLLOW),
                    )
                    .then_refresh(&[TV]),
                ),
            ));
        }
    }
    groups.push(Group::new(w.s(k::TV_GROUP_TITLE), Body::Buttons(actions)));
    groups
}

pub fn tv_page(cx: &mut Cx) -> Screen {
    let tv = cx.live.want(&cx.sources.tv);
    let last = last(cx);
    let w = cx.words;
    let Some(d) = tv.data.as_deref() else {
        let body = loaded(w, &tv, &[TV], ("", ""), |_| None);
        return super::page(
            w,
            w.s(k::TV_TITLE),
            vec![Group::new(w.s(k::TV_TITLE), body)],
        );
    };
    let mut groups = tv_groups(w, d, last);
    groups[0].stale = stale(cx, &tv, &[TV]);
    let title = groups[0].body.clone();
    let title = match title {
        Body::Tiles(t) => t[0].name.clone(),
        _ => w.s(k::TV_TITLE),
    };
    super::page(w, title, groups)
}

/// What the box's page shows, from `/androidtv`: its tile, the pad, media keys and volume. Out
/// of reach they stay, not operable, the tile's state line saying why.
pub fn box_groups(w: &Words, data: &Value) -> Vec<Group> {
    let s = &data["status"];
    let head = box_tile(w, data).open(None);
    let name = head.name.clone();
    let off = !s["reachable"].as_bool().unwrap_or(false);
    let configured = s["configured"].as_bool().unwrap_or(false);
    let mut groups = vec![Group::new(
        w.s(k::ANDROID_TV_TITLE),
        Body::Tiles(vec![head]),
    )];
    if !configured {
        return groups;
    }
    let send = |key: PadKey| {
        box_key(
            &name,
            match key {
                PadKey::Up => AndroidKey::Up,
                PadKey::Down => AndroidKey::Down,
                PadKey::Left => AndroidKey::Left,
                PadKey::Right => AndroidKey::Right,
                PadKey::Ok => AndroidKey::Ok,
                PadKey::Back => AndroidKey::Back,
                PadKey::Home => AndroidKey::Home,
                PadKey::Menu => AndroidKey::Menu,
            },
        )
    };
    let key = |icon, label: &str, k: AndroidKey, every| {
        key_button(icon, w.s(label), box_key(&name, k), every).unavailable(off)
    };
    groups.push(Group::new(
        w.t(k::TV_PAD, &[("name", name.as_str().into())]),
        Body::Parts(vec![
            pad(w, off, &send, &[PadKey::Back, PadKey::Home, PadKey::Menu]),
            Part::Row(vec![
                key(icon::SEARCH, k::KEY_SEARCH, AndroidKey::Search, None),
                key(icon::SKIP_BACK, k::KEY_PREVIOUS, AndroidKey::Previous, None),
                key(icon::PLAY, k::KEY_PLAY_PAUSE, AndroidKey::PlayPause, None),
                key(icon::SKIP_FORWARD, k::KEY_NEXT, AndroidKey::Next, None),
            ]),
        ]),
    ));
    groups.push(Group::new(
        w.s(k::TV_VOLUME),
        Body::Parts(vec![Part::Row(vec![
            key(
                icon::MINUS,
                k::KEY_VOLUME_DOWN,
                AndroidKey::VolumeDown,
                Some(VOLUME_REPEAT),
            ),
            key(
                icon::PLUS,
                k::KEY_VOLUME_UP,
                AndroidKey::VolumeUp,
                Some(VOLUME_REPEAT),
            ),
            key(icon::VOLUME_X, k::KEY_MUTE, AndroidKey::Mute, None),
        ])]),
    ));
    groups
}

pub fn box_page(cx: &mut Cx) -> Screen {
    let boxed = cx.live.want(&cx.sources.androidtv);
    let w = cx.words;
    let Some(d) = boxed.data.as_deref() else {
        let title = w.s(k::ANDROID_TV_TITLE);
        let body = loaded(w, &boxed, &[BOX], ("", ""), |_| None);
        return super::page(w, title.clone(), vec![Group::new(title, body)]);
    };
    let mut groups = box_groups(w, d);
    groups[0].stale = stale(cx, &boxed, &[BOX]);
    let title = box_tile(w, d).name;
    super::page(w, title, groups)
}

pub fn group(cx: &mut Cx) -> Option<Group> {
    let tv = cx.live.want(&cx.sources.tv);
    let boxed = cx.live.want(&cx.sources.androidtv);
    let last = last(cx);
    let w = cx.words;
    let mut tiles = Vec::new();
    // each reads on its own; the group shows what came, its place kept until both did
    if let Some(d) = tv.data.as_deref() {
        tiles.push(tv_tile(w, d, last));
    }
    if let Some(d) = boxed.data.as_deref() {
        tiles.push(box_tile(w, d));
    }
    let body = if tiles.is_empty() {
        loaded(w, &tv, &[TV, BOX], ("", ""), |_| None)
    } else {
        Body::Tiles(tiles)
    };
    Some(
        Group::new(w.s(k::TV_GROUP_TITLE), body)
            .stale(stale(cx, &tv, &[TV, BOX]).or_else(|| stale(cx, &boxed, &[TV, BOX]))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;
    use maison_api::Answer;

    fn body(json: &str) -> Value {
        serde_json::from_str::<Answer<Value>>(json).unwrap().body
    }

    #[test]
    fn the_tv_read_turns_on_and_off_by_its_icon() {
        let w = Words::new(Lang::Fr);
        let t = tv_tile(
            &w,
            &body(
                r#"{"success":true,"config":{"host":"192.168.1.30"},"status":{"configured":true,"power":"on","name":"","volume":{"current":12,"min":0,"max":60,"muted":false}}}"#,
            ),
            None,
        );
        assert_eq!(t.name, "TV du salon");
        assert_eq!(t.state, "Allumée");
        assert_eq!(t.fact.as_deref(), Some("Vol.\u{a0}12"));
        let Lead::Toggle {
            on: true, turn_off, ..
        } = &t.lead
        else {
            panic!("{:?}", t.lead)
        };
        assert_eq!(turn_off.posts[0].1.path, "/tv/power");
        assert_eq!(turn_off.posts[0].1.body.as_ref().unwrap()["state"], "off");
        // the volume a step down and up, each saying the level it sets, by PUT
        assert!(matches!(t.open, Some(Act::Open(Page::Tv))));
        let [Part::Progress { .. }, Part::Row(v)] = &t.parts[..] else {
            panic!("{:?}", t.parts)
        };
        assert_eq!(v[0].label, "Vol.\u{a0}11");
        let Act::Order(up) = &v[1].act else { panic!() };
        assert!(up.posts[0].1.put);
        assert_eq!(up.posts[0].1.path, "/tv/volume");
        assert_eq!(up.posts[0].1.body.as_ref().unwrap()["level"], 13);
    }

    #[test]
    fn crossed_bounds_from_the_server_are_survived() {
        let w = Words::new(Lang::Fr);
        let t = tv_tile(
            &w,
            &body(
                r#"{"success":true,"config":{},"status":{"configured":true,"power":"on","name":"","volume":{"current":12,"min":60,"max":0,"muted":false}}}"#,
            ),
            None,
        );
        let [_, Part::Row(v)] = &t.parts[..] else {
            panic!("{:?}", t.parts)
        };
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn up_but_silent_its_state_is_assumed_two_buttons_instead() {
        let w = Words::new(Lang::Fr);
        let t = tv_tile(
            &w,
            &body(r#"{"success":true,"config":{},"status":{"configured":true,"power":"on"}}"#),
            None,
        );
        assert!(t.warn);
        assert_eq!(t.state, "État inconnu, la TV ne répond plus");
        assert!(matches!(t.lead, Lead::Glyph { .. }));
        let [Part::Row(b)] = &t.parts[..] else {
            panic!()
        };
        assert_eq!(b.len(), 2);
    }

    #[test]
    fn deep_standby_is_said() {
        let w = Words::new(Lang::Fr);
        let t = tv_tile(
            &w,
            &body(
                r#"{"success":true,"config":{},"status":{"configured":true,"power":"deep_standby"}}"#,
            ),
            None,
        );
        assert_eq!(t.state, "Veille profonde");
        assert_eq!(t.fact, None);
        // out of deep standby, its wake is timed; a TV in standby answers soon enough
        let Lead::Toggle { turn_on, .. } = &t.lead else {
            panic!()
        };
        assert!(turn_on.timed);
        assert_eq!(turn_on.limit, limit::TV);
    }

    fn on() -> Value {
        body(
            r#"{"success":true,"config":{},"status":{"configured":true,"power":"on","name":"Philips","volume":{"current":12,"min":0,"max":60,"muted":false},"ambilight":{"power":true,"mode":"lounge","style":"STANDARD"}}}"#,
        )
    }

    fn act_of(b: &Button) -> &Order {
        match &b.act {
            Act::Order(o) | Act::Key(o, _) => o,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_assumed_state_says_its_last_order_under_its_buttons() {
        use chrono::TimeZone;
        let w = Words::new(Lang::Fr);
        let at = maison_api::HOUSE_TZ
            .with_ymd_and_hms(2026, 11, 12, 19, 5, 0)
            .unwrap();
        let assumed =
            body(r#"{"success":true,"config":{},"status":{"configured":true,"power":"on"}}"#);
        let t = tv_tile(&w, &assumed, Some((false, at)));
        let [Part::Row(_), Part::Hint(h)] = &t.parts[..] else {
            panic!("{:?}", t.parts)
        };
        assert_eq!(h, "Dernier ordre\u{a0}: éteindre, à 19:05");
        // its page says why it stopped answering, and has no pad: nothing would hear it
        let g = tv_groups(&w, &assumed, Some((true, at)));
        assert_eq!(g.len(), 2);
    }

    #[test]
    fn the_tv_page_has_its_volume_pad_keys_and_ambilight() {
        let w = Words::new(Lang::Fr);
        let g = tv_groups(&w, &on(), None);
        assert_eq!(g.len(), 4);
        let Body::Tiles(head) = &g[0].body else {
            panic!()
        };
        assert!(head[0].open.is_none());
        // the volume: a slider sent on release (the TV is not chased), steps, mute
        let Body::Tiles(v) = &g[1].body else { panic!() };
        assert_eq!(v[0].name, "Volume 12 sur 60");
        let [Part::Slider(s), Part::Row(keys)] = &v[0].parts[..] else {
            panic!("{:?}", v[0].parts)
        };
        assert!(s.put && !s.live);
        assert_eq!(s.path, "/tv/volume");
        assert_eq!((s.body)(20.0), serde_json::json!({ "level": 20 }));
        assert_eq!(keys.len(), 3);
        let mute = act_of(&keys[2]);
        assert!(mute.posts[0].1.put);
        assert_eq!(
            mute.posts[0].1.body,
            Some(serde_json::json!({ "muted": true }))
        );
        // the pad: each key its own order, arrows repeating, OK not
        let Body::Parts(parts) = &g[2].body else {
            panic!()
        };
        let [Part::Pad(p), Part::Row(media)] = &parts[..] else {
            panic!()
        };
        let up = act_of(&p.up);
        assert_eq!(up.key, "tv:key:cursor_up");
        assert_eq!(up.posts[0].1.path, "/tv/key");
        assert_eq!(
            up.posts[0].1.body,
            Some(serde_json::json!({ "key": "cursor_up" }))
        );
        assert!(up.refresh.is_empty(), "a key reads nothing again");
        assert!(matches!(p.up.act, Act::Key(_, Some(_))));
        assert!(matches!(p.ok.act, Act::Key(_, None)));
        assert_eq!(act_of(&p.ok).key, "tv:key:confirm");
        assert_eq!(p.under.len(), 2);
        assert_eq!(p.under[0].label, "Retour");
        assert_eq!(act_of(&media[0]).key, "tv:key:source");
        assert_eq!(act_of(&media[1]).key, "tv:key:play_pause");
        // its input, Ambilight, the way back to the image
        let Body::Buttons(b) = &g[3].body else {
            panic!()
        };
        assert_eq!(act_of(&b[0]).posts[0].1.path, "/tv/source/box");
        assert!(b[1].primary, "Ambilight lit");
        assert_eq!(act_of(&b[1]).posts[0].1.path, "/tv/ambilight");
        assert_eq!(act_of(&b[2]).posts[0].1.path, "/tv/ambilight/follow-video");
    }

    #[test]
    fn following_the_image_is_only_offered_when_it_drifted() {
        let w = Words::new(Lang::Fr);
        let mut d = on();
        d["status"]["ambilight"]["style"] = "FOLLOW_VIDEO".into();
        let g = tv_groups(&w, &d, None);
        let Body::Buttons(b) = &g[3].body else {
            panic!()
        };
        assert_eq!(b.len(), 2);
    }

    #[test]
    fn the_box_page_has_its_pad_media_and_volume_keys_out_of_reach_when_it_is() {
        let w = Words::new(Lang::Fr);
        let d = body(
            r#"{"success":true,"config":{},"status":{"configured":true,"reachable":true,"awake":true,"model":"LEAP-S1","paired":true}}"#,
        );
        let g = box_groups(&w, &d);
        assert_eq!(g.len(), 3);
        let Body::Parts(parts) = &g[1].body else {
            panic!()
        };
        let [Part::Pad(p), Part::Row(media)] = &parts[..] else {
            panic!()
        };
        assert_eq!(act_of(&p.left).key, "androidtv:key:left");
        assert_eq!(act_of(&p.left).posts[0].1.path, "/androidtv/key");
        assert_eq!(p.under.len(), 3);
        let search = act_of(&media[0]);
        assert_eq!(
            search.posts[0].1.body,
            Some(serde_json::json!({ "key": "search" }))
        );
        assert_eq!(act_of(&media[3]).key, "androidtv:key:next");
        let Body::Parts(v) = &g[2].body else { panic!() };
        let Part::Row(v) = &v[0] else { panic!() };
        assert!(matches!(v[0].act, Act::Key(_, Some(e)) if e == VOLUME_REPEAT));
        assert_eq!(act_of(&v[1]).key, "androidtv:key:volume_up");
        assert!(
            matches!(v[2].act, Act::Key(_, None)),
            "mute does not repeat"
        );
        let mut far = d.clone();
        far["status"]["reachable"] = false.into();
        let g = box_groups(&w, &far);
        let Body::Parts(parts) = &g[1].body else {
            panic!()
        };
        let [Part::Pad(p), Part::Row(media)] = &parts[..] else {
            panic!()
        };
        assert!(p.ok.unavailable && media.iter().all(|b| b.unavailable));
    }

    #[test]
    fn the_box_wakes_and_launches_its_apps() {
        let w = Words::new(Lang::Fr);
        let t = box_tile(
            &w,
            &body(
                r#"{"success":true,"config":{"host":"192.168.1.40"},"status":{"configured":true,"reachable":true,"awake":true,"currentApp":"org.smarttube.beta","model":"LEAP-S1","paired":true}}"#,
            ),
        );
        assert_eq!(t.name, "LEAP-S1");
        assert_eq!(t.state, "Actif");
        let [Part::Row(apps)] = &t.parts[..] else {
            panic!()
        };
        assert_eq!(apps[0].label, "SmartTube");
        assert!(apps[0].primary, "the app in front is marked");
        let Act::Order(o) = &apps[1].act else {
            panic!()
        };
        assert_eq!(
            o.posts[0].1.body.as_ref().unwrap()["package"],
            "studio.kahn.iris.tv"
        );
    }

    #[test]
    fn its_favourites_replace_the_shortcuts_and_out_of_reach_they_wait() {
        let w = Words::new(Lang::Fr);
        let t = box_tile(
            &w,
            &body(
                r#"{"success":true,"config":{"favoriteApps":[{"package":"tv.molotov","label":"Molotov"}]},"status":{"configured":true,"reachable":false,"awake":false,"paired":false}}"#,
            ),
        );
        assert_eq!(t.state, "Injoignable");
        let [Part::Row(apps)] = &t.parts[..] else {
            panic!()
        };
        assert_eq!(apps.len(), 1);
        assert!(apps[0].unavailable);
        assert!(matches!(t.lead, Lead::Toggle { offline: true, .. }));
    }
}
