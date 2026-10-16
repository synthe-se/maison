//! The cats' corner on the dashboard (CatsGroup, CatTile, cats/alerts.ts, cats/tokens.ts): the
//! feeder, the fountain and the litter box, each a row whose line says what matters, what
//! needs a hand in the warning style (« Eau basse », « Litière à moitié pleine »), then what it
//! is doing (« Propre »), « En ligne » only when there is nothing to say, « Hors ligne » when
//! its local session is down. The feeder gives one portion from its row; offline, out of reach.

use serde_json::{Value, json};

use super::{Cx, TUYA, loaded, stale};
use crate::commands::{Order, Outcome, Post, Toast};
use crate::i18n::{Arg, Words, k};
use crate::paths;
use crate::ui::icon::{self, Icon};
use crate::ui::model::{Act, Body, Button, Group, Lead, Part, Tile};

/// A device's kind as the backend says it (`type`), its status path's segment too.
pub fn icon_of(kind: &str) -> Icon {
    match kind {
        "feeder" => icon::UTENSILS,
        "fountain" => icon::DROPLETS,
        "litter-box" => icon::TRASH,
        _ => icon::POWER,
    }
}

/// What a device's status has to say (`statusLine`), and whether it needs a hand; `None`
/// when it has nothing to say. The feeder reports no fault it can be trusted with.
pub fn status_line(w: &Words, kind: &str, s: &Value) -> Option<(String, bool)> {
    match kind {
        "fountain" if s["waterLevel"] == "low" => Some((w.s(k::CATS_WATER_LOW), true)),
        "litter-box" => {
            if let Some(code) = s["sensors"]["faultAlarm"].as_u64().filter(|c| *c != 0) {
                return Some((
                    w.t(
                        k::LITTER_BOX_FAULT_ALARM,
                        &[("code", Arg::Count(code as f64))],
                    ),
                    true,
                ));
            }
            if s["system"]["maintenanceRequired"].as_bool() == Some(true) {
                return Some((w.s(k::LITTER_BOX_MAINTENANCE_REQUIRED), true));
            }
            if s["sensors"]["litterLevel"] == "half" {
                return Some((w.s(k::CATS_LITTER_HALF), true));
            }
            match s["system"]["state"].as_str() {
                Some("cleaning") => Some((w.s(k::LITTER_BOX_STATUS_CLEANING), false)),
                Some("cat_inside") => Some((w.s(k::LITTER_BOX_STATUS_CAT_INSIDE), false)),
                // the device's own spelling of « standby »: cleaned, waiting for the next visit
                Some("satnd_by") => Some((w.s(k::CATS_CLEAN), false)),
                _ => None,
            }
        }
        _ => None,
    }
}

fn served_said(w: &Words, o: &Outcome) -> Option<Toast> {
    o.failed.is_empty().then(|| Toast {
        text: w.t(k::FEEDER_PORTIONS_DISTRIBUTED, &[crate::i18n::count(1)]),
        warn: false,
    })
}

/// One device's row, from its entry in `/devices` and its parsed status (once read).
pub fn tile(w: &Words, device: &Value, status: Option<&Value>) -> Tile {
    let id = device["id"].as_str().unwrap_or_default();
    let kind = device["type"].as_str().unwrap_or("unknown");
    let name = device["name"].as_str().unwrap_or(id).to_string();
    let connected = device["connected"].as_bool().unwrap_or(false);
    let (state, warn) = if !connected {
        (w.s(k::DEVICE_OFFLINE), true)
    } else {
        status
            .and_then(|s| status_line(w, kind, s))
            .unwrap_or_else(|| (w.s(k::DEVICE_ONLINE), false))
    };
    let parts = if kind == "feeder" {
        let feed = Order::one(
            format!("{TUYA}{id}:feed"),
            name.clone(),
            Post::json(
                paths::of(paths::DEVICES, id, "feeder/feed"),
                &json!({ "portion": 1 }),
            ),
        )
        .then_refresh(&[TUYA])
        .said(served_said);
        vec![Part::Row(vec![
            Button::new(
                Some(icon::UTENSILS),
                w.s(k::FEEDER_ONE_PORTION),
                Act::Order(feed),
            )
            .unavailable(!connected),
        ])]
    } else {
        Vec::new()
    };
    Tile {
        name,
        state,
        warn,
        lead: Lead::Glyph {
            icon: icon_of(kind),
            on: connected,
        },
        fact: None,
        parts,
        open: None,
    }
}

