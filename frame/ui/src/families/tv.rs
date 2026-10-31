//! « Télé » on the dashboard (TvGroup, TvTile, BoxTile, tv/words.ts): the Philips TV and the
//! Android box in one list. The TV's icon (and its row) turns it on or off over infrared; up but
//! JointSPACE silent, its state is assumed (docs/ux.md § 2, case 3): « Allumer » and
//! « Éteindre » instead. The box's icon wakes it or puts it to sleep (the TV follows over CEC),
//! its app chips launch an app, the TV turned on first. Both read as slowly as the TV allows.

use maison_api::androidtv::{LaunchRequest, SHORTCUTS};
use maison_api::remote::SwitchState;
use maison_api::tv::{PowerRequest, VolumeRequest};
use serde_json::Value;

use super::{BOX, Cx, TV, loaded, stale};
use crate::commands::{Order, Outcome, Post, Say, Toast, limit};
use crate::i18n::{Words, k};
use crate::paths;
use crate::ui::icon;
use crate::ui::model::{Act, Body, Button, Group, Lead, Part, Tile};

/// Up, JointSPACE silent: the backend says « on » and reads nothing else (`assumedOn`).
pub fn assumed_on(status: &Value) -> bool {
    status["power"] == "on" && status["volume"].is_null()
}

/// The TV's power order: `on` or `off`, the box's input taken when it comes on.
fn power(name: &str, on: bool) -> Order {
    Order::one(
        format!("{TV}:power"),
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

/// The TV's row, from `/tv` (`{config, status}`).
pub fn tv_tile(w: &Words, data: &Value) -> Tile {
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
    let (lead, parts) = if configured && !assumed {
        (
            Lead::Toggle {
                icon: icon::TV,
                on,
                offline: false,
                turn_on: Box::new(power(&name, true)),
                turn_off: Box::new(power(&name, false)),
            },
            volume_steps(w, &name, volume).into_iter().collect(),
        )
    } else {
        let parts = if configured {
            vec![Part::Row(vec![
                Button::new(
                    Some(icon::POWER),
                    w.s(k::ACTION_TURN_ON),
                    Act::Order(power(&name, true)),
                ),
                Button::new(
                    Some(icon::POWER),
                    w.s(k::ACTION_TURN_OFF),
                    Act::Order(power(&name, false)),
                ),
            ])]
        } else {
            Vec::new()
        };
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
    Tile::new(name.clone(), state, lead).parts(if configured {
        vec![Part::Row(apps)]
    } else {
        Vec::new()
    })
}

pub fn group(cx: &mut Cx) -> Option<Group> {
    let tv = cx.live.want(&cx.sources.tv);
    let boxed = cx.live.want(&cx.sources.androidtv);
    let w = cx.words;
    let mut tiles = Vec::new();
    // each reads on its own; the group shows what came, its place kept until both did
    if let Some(d) = tv.data.as_deref() {
        tiles.push(tv_tile(w, d));
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
        let [Part::Row(v)] = &t.parts[..] else {
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
        );
        let [Part::Row(v)] = &t.parts[..] else {
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
        );
        assert_eq!(t.state, "Veille profonde");
        assert_eq!(t.fact, None);
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
