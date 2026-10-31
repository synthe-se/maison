//! The cats' corner on the dashboard (CatsGroup, CatTile, cats/alerts.ts, cats/tokens.ts): the
//! feeder, the fountain and the litter box, each a row whose line says what matters, what
//! needs a hand in the warning style (« Eau basse », « Litière à moitié pleine »), then what it
//! is doing (« Propre »), « En ligne » only when there is nothing to say, « Hors ligne » when
//! its local session is down. The feeder gives one portion from its row; offline, out of reach.
//! A row opens its device's page (routes/device/[deviceId], FeederPanel, FountainPanel,
//! LitterBoxPanel): its local connection, what it reports, and what a passer-by does there (clean
//! the litter now, say it was refilled, a fountain's counters reset, a portion). Its settings
//! (night hours, preferences, meal plans) stay the web app's.

use std::collections::BTreeSet;
use std::sync::Mutex;

use chrono::{DateTime, TimeZone};
use maison_api::Tz;
use maison_api::cats::{
    FeedRequest, FountainUvSettingsRequest, LitterActions, LitterBoxSettingsRequest,
};
use maison_api::lamps::PowerRequest;
use serde_json::Value;

use super::{Cx, TUYA, loaded, stale};
use crate::commands::{Order, Outcome, Post, Say, Toast, json};
use crate::i18n::{Arg, Words, count, k};
use crate::paths;
use crate::ui::icon::{self, Icon};
use crate::ui::model::{Act, Body, Button, Group, Lead, Part, Screen, Tile, View};

/// The litter box waiting between two visits, in the device's own spelling: cleaned.
const STANDBY: &str = "satnd_by";

/// What the devices report, each token in words (tokens.ts): the litter box's state and level,
/// the fountain's water, what powers the feeder.
const LITTER_STATE: &[(&str, &str)] = &[
    ("cleaning", k::LITTER_BOX_STATUS_CLEANING),
    ("cat_inside", k::LITTER_BOX_STATUS_CAT_INSIDE),
    ("clumping", k::LITTER_BOX_STATUS_CLUMPING),
    (STANDBY, k::STATE_STANDBY),
];
const LITTER_LEVEL: &[(&str, &str)] = &[
    ("full", k::LITTER_BOX_FILLED),
    ("half", k::LITTER_BOX_HALF_FILLED),
];
const WATER_LEVEL: &[(&str, &str)] = &[
    ("low", k::DEVICE_LEVEL_LOW),
    ("medium", k::DEVICE_LEVEL_MEDIUM),
    ("ok", k::FOUNTAIN_WATER_LEVEL_OK),
];
const FEEDER_POWER: &[(&str, &str)] = &[
    ("AC Power", k::FEEDER_POWER_MAINS),
    ("Battery", k::FEEDER_POWER_BATTERY),
    ("Unknown", k::COMMON_UNKNOWN),
];

/// The tokens a device said that the frame does not know, logged once each.
static TOLD: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// `token` in the words of `table` (tokens.ts `wordOf`); one the frame does not know is
/// « Inconnu », logged once, never shown raw.
fn word_of(w: &Words, table: &[(&str, &str)], token: Option<&str>) -> String {
    let Some(token) = token else {
        return w.s(k::COMMON_UNKNOWN);
    };
    if let Some((_, key)) = table.iter().find(|(t, _)| *t == token) {
        return w.s(key);
    }
    let mut told = TOLD.lock().unwrap_or_else(|e| e.into_inner());
    if told.insert(token.to_string()) {
        eprintln!("a device said « {token} », which the frame does not know");
    }
    w.s(k::COMMON_UNKNOWN)
}

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
                Some(STANDBY) => Some((w.s(k::CATS_CLEAN), false)),
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