pub fn group(cx: &mut Cx) -> Option<Group> {
    let list = cx.live.want(&cx.sources.cats);
    let devices: Vec<Value> = list
        .data
        .as_deref()
        .and_then(|d| d["devices"].as_array().cloned())
        .unwrap_or_default();
    // each device's status, read while its row is shown
    let mut statuses = Vec::with_capacity(devices.len());
    for d in &devices {
        let (id, kind) = (
            d["id"].as_str().unwrap_or_default(),
            d["type"].as_str().unwrap_or("unknown"),
        );
        let status = if kind == "unknown" || id.is_empty() {
            None
        } else {
            let source = cx.sources.cat_status(id, kind).clone();
            cx.live.want(&source).data.as_deref().cloned()
        };
        statuses.push(status);
    }
    let w = cx.words;
    let body = loaded(
        w,
        &list,
        &[TUYA],
        (
            &w.s(k::DASHBOARD_NO_DEVICES),
            &w.s(k::DASHBOARD_NO_DEVICES_HINT),
        ),
        |_| {
            (!devices.is_empty()).then(|| {
                Body::Tiles(
                    devices
                        .iter()
                        .zip(&statuses)
                        .map(|(d, s)| tile(w, d, s.as_ref()))
                        .collect(),
                )
            })
        },
    );
    Some(Group {
        title: w.s(k::DASHBOARD_CATS_TITLE),
        fact: list.data.as_deref().map(|d| {
            w.t(
                k::DASHBOARD_DEVICE_COUNT,
                &[crate::i18n::count(
                    d["total"].as_u64().unwrap_or(devices.len() as u64) as usize,
                )],
            )
        }),
        actions: Vec::new(),
        stale: stale(cx, &list, &[TUYA]),
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;

    fn v(json: &str) -> Value {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn what_needs_a_hand_first_then_what_it_does_else_online() {
        let w = Words::new(Lang::Fr);
        let fountain =
            v(r#"{"id":"f1","name":"Pixi Smart Fountain","type":"fountain","connected":true}"#);
        let low = tile(&w, &fountain, Some(&v(r#"{"waterLevel":"low"}"#)));
        assert_eq!((low.state.as_str(), low.warn), ("Eau basse", true));
        let ok = tile(&w, &fountain, Some(&v(r#"{"waterLevel":"ok"}"#)));
        assert_eq!((ok.state.as_str(), ok.warn), ("En ligne", false));
        let litter =
            v(r#"{"id":"l1","name":"Cat Litter Box","type":"litter-box","connected":true}"#);
        let clean = tile(&w, &litter, Some(&v(r#"{"system":{"state":"satnd_by"}}"#)));
        assert_eq!(clean.state, "Propre");
        let half = tile(
            &w,
            &litter,
            Some(&v(
                r#"{"sensors":{"litterLevel":"half"},"system":{"state":"satnd_by"}}"#,
            )),
        );
        assert_eq!(
            (half.state.as_str(), half.warn),
            ("Litière à moitié pleine", true)
        );
    }

    #[test]
    fn the_feeder_serves_one_portion_unless_offline() {
        let w = Words::new(Lang::Fr);
        let on = tile(
            &w,
            &v(r#"{"id":"feeder-1","name":"Pixi Smart Feeder","type":"feeder","connected":true}"#),
            None,
        );
        let [Part::Row(b)] = &on.parts[..] else {
            panic!()
        };
        assert_eq!(b[0].label, "1 portion");
        let Act::Order(o) = &b[0].act else { panic!() };
        assert_eq!(o.posts[0].1.path, "/devices/feeder-1/feeder/feed");
        let off = tile(
            &w,
            &v(r#"{"id":"feeder-1","name":"Pixi Smart Feeder","type":"feeder","connected":false}"#),
            None,
        );
        assert_eq!((off.state.as_str(), off.warn), ("Hors ligne", true));
        let [Part::Row(b)] = &off.parts[..] else {
            panic!()
        };
        assert!(b[0].unavailable);
    }
}
