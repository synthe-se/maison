//! The Mitsubishi air conditioner on the dashboard (ClimateGroup), driven in infrared through the
//! Broadlink blaster. The unit never says its state: the tile says the last order the house
//! sent, « Allumer » sends the last settings again (without their sleep timer, as the remote's
//! `climate_on`), « Éteindre » switches it off, and the two setpoint buttons send the same
//! settings a degree cooler or warmer, each saying the setpoint it sends. The settings are the
//! backend's (`settings` in its answer, written into a command by it): the frame changes only
//! their temperature. Without a blaster on the network the buttons stay, out of reach.

use chrono::DateTime;
use maison_api::Tz;
use maison_api::climate::{self, FIRST_ON_COMMAND, OFF_COMMAND};
use serde_json::{Value, json};

use super::{CLIMATE, Cx, loaded, stale};
use crate::commands::{Order, Post};
use crate::i18n::{Words, k};
use crate::paths;
use crate::ui::icon;
use crate::ui::model::{Act, Body, Button, Group, Lead, Part, Tile};

/// Every order to the unit goes under one key: one at a time, the latest waits.
const KEY: &str = "broadlink:climate:send";

/// « 21 °C » (labels.ts `degrees`).
pub fn degrees(w: &Words, t: u64) -> String {
    w.t(
        k::CLIMATE_DEGREES,
        &[("degrees", w.num(t as f64, 0).into())],
    )
}

/// A mode as the backend keeps it: its word when known, the token otherwise (`modeLabel`).
pub fn mode_label(w: &Words, mode: &str) -> String {
    match mode {
        "cool" => w.s(k::CLIMATE_MODES_COOL),
        "heat" => w.s(k::CLIMATE_MODES_HEAT),
        "dry" => w.s(k::CLIMATE_MODES_DRY),
        "fan" => w.s(k::CLIMATE_FAN),
        "auto" => w.s(k::CLIMATE_AUTO),
        other => other.to_string(),
    }
}

/// The first blaster found (`/broadlink/discover`'s `devices`).
pub fn blaster(found: &Value) -> Option<&str> {
    found["devices"][0]["host"].as_str()
}

/// The order sending `body` (a command or settings) through the blaster at `host`.
fn send(host: &str, mut body: Value) -> Order {
    body["host"] = json!(host);
    body["model"] = json!(climate::MODEL);
    Order::one(KEY, "climate", Post::json(paths::CLIMATE_SEND, &body)).then_refresh(&[CLIMATE])
}

/// The last settings sent, again: without their sleep timer, at `temperature` if given.
fn settings_at(settings: &Value, temperature: Option<u8>) -> Value {
    let mut s = settings.clone();
    s["stopInMinutes"] = Value::Null;
    if let Some(t) = temperature {
        s["temperature"] = json!(t);
    }
    json!({ "settings": s })
}

/// The tile, from the blaster found (if any) and the last order the server kept
/// (`/broadlink/mitsubishi/state`'s `state`, null before the first).
pub fn tile(w: &Words, host: Option<&str>, last: &Value, now: &DateTime<Tz>) -> Tile {
    let power = last["power"].as_bool().unwrap_or(false);
    let settings = last.get("settings").filter(|s| s.is_object());
    let temperature = settings.and_then(|s| s["temperature"].as_u64());
    let when = last["updatedAt"]
        .as_str()
        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
        .map(|t| w.when(&t.with_timezone(&now.timezone()), now));
    let state = match (last.is_object(), power, when) {
        (true, true, Some(when)) => w.t(k::CLIMATE_LAST_ON, &[("when", when.into())]),
        (true, false, Some(when)) => w.t(k::CLIMATE_LAST_OFF, &[("when", when.into())]),
        _ => w.s(k::CLIMATE_NO_ORDER),
    };
    let fact = match (
        power,
        temperature,
        settings.and_then(|s| s["mode"].as_str()),
    ) {
        (true, Some(t), Some(mode)) => Some(w.t(
            k::CLIMATE_FACT,
            &[
                ("temperature", degrees(w, t).into()),
                ("mode", mode_label(w, mode).into()),
            ],
        )),
        _ => None,
    };
    let order = |body: Value| host.map(|h| send(h, body));
    let button = |icon, label: String, body: Option<Value>| {
        let o = body.and_then(order);
        let out = o.is_none();
        let act = o.map_or(Act::Refresh(&[CLIMATE]), Act::Order);
        Button::new(Some(icon), label, act).unavailable(out)
    };
    let on = settings.map_or_else(
        || json!({ "command": FIRST_ON_COMMAND }),
        |s| settings_at(s, None),
    );
    let mut parts = Vec::new();
    if host.is_none() {
        parts.push(Part::Hint(w.s(k::CLIMATE_SEARCHING_TITLE)));
    }
    parts.push(Part::Row(vec![
        button(icon::POWER, w.s(k::ACTION_TURN_ON), Some(on)),
        button(
            icon::SQUARE,
            w.s(k::ACTION_TURN_OFF),
            Some(json!({ "command": OFF_COMMAND })),
        ),
    ]));
    if let (Some(s), Some(t)) = (settings, temperature) {
        let t = t.min(u64::from(u8::MAX)) as u8;
        let step = |delta: i8, icon| {
            let to = climate::step_temperature(t, delta);
            button(
                icon,
                degrees(w, u64::from(to)),
                (to != t).then(|| settings_at(s, Some(to))),
            )
        };
        parts.push(Part::Row(vec![
            step(-1, icon::ARROW_DOWN),
            step(1, icon::ARROW_UP),
        ]));
    }
    Tile {
        name: w.s(k::CLIMATE_BRAND_NAME),
        state,
        warn: false,
        lead: Lead::Glyph {
            icon: icon::SNOWFLAKE,
            on: power,
        },
        fact,
        parts,
        open: None,
    }
}