/// One portion now, from the feeder's row or its page.
fn feed(id: &str, name: &str) -> Order {
    Order::one(
        format!("{TUYA}{id}:feed"),
        name,
        Post::json(
            paths::of(paths::DEVICES, id, "feeder/feed"),
            &FeedRequest { portion: 1 },
        ),
    )
    .then_refresh(&[TUYA])
    .said(Say::With(served_said))
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
        vec![Part::Row(vec![
            Button::new(
                Some(icon::UTENSILS),
                w.s(k::FEEDER_ONE_PORTION),
                Act::Order(feed(id, &name)),
            )
            .unavailable(!connected),
        ])]
    } else {
        Vec::new()
    };
    // the litter's level at the row's end, seen without opening its page (« Remplie »)
    let level = status
        .filter(|_| connected && kind == "litter-box")
        .and_then(|s| s["sensors"]["litterLevel"].as_str())
        .map(|l| word_of(w, LITTER_LEVEL, Some(l)));
    Tile::new(
        name,
        state,
        Lead::Glyph {
            icon: icon_of(kind),
            on: connected,
        },
    )
    .warn(warn)
    .fact(level)
    .parts(parts)
    .open((kind != "unknown" && !id.is_empty()).then(|| Act::Open(View::Device(id.to_string()))))
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
    let fact = list.data.as_deref().map(|d| {
        w.t(
            k::DASHBOARD_DEVICE_COUNT,
            &[crate::i18n::count(
                d["total"].as_u64().unwrap_or(devices.len() as u64) as usize,
            )],
        )
    });
    Some(
        Group::new(w.s(k::DASHBOARD_CATS_TITLE), body)
            .fact(fact)
            .stale(stale(cx, &list, &[TUYA])),
    )
}

/// A fact of a device's page (StatusRow): what it is, its value, in the warning style when it
/// needs a hand; its controls under it.
fn fact(icon: Icon, label: &str, value: String, warn: bool, parts: Vec<Part>) -> Tile {
    Tile::new(label, value, Lead::Glyph { icon, on: false })
        .warn(warn)
        .parts(parts)
}

/// A device's order from its page: `action` under its path, `body` if any; its key under the
/// device's (`tuya:<id>:clean`), every cats' source read again once done.
fn order(id: &str, name: &str, key: &str, action: &str, body: Option<Value>, say: Say) -> Order {
    let path = paths::of(paths::DEVICES, id, action);
    Order::one(
        format!("{TUYA}{id}:{key}"),
        name,
        match body {
            Some(b) => Post::json(path, &b),
            None => Post::empty(path),
        },
    )
    .then_refresh(&[TUYA])
    .said(say)
}

/// The device's local connection (connection.ts): a switch, as on its web page; switched off
/// then on, a session that hangs starts again.
fn connection(w: &Words, id: &str, name: &str, connected: bool) -> Tile {
    fn connecting(w: &Words, o: &Outcome) -> Option<Toast> {
        let name = o.done.first()?;
        Some(Toast {
            text: w.t(
                k::DEVICE_CONNECTION_INITIATED_DESCRIPTION,
                &[("name", name.into())],
            ),
            warn: false,
        })
    }
    fn disconnected(w: &Words, o: &Outcome) -> Option<Toast> {
        let name = o.done.first()?;
        Some(Toast {
            text: w.t(k::DEVICE_DISCONNECTED_DESCRIPTION, &[("name", name.into())]),
            warn: false,
        })
    }
    let toggle = |on: bool| {
        let (action, say) = if on {
            ("connect", Say::With(connecting))
        } else {
            ("disconnect", Say::With(disconnected))
        };
        Box::new(order(id, name, "connection", action, None, say).toggle(on))
    };
    Tile::new(
        w.s(k::DEVICE_LOCAL_CONNECTION),
        w.s(if connected {
            k::DEVICE_ONLINE
        } else {
            k::DEVICE_OFFLINE
        }),
        Lead::Toggle {
            icon: icon::WIFI,
            on: connected,
            offline: false,
            turn_on: toggle(true),
            turn_off: toggle(false),
        },
    )
    .warn(!connected)
    .parts(vec![Part::Hint(w.s(k::DEVICE_LOCAL_CONNECTION_HINT))])
}

/// The litter box's page (LitterBoxPanel): clean now, its state, its level and « J'ai rempli
/// la litière », what needs a hand. Offline, « Lancer le nettoyage » stays, out of reach, said
/// why; refilling waits for the connection.
pub fn litter_tiles(w: &Words, id: &str, name: &str, connected: bool, s: &Value) -> Vec<Tile> {
    let clean = Button::new(
        Some(icon::TRASH),
        w.s(k::LITTER_BOX_START_CLEANING),
        Act::Order(order(
            id,
            name,
            "clean",
            "litter-box/clean",
            None,
            Say::Done(k::LITTER_BOX_CLEANING_STARTED),
        )),
    )
    .primary(true)
    .unavailable(!connected);
    let mut state = vec![Part::Row(vec![clean])];
    if !connected {
        state.push(Part::Hint(w.s(k::LITTER_BOX_OFFLINE_REASON)));
    }
    let mut tiles = vec![fact(
        icon::CLOCK,
        &w.s(k::COMMON_STATUS),
        word_of(w, LITTER_STATE, s["system"]["state"].as_str()),
        false,
        state,
    )];
    if let Some(level) = s["sensors"]["litterLevel"].as_str() {
        let half = level == "half";
        let mut parts = Vec::new();
        if half {
            parts.push(Part::Hint(w.s(k::LITTER_BOX_FILL_SOON)));
        }
        if connected {
            let refilled = order(
                id,
                name,
                "level",
                "litter-box/settings",
                Some(json(&LitterBoxSettingsRequest {
                    actions: Some(LitterActions {
                        reset_sand_level: Some(true),
                        ..LitterActions::default()
                    }),
                    ..LitterBoxSettingsRequest::default()
                })),
                Say::Done(k::DEVICE_SETTING_SAVED),
            );
            parts.push(Part::Row(vec![Button::new(
                Some(icon::REFRESH_CW),
                w.s(k::LITTER_BOX_RESET_LITTER_LEVEL),
                Act::Order(refilled),
            )]));
        }
        tiles.push(fact(
            icon::GAUGE,
            &w.s(k::LITTER_BOX_LITTER_LEVEL),
            word_of(w, LITTER_LEVEL, Some(level)),
            half,
            parts,
        ));
    }
    if s["system"]["maintenanceRequired"].as_bool() == Some(true) {
        tiles.push(fact(
            icon::SETTINGS,
            &w.s(k::COMMON_STATUS),
            w.s(k::LITTER_BOX_MAINTENANCE_REQUIRED),
            true,
            Vec::new(),
        ));
    }
    if let Some(code) = s["sensors"]["faultAlarm"].as_u64().filter(|c| *c != 0) {
        tiles.push(fact(
            icon::TRIANGLE_ALERT,
            &w.s(k::COMMON_ERROR),
            w.t(
                k::LITTER_BOX_FAULT_ALARM,
                &[("code", Arg::Count(code as f64))],
            ),
            true,
            Vec::new(),
        ));
    }
    tiles
}

/// The fountain's switches (FountainPanel): running, UV.
pub fn fountain_switches(w: &Words, id: &str, name: &str, connected: bool, s: &Value) -> Vec<Tile> {
    let switch = |label: &str,
                  icon,
                  is_on: bool,
                  state: String,
                  key: &str,
                  body: fn(bool) -> Value,
                  says: [&'static str; 2]| {
        let to = |target: bool| {
            Box::new(
                order(
                    id,
                    name,
                    key,
                    &format!("fountain/{key}"),
                    Some(body(target)),
                    Say::Done(says[usize::from(!target)]),
                )
                .toggle(target),
            )
        };
        Tile::new(
            label,
            state,
            Lead::Toggle {
                icon,
                on: is_on,
                offline: !connected,
                turn_on: to(true),
                turn_off: to(false),
            },
        )
    };
    let on_off = |yes: bool| w.s(if yes { k::STATE_ON } else { k::STATE_OFF });
    let power = s["power"].as_bool().unwrap_or(false);
    let runtime = s["uvRuntime"].as_u64().unwrap_or(0);
    let uv = runtime > 0 || s["uv"].as_bool().unwrap_or(false);
    let uv_state = if uv && runtime > 0 {
        w.t(
            k::FOUNTAIN_UV_RUNTIME,
            &[("time", w.duration(runtime).into())],
        )
    } else {
        on_off(uv)
    };
    vec![
        switch(
            &w.s(k::FOUNTAIN_POWER),
            icon::POWER,
            power,
            on_off(power),
            "power",
            |enabled| json(&PowerRequest { enabled }),
            [k::FOUNTAIN_FOUNTAIN_ON, k::FOUNTAIN_FOUNTAIN_OFF],
        ),
        switch(
            &w.s(k::FOUNTAIN_UV_STERILIZATION),
            icon::SUN,
            uv,
            uv_state,
            "uv",
            |on| {
                json(&FountainUvSettingsRequest {
                    enabled: Some(on),
                    runtime: None,
                })
            },
            [k::FOUNTAIN_UV_ON, k::FOUNTAIN_UV_OFF],
        ),
    ]
}