pub fn group(cx: &mut Cx) -> Option<Group> {
    let found = cx.live.want(&cx.sources.blasters);
    let state = cx.live.want(&cx.sources.climate);
    let now = cx.house_now();
    let w = cx.words;
    let host = found.data.as_deref().and_then(blaster).map(str::to_string);
    let body = loaded(w, &state, &[CLIMATE], ("", ""), |s| {
        Some(Body::Tiles(vec![tile(
            w,
            host.as_deref(),
            &s["state"],
            &now,
        )]))
    });
    Some(Group {
        title: w.s(k::CLIMATE_DASHBOARD_TITLE),
        fact: None,
        actions: Vec::new(),
        stale: stale(cx, &state, &[CLIMATE]),
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;
    use chrono::TimeZone;
    use maison_api::{Answer, HOUSE_TZ};

    /// The backend's answer to `/broadlink/mitsubishi/state`, its body.
    fn state(json: &str) -> Value {
        serde_json::from_str::<Answer<Value>>(json).unwrap().body
    }

    fn now() -> DateTime<Tz> {
        HOUSE_TZ.with_ymd_and_hms(2026, 10, 16, 19, 0, 0).unwrap()
    }

    const ON: &str = r#"{"success":true,"state":{"power":true,"lastCommand":"state-cool-21-fan-auto-vane-auto-wide-center-stopin-180","lastOnCommand":"state-cool-21-fan-auto-vane-auto-wide-center-stopin-180","settings":{"mode":"cool","temperature":21,"fan":"auto","vane":"auto","wide":"center","econo":false,"stopInMinutes":180},"host":"192.168.1.60","model":"msz-hj5va","updatedAt":"2026-10-16T16:50:00Z"},"message":"ok"}"#;

    fn posted(b: &Button) -> &Value {
        let Act::Order(o) = &b.act else {
            panic!("{:?}", b.act)
        };
        o.posts[0].1.body.as_ref().unwrap()
    }

    #[test]
    fn on_it_says_the_last_order_its_setpoint_and_sends_them_again() {
        let w = Words::new(Lang::Fr);
        let s = state(ON);
        let t = tile(&w, Some("192.168.1.60"), &s["state"], &now());
        assert_eq!(t.state, "Dernier ordre\u{a0}: allumer · 18:50");
        assert_eq!(t.fact.as_deref(), Some("21\u{a0}°C · Froid"));
        assert!(matches!(t.lead, Lead::Glyph { on: true, .. }));
        let [Part::Row(power), Part::Row(steps)] = &t.parts[..] else {
            panic!("{:?}", t.parts)
        };
        // « Allumer »: the same settings, without the sleep timer, to the blaster found
        let on = posted(&power[0]);
        assert_eq!(on["host"], "192.168.1.60");
        assert_eq!(on["model"], "msz-hj5va");
        assert_eq!(on["settings"]["temperature"], 21);
        assert_eq!(on["settings"]["stopInMinutes"], Value::Null);
        assert_eq!(posted(&power[1])["command"], "state-off");
        // each step says the setpoint it sends
        assert_eq!(steps[0].label, "20\u{a0}°C");
        assert_eq!(posted(&steps[0])["settings"]["temperature"], 20);
        assert_eq!(steps[1].label, "22\u{a0}°C");
        assert_eq!(posted(&steps[1])["settings"]["mode"], "cool");
    }

    #[test]
    fn never_ordered_it_starts_from_the_first_settings() {
        let w = Words::new(Lang::Fr);
        let s = state(r#"{"success":true,"state":null,"message":"none"}"#);
        let t = tile(&w, Some("192.168.1.60"), &s["state"], &now());
        assert_eq!(t.state, "Aucun ordre envoyé");
        assert_eq!(t.fact, None);
        let [Part::Row(power)] = &t.parts[..] else {
            panic!("{:?}", t.parts)
        };
        assert_eq!(posted(&power[0])["command"], FIRST_ON_COMMAND);
    }

    #[test]
    fn without_a_blaster_the_buttons_stay_out_of_reach() {
        let w = Words::new(Lang::Fr);
        let s = state(ON);
        let t = tile(&w, None, &s["state"], &now());
        assert!(matches!(&t.parts[0], Part::Hint(h) if h == "Recherche du RM4 Pro"));
        let Part::Row(power) = &t.parts[1] else {
            panic!()
        };
        assert!(power.iter().all(|b| b.unavailable));
    }

    #[test]
    fn at_a_bound_the_step_beyond_it_sends_nothing() {
        let w = Words::new(Lang::Fr);
        let s = state(&ON.replace("\"temperature\":21", "\"temperature\":31"));
        let t = tile(&w, Some("192.168.1.60"), &s["state"], &now());
        let Part::Row(steps) = &t.parts[1] else {
            panic!()
        };
        assert!(!steps[0].unavailable && steps[1].unavailable);
    }
}