/// The fountain's upkeep (FountainPanel): its water, then each counter and its reset.
pub fn fountain_upkeep(w: &Words, id: &str, name: &str, connected: bool, s: &Value) -> Vec<Tile> {
    let counters: [(&str, &str, Icon, &str, &str, &'static str); 3] = [
        (
            "water",
            "waterTime",
            icon::DROPLETS,
            k::FOUNTAIN_FRESH_WATER,
            k::FOUNTAIN_WATER_CHANGED,
            k::FOUNTAIN_WATER_COUNTER_RESET,
        ),
        (
            "filter",
            "filterLife",
            icon::FUNNEL,
            k::FOUNTAIN_FILTER_TIME,
            k::FOUNTAIN_RESET_FILTER_CHANGED,
            k::FOUNTAIN_FILTER_COUNTER_RESET,
        ),
        (
            "pump",
            "pumpTime",
            icon::GAUGE,
            k::FOUNTAIN_PUMP_TIME,
            k::FOUNTAIN_RESET_PUMP_CLEANED,
            k::FOUNTAIN_PUMP_COUNTER_RESET,
        ),
    ];
    let low = s["waterLevel"] == "low";
    let mut tiles = vec![fact(
        icon::DROPLETS,
        &w.s(k::FOUNTAIN_WATER_LEVEL),
        word_of(w, WATER_LEVEL, s["waterLevel"].as_str()),
        low,
        if low {
            vec![Part::Hint(w.s(k::FOUNTAIN_ADD_WATER))]
        } else {
            Vec::new()
        },
    )];
    for (key, field, icon, label, button, done) in counters {
        let value = s[field].as_u64().map_or_else(
            || w.s(k::COMMON_UNKNOWN),
            |minutes| w.duration(minutes * 60),
        );
        let reset = order(
            id,
            name,
            key,
            &format!("fountain/reset/{key}"),
            None,
            Say::Done(done),
        );
        let parts = if connected {
            vec![Part::Row(vec![Button::new(
                Some(icon::REFRESH_CW),
                w.s(button),
                Act::Order(reset),
            )])]
        } else {
            Vec::new()
        };
        tiles.push(fact(icon, &w.s(label), value, false, parts));
    }
    tiles
}

/// The feeder's last meal in words (« 2 portions, 08:00 »), from its report; `None` without one
/// (alerts.ts `lastMeal`).
pub fn last_meal(w: &Words, s: &Value, now: &DateTime<Tz>) -> Option<String> {
    let p = &s["history"]["parsed"];
    let count_of = p["count"].as_str()?.parse::<usize>().ok()?;
    let at = p["timestamp"]
        .as_str()?
        .parse::<i64>()
        .ok()
        .filter(|t| *t > 0)?;
    let at = now.timezone().timestamp_opt(at, 0).single()?;
    Some(w.t(
        k::FEEDER_LAST_MEAL_VALUE,
        &[
            (
                "portions",
                w.t(k::FEEDER_PORTION, &[count(count_of)]).into(),
            ),
            ("when", w.when(&at, now).into()),
        ],
    ))
}

/// The feeder's report (FeederPanel): what powers it, its last meal.
pub fn feeder_facts(w: &Words, s: &Value, now: &DateTime<Tz>) -> Vec<Tile> {
    let mut tiles = Vec::new();
    if let Some(power) = s["system"]["poweredBy"].as_str() {
        tiles.push(fact(
            icon::PLUG,
            &w.s(k::FEEDER_POWER_SOURCE),
            word_of(w, FEEDER_POWER, Some(power)),
            false,
            Vec::new(),
        ));
    }
    tiles.push(fact(
        icon::CLOCK,
        &w.s(k::FEEDER_LAST_MEAL),
        last_meal(w, s, now).unwrap_or_else(|| w.s(k::COMMON_UNKNOWN)),
        false,
        Vec::new(),
    ));
    tiles
}

/// A cat device's page (routes/device/[deviceId]): its name for a title, its local connection,
/// then its kind's groups, each read from its status. The list unread says so with a retry,
/// never « not found ».
pub fn page(cx: &mut Cx, id: &str) -> Screen {
    let list = cx.live.want(&cx.sources.cats);
    let device = list.data.as_deref().and_then(|d| {
        d["devices"]
            .as_array()?
            .iter()
            .find(|d| d["id"] == id)
            .cloned()
    });
    let w = cx.words;
    let back = Box::new(Button::new(
        Some(icon::ARROW_LEFT),
        w.s(k::BACK_HOME),
        Act::Open(View::House),
    ));
    let Some(device) = device else {
        let title = if list.data.is_some() {
            k::DEVICE_NOT_FOUND
        } else if list.failure.is_some() {
            k::DEVICE_TITLE
        } else {
            k::COMMON_LOADING
        };
        let missing = Group::new(
            w.s(k::DEVICE_TITLE),
            loaded(
                w,
                &list,
                &[TUYA],
                (k::DEVICE_NOT_FOUND, k::DEVICE_NOT_FOUND_DESCRIPTION),
                |_| None,
            ),
        )
        .stale(stale(cx, &list, &[TUYA]));
        return Screen::Page {
            title: w.s(title),
            back,
            groups: vec![missing],
        };
    };
    let kind = device["type"].as_str().unwrap_or("unknown");
    let name = device["name"].as_str().unwrap_or(id).to_string();
    let connected = device["connected"].as_bool().unwrap_or(false);
    let about = [
        device["productName"].as_str().filter(|p| !p.is_empty()),
        device["ip"].as_str(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    let type_label = match kind {
        "feeder" => k::DEVICE_TYPES_FEEDER,
        "fountain" => k::DEVICE_TYPES_FOUNTAIN,
        "litter-box" => k::DEVICE_TYPES_LITTER_BOX,
        _ => k::COMMON_UNKNOWN,
    };
    let mut groups = vec![
        Group::new(
            w.s(type_label),
            Body::Tiles(vec![connection(w, id, &name, connected)]),
        )
        .fact((!about.is_empty()).then_some(about))
        .stale(stale(cx, &list, &[TUYA])),
    ];
    if kind == "unknown" {
        groups.push(Group::new(
            w.s(k::DEVICE_TITLE),
            Body::Empty {
                text: w.s(k::DEVICE_UNKNOWN_TYPE),
                hint: String::new(),
            },
        ));
        return Screen::Page {
            title: name,
            back,
            groups,
        };
    }
    let source = cx.sources.cat_status(id, kind).clone();
    let st = cx.live.want(&source);
    let now = cx.house_now();
    let w = cx.words;
    let group = |title: &str, tiles: &dyn Fn(&Value) -> Vec<Tile>, none: (&str, &str)| {
        Group::new(
            w.s(title),
            loaded(w, &st, &[TUYA], none, |s| {
                let tiles = tiles(s);
                (!s.is_null() && !tiles.is_empty()).then_some(Body::Tiles(tiles))
            }),
        )
        .stale(stale(cx, &st, &[TUYA]))
    };
    let none = (k::FEEDER_STATUS_FETCH_ERROR, "");
    match kind {
        "litter-box" => groups.push(group(
            k::LITTER_BOX_LITTER_STATUS,
            &|s| litter_tiles(w, id, &name, connected, s),
            none,
        )),
        "fountain" => {
            groups.push(group(
                k::COMMON_SETTINGS,
                &|s| fountain_switches(w, id, &name, connected, s),
                none,
            ));
            groups.push(group(
                k::FOUNTAIN_STATUS_MAINTENANCE,
                &|s| fountain_upkeep(w, id, &name, connected, s),
                none,
            ));
        }
        _ => {
            // its row as on the dashboard, its offline reason said in full
            let mut serve = tile(w, &device, None);
            serve.open = None;
            if !connected {
                serve.state = w.s(k::CATS_OFFLINE_REASON);
            }
            groups.push(Group::new(
                w.s(k::FEEDER_MANUAL_DISTRIBUTION),
                Body::Tiles(vec![serve]),
            ));
            groups.push(group(
                k::FEEDER_FEEDER_STATUS,
                &|s| feeder_facts(w, s, &now),
                none,
            ));
        }
    }
    Screen::Page {
        title: name,
        back,
        groups,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;
    use serde_json::json;

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
        assert_eq!((clean.state.as_str(), clean.fact), ("Propre", None));
        let full = tile(
            &w,
            &litter,
            Some(&v(
                r#"{"sensors":{"litterLevel":"full"},"system":{"state":"satnd_by"}}"#,
            )),
        );
        assert_eq!(
            (full.state.as_str(), full.fact.as_deref()),
            ("Propre", Some("Remplie")),
            "the level at the row's end"
        );
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

    fn order_of(b: &Button) -> &Order {
        let Act::Order(o) = &b.act else {
            panic!("an order")
        };
        o
    }

    #[test]
    fn a_row_opens_its_device_page() {
        let w = Words::new(Lang::Fr);
        let litter = tile(
            &w,
            &v(r#"{"id":"l1","name":"Litière","type":"litter-box","connected":true}"#),
            None,
        );
        assert!(matches!(&litter.open, Some(Act::Open(View::Device(id))) if id == "l1"));
        assert_eq!(View::Device("l1".into()).key(), "open:device:l1");
        let unknown = tile(&w, &v(r#"{"id":"u","name":"?","type":"unknown"}"#), None);
        assert!(unknown.open.is_none());
    }

    #[test]
    fn the_litter_box_page_cleans_says_its_level_and_takes_a_refill() {
        let w = Words::new(Lang::Fr);
        let s = v(r#"{"sensors":{"litterLevel":"half"},"system":{"state":"satnd_by"}}"#);
        let tiles = litter_tiles(&w, "l1", "Litière", true, &s);
        assert_eq!(
            (tiles[0].name.as_str(), tiles[0].state.as_str()),
            ("Statut", "En veille")
        );
        let [Part::Row(b)] = &tiles[0].parts[..] else {
            panic!("the clean button alone")
        };
        let clean = order_of(&b[0]);
        assert!(b[0].primary && !b[0].unavailable);
        assert_eq!(clean.key, "tuya:l1:clean");
        assert_eq!(clean.posts[0].1.path, "/devices/l1/litter-box/clean");
        assert_eq!(clean.refresh, [TUYA]);
        let level = &tiles[1];
        assert_eq!((level.state.as_str(), level.warn), ("À moitié", true));
        let [Part::Hint(soon), Part::Row(b)] = &level.parts[..] else {
            panic!("a hint, then « J'ai rempli la litière »")
        };
        assert_eq!(soon, "Pense à la remplir bientôt");
        let refill = order_of(&b[0]);
        assert_eq!(refill.posts[0].1.path, "/devices/l1/litter-box/settings");
        assert_eq!(
            refill.posts[0].1.body,
            Some(json!({ "actions": { "resetSandLevel": true } }))
        );
        // what needs a hand, each on its own row
        let worn = v(
            r#"{"sensors":{"litterLevel":"full","faultAlarm":4},"system":{"state":"cleaning","maintenanceRequired":true}}"#,
        );
        let tiles = litter_tiles(&w, "l1", "Litière", true, &worn);
        assert_eq!(tiles.len(), 4);
        assert!(tiles[2].warn && tiles[3].warn);
        assert_eq!(tiles[3].state, "Alarme de défaut\u{a0}: 4");
    }

    #[test]
    fn offline_the_litter_box_keeps_its_clean_button_out_of_reach_and_says_why() {
        let w = Words::new(Lang::Fr);
        let s = v(r#"{"sensors":{"litterLevel":"full"},"system":{"state":"strange"}}"#);
        let tiles = litter_tiles(&w, "l1", "Litière", false, &s);
        assert_eq!(tiles[0].state, "Inconnu", "a token the frame does not know");
        let [Part::Row(b), Part::Hint(why)] = &tiles[0].parts[..] else {
            panic!("the button, then why")
        };
        assert!(b[0].unavailable);
        assert_eq!(why, &w.s(k::LITTER_BOX_OFFLINE_REASON));
        assert!(
            tiles[1].parts.is_empty(),
            "refilling waits for the connection"
        );
    }

    #[test]
    fn the_connection_is_a_switch() {
        let w = Words::new(Lang::Fr);
        let t = connection(&w, "l1", "Litière", false);
        assert_eq!((t.state.as_str(), t.warn), ("Hors ligne", true));
        let Lead::Toggle {
            on,
            turn_on,
            turn_off,
            ..
        } = &t.lead
        else {
            panic!("a switch")
        };
        assert!(!on);
        assert_eq!(turn_on.posts[0].1.path, "/devices/l1/connect");
        assert_eq!(turn_off.posts[0].1.path, "/devices/l1/disconnect");
        assert_eq!(turn_on.key, turn_off.key);
        let said = turn_on.say.unwrap().toast(
            &w,
            &Outcome {
                done: vec!["Litière".into()],
                failed: Vec::new(),
                answer: None,
            },
        );
        assert_eq!(said.unwrap().text, "Connexion à Litière…");
    }

    #[test]
    fn the_fountain_page_switches_and_resets_its_counters() {
        let w = Words::new(Lang::Fr);
        let s =
            v(r#"{"power":true,"uv":false,"uvRuntime":600,"waterLevel":"low","filterLife":2400}"#);
        let sw = fountain_switches(&w, "f1", "Fontaine", true, &s);
        let Lead::Toggle { on, turn_off, .. } = &sw[0].lead else {
            panic!()
        };
        assert!(on);
        assert_eq!(turn_off.posts[0].1.path, "/devices/f1/fountain/power");
        assert_eq!(turn_off.posts[0].1.body, Some(json!({ "enabled": false })));
        let Lead::Toggle { on, turn_off, .. } = &sw[1].lead else {
            panic!()
        };
        assert!(on, "UV counting down is UV on");
        assert_eq!(turn_off.posts[0].1.path, "/devices/f1/fountain/uv");
        assert_eq!(sw[1].state, "Encore 10\u{a0}min");
        let up = fountain_upkeep(&w, "f1", "Fontaine", true, &s);
        assert_eq!((up[0].state.as_str(), up[0].warn), ("Bas", true));
        assert_eq!(up[2].name, "Filtre");
        assert_eq!(up[2].state, w.duration(2400 * 60));
        let [Part::Row(b)] = &up[2].parts[..] else {
            panic!()
        };
        assert_eq!(
            order_of(&b[0]).posts[0].1.path,
            "/devices/f1/fountain/reset/filter"
        );
        assert!(
            fountain_upkeep(&w, "f1", "Fontaine", false, &s)[1]
                .parts
                .is_empty()
        );
    }

    #[test]
    fn the_feeder_says_its_power_and_its_last_meal() {
        use maison_api::HOUSE_TZ;
        let w = Words::new(Lang::Fr);
        let now = HOUSE_TZ.with_ymd_and_hms(2026, 10, 2, 9, 0, 0).unwrap();
        let at = HOUSE_TZ
            .with_ymd_and_hms(2026, 10, 2, 8, 0, 0)
            .unwrap()
            .timestamp();
        let s = v(&format!(
            r#"{{"system":{{"poweredBy":"AC Power"}},"history":{{"parsed":{{"count":"2","timestamp":"{at}"}}}}}}"#
        ));
        let facts = feeder_facts(&w, &s, &now);
        assert_eq!(facts[0].state, "Secteur");
        assert_eq!(facts[1].state, "2 portions, 8:00");
        assert_eq!(last_meal(&w, &v("{}"), &now), None);
    }
}
