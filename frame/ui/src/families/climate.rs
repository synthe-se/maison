//! The Mitsubishi air conditioner on the dashboard (ClimateGroup), driven in infrared through the
//! Broadlink blaster. The unit never says its state: the tile says the last order the house
//! sent, « Allumer » sends the last settings again (without their sleep timer, as the remote's
//! `climate_on`), « Éteindre » switches it off, and the two setpoint buttons send the same
//! settings a degree cooler or warmer, each saying the setpoint it sends. The settings are the
//! backend's (`settings` in its answer, written into a command by it). Without a blaster on the
//! network the buttons stay, out of reach. Its row opens its page (SettingsFields, ClimateForm):
//! mode, fan, vane, econo, the sleep timer, each choice sent at once, as a remote's key sends
//! the unit its whole state; the order keeps the running timer. Off, a choice does what the
//! remote does: such a frame is the unit's whole state, so it starts with that setting.

use chrono::DateTime;
use maison_api::Tz;
use maison_api::climate::{
    self, FANS, FIRST_ON_COMMAND, MODES, OFF_COMMAND, STOP_CHOICES_MIN, SendRequest, VANES,
};
use serde_json::Value;

use super::{CLIMATE, Cx, fact, loaded, stale};
use crate::commands::{Order, Post};
use crate::i18n::{Words, k};
use crate::paths;
use crate::ui::icon::{self, Icon};
use crate::ui::model::{Act, Body, Button, Group, Lead, Part, Screen, Tile, View};

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

/// A fan speed in words (`FAN_LABEL`).
fn fan_label(w: &Words, fan: &str) -> String {
    match fan {
        "auto" => w.s(k::CLIMATE_AUTO),
        "silent" => w.s(k::CLIMATE_FAN_LEVELS_SILENT),
        level => w.t(k::CLIMATE_FAN_LEVELS_LEVEL, &[("level", level.into())]),
    }
}

/// A vane's place in words (`VANE_LABEL`).
fn vane_label(w: &Words, vane: &str) -> String {
    w.s(match vane {
        "highest" => k::CLIMATE_VANES_HIGHEST,
        "high" => k::CLIMATE_VANES_HIGH,
        "middle" => k::CLIMATE_VANES_MIDDLE,
        "low" => k::CLIMATE_VANES_LOW,
        "lowest" => k::CLIMATE_VANES_LOWEST,
        "swing" => k::CLIMATE_VANES_SWING,
        _ => k::CLIMATE_AUTO,
    })
}

/// The first blaster found (`/broadlink/discover`'s `devices`).
pub fn blaster(found: &Value) -> Option<&str> {
    found["devices"][0]["host"].as_str()
}

/// What the unit is sent: a command, or settings.
enum Sent {
    Command(&'static str),
    Settings(Value),
}

/// The order sending `what` through the blaster at `host`.
fn send(w: &Words, host: &str, what: Sent) -> Order {
    let (command, settings) = match what {
        Sent::Command(c) => (Some(c.to_string()), None),
        Sent::Settings(s) => (None, Some(s)),
    };
    let body = SendRequest {
        host: host.to_string(),
        local_ip: None,
        command,
        settings,
        model: Some(climate::MODEL.to_string()),
    };
    let name = w.s(k::CLIMATE_BRAND_NAME);
    Order::one(KEY, name, Post::json(paths::CLIMATE_SEND, &body)).then_refresh(&[CLIMATE])
}

/// A button sending `what`; without it, or a blaster, out of reach.
fn button(
    w: &Words,
    host: Option<&str>,
    icon: Option<Icon>,
    label: String,
    what: Option<Sent>,
) -> Button {
    let o = what.and_then(|what| host.map(|h| send(w, h, what)));
    let out = o.is_none();
    let act = o.map_or(Act::Refresh(&[CLIMATE]), Act::Order);
    Button::new(icon, label, act).unavailable(out)
}

/// The last settings sent, again, without their sleep timer (« Allumer »).
fn again(settings: &Value) -> Sent {
    let mut s = settings.clone();
    s["stopInMinutes"] = Value::Null;
    Sent::Settings(s)
}

/// The last settings with one thing changed, what runs left running.
fn changed(settings: &Value, change: impl FnOnce(&mut Value)) -> Sent {
    let mut s = settings.clone();
    change(&mut s);
    Sent::Settings(s)
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
    let button = |icon, label: String, what: Option<Sent>| button(w, host, Some(icon), label, what);
    let on = settings.map_or(Sent::Command(FIRST_ON_COMMAND), again);
    let mut parts = Vec::new();
    if host.is_none() {
        parts.push(Part::Hint(w.s(k::CLIMATE_SEARCHING_TITLE)));
    }
    parts.push(Part::Row(vec![
        button(icon::POWER, w.s(k::ACTION_TURN_ON), Some(on)),
        button(
            icon::SQUARE,
            w.s(k::ACTION_TURN_OFF),
            Some(Sent::Command(OFF_COMMAND)),
        ),
    ]));
    if let (Some(s), Some(t)) = (settings, temperature) {
        let t = t.min(u64::from(u8::MAX)) as u8;
        let step = |delta: i8, icon| {
            let to = climate::step_temperature(t, delta);
            button(
                icon,
                degrees(w, u64::from(to)),
                (to != t).then(|| changed(s, |s| s["temperature"] = to.into())),
            )
        };
        parts.push(Part::Row(vec![
            step(-1, icon::ARROW_DOWN),
            step(1, icon::ARROW_UP),
        ]));
    }
    Tile::new(
        w.s(k::CLIMATE_BRAND_NAME),
        state,
        Lead::Glyph {
            icon: icon::SNOWFLAKE,
            on: power,
        },
    )
    .fact(fact)
    .parts(parts)
    .open(Some(Act::Open(View::Climate)))
}

/// One setting: its name, what it is now, its choices (the chosen one marked), each sent at once.
fn choice(
    w: &Words,
    host: Option<&str>,
    (icon, label, now): (Icon, &str, String),
    options: impl Iterator<Item = (String, bool, Sent)>,
) -> Tile {
    let buttons = options
        .map(|(word, chosen, sent)| button(w, host, None, word, Some(sent)).primary(chosen))
        .collect();
    fact(icon, label, now, false, vec![Part::Flow(buttons)])
}

/// What the page sets, from the last order the server kept; `None` before the first: there is
/// nothing yet to change.
pub fn setting_tiles(w: &Words, host: Option<&str>, last: &Value) -> Option<Vec<Tile>> {
    let s = last.get("settings").filter(|s| s.is_object())?;
    let off = !last["power"].as_bool().unwrap_or(false);
    let word = |key: &str| s[key].as_str().unwrap_or_default().to_string();
    let (mode, fan, vane) = (word("mode"), word("fan"), word("vane"));
    let econo = s["econo"].as_bool().unwrap_or(false);
    let stop = s["stopInMinutes"].as_u64().unwrap_or(0);
    let mut minutes: Vec<u64> = STOP_CHOICES_MIN.iter().map(|m| u64::from(*m)).collect();
    if stop > 0 && !minutes.contains(&stop) {
        // a timer from elsewhere stays in the list
        minutes.push(stop);
        minutes.sort_unstable();
    }
    let timer_word = |m: u64| match m {
        0 => w.s(k::CLIMATE_TIMER_MODES_NONE),
        m => w.duration(m * 60),
    };
    let timer = std::iter::once(0).chain(minutes).map(|m| {
        let sent = changed(s, |s| {
            s["stopInMinutes"] = if m == 0 { Value::Null } else { m.into() };
        });
        (timer_word(m), m == stop, sent)
    });
    let mut econo_parts = vec![Part::Row(vec![
        button(
            w,
            host,
            Some(icon::LEAF),
            w.s(k::CLIMATE_ECONO_COOL),
            (mode == "cool").then(|| changed(s, |s| s["econo"] = (!econo).into())),
        )
        .primary(econo),
    ])];
    if mode != "cool" {
        econo_parts.push(Part::Hint(w.s(k::CLIMATE_ECONO_COOL_DESCRIPTION)));
    }
    let on_off = w.s(if econo { k::STATE_ON } else { k::STATE_OFF });
    let mut tiles = Vec::new();
    if off {
        tiles.push(fact(
            icon::POWER,
            &w.s(k::CLIMATE_MODE),
            w.s(k::CLIMATE_CHOICE_TURNS_ON),
            false,
            Vec::new(),
        ));
    }
    tiles.extend([
        choice(
            w,
            host,
            (
                icon::THERMOMETER,
                &w.s(k::CLIMATE_MODE),
                mode_label(w, &mode),
            ),
            MODES.iter().map(|m| {
                let sent = changed(s, |s| {
                    s["mode"] = (*m).into();
                    // econo works in cooling only: another mode turns it off
                    s["econo"] = (econo && *m == "cool").into();
                });
                (mode_label(w, m), *m == mode, sent)
            }),
        ),
        choice(
            w,
            host,
            (icon::ACTIVITY, &w.s(k::CLIMATE_FAN), fan_label(w, &fan)),
            FANS.iter().map(|f| {
                let sent = changed(s, |s| s["fan"] = (*f).into());
                (fan_label(w, f), *f == fan, sent)
            }),
        ),
        choice(
            w,
            host,
            (
                icon::BLINDS,
                &w.s(k::CLIMATE_VERTICAL_VANE),
                vane_label(w, &vane),
            ),
            VANES.iter().map(|v| {
                let sent = changed(s, |s| s["vane"] = (*v).into());
                (vane_label(w, v), *v == vane, sent)
            }),
        ),
        fact(
            icon::LEAF,
            &w.s(k::CLIMATE_ECONO_COOL),
            on_off,
            false,
            econo_parts,
        ),
        choice(
            w,
            host,
            (icon::CLOCK, &w.s(k::CLIMATE_TIMER_MODE), timer_word(stop)),
            timer,
        ),
    ]);
    Some(tiles)
}

/// The air conditioner's page: its tile, then what it is set to.
pub fn page(cx: &mut Cx) -> Screen {
    let found = cx.live.want(&cx.sources.blasters);
    let state = cx.live.want(&cx.sources.climate);
    let now = cx.house_now();
    let w = cx.words;
    let host = found.data.as_deref().and_then(blaster).map(str::to_string);
    let head = loaded(w, &state, &[CLIMATE], ("", ""), |s| {
        let t = tile(w, host.as_deref(), &s["state"], &now).open(None);
        Some(Body::Tiles(vec![t]))
    });
    let mut groups = vec![
        Group::new(w.s(k::CLIMATE_DASHBOARD_TITLE), head).stale(stale(cx, &state, &[CLIMATE])),
    ];
    if let Some(tiles) = state
        .data
        .as_deref()
        .and_then(|s| setting_tiles(w, host.as_deref(), &s["state"]))
    {
        groups.push(Group::new(w.s(k::COMMON_SETTINGS), Body::Tiles(tiles)));
    }
    super::page(w, w.s(k::CLIMATE_BRAND_NAME), groups)
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
    Some(Group::new(w.s(k::CLIMATE_DASHBOARD_TITLE), body).stale(stale(cx, &state, &[CLIMATE])))
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
        assert_eq!(
            posted(&steps[1])["settings"]["stopInMinutes"],
            180,
            "a step leaves the timer running"
        );
        assert!(matches!(t.open, Some(Act::Open(View::Climate))));
    }

    fn flow(t: &Tile) -> &[Button] {
        let Part::Flow(b) = &t.parts[0] else {
            panic!("{:?}", t.parts)
        };
        b
    }

    #[test]
    fn its_page_sets_each_thing_at_once_the_chosen_one_marked() {
        let w = Words::new(Lang::Fr);
        let s = state(ON);
        let t = setting_tiles(&w, Some("192.168.1.60"), &s["state"]).unwrap();
        let [mode, fan, vane, econo, timer] = &t[..] else {
            panic!("{t:?}")
        };
        assert_eq!((mode.name.as_str(), mode.state.as_str()), ("Mode", "Froid"));
        let m = flow(mode);
        assert_eq!(m.len(), 5);
        assert!(m[0].primary && !m[1].primary);
        assert_eq!(m[1].label, "Chaud");
        assert_eq!(posted(&m[1])["settings"]["mode"], "heat");
        assert_eq!(posted(&m[1])["host"], "192.168.1.60");
        assert_eq!(
            posted(&m[1])["settings"]["stopInMinutes"],
            180,
            "the timer keeps running"
        );
        let f = flow(fan);
        assert_eq!(f[1].label, "Niveau 1");
        assert_eq!(posted(&f[5])["settings"]["fan"], "silent");
        assert_eq!(fan.state, "Auto");
        assert_eq!(posted(&flow(vane)[6])["settings"]["vane"], "swing");
        let Part::Row(e) = &econo.parts[0] else {
            panic!()
        };
        assert!(!e[0].unavailable);
        assert_eq!(posted(&e[0])["settings"]["econo"], true);
        let c = flow(timer);
        assert_eq!(c[0].label, "Désactivé");
        assert_eq!(posted(&c[0])["settings"]["stopInMinutes"], Value::Null);
        assert!(c[4].primary, "180 min: {:?}", c[4].label);
        assert_eq!(timer.state, "3\u{a0}h");
        assert_eq!(posted(&c[1])["settings"]["stopInMinutes"], 30);
    }

    #[test]
    fn econo_is_for_cooling_and_leaving_it_turns_econo_off() {
        let w = Words::new(Lang::Fr);
        let s = state(&ON.replace("\"econo\":false", "\"econo\":true"));
        let t = setting_tiles(&w, Some("h"), &s["state"]).unwrap();
        assert_eq!(posted(&flow(&t[0])[1])["settings"]["econo"], false);
        let dry = state(&ON.replace("\"mode\":\"cool\"", "\"mode\":\"dry\""));
        let t = setting_tiles(&w, Some("h"), &dry["state"]).unwrap();
        let [Part::Row(e), Part::Hint(why)] = &t[3].parts[..] else {
            panic!()
        };
        assert!(e[0].unavailable);
        assert_eq!(why, "Utile uniquement en mode froid.");
    }

    #[test]
    fn off_a_choice_starts_it_with_that_setting_and_it_says_so() {
        let w = Words::new(Lang::Fr);
        let s = state(&ON.replace("\"power\":true", "\"power\":false"));
        let t = setting_tiles(&w, Some("h"), &s["state"]).unwrap();
        assert_eq!(t[0].state, "Choisir un réglage allume la clim.");
        let modes = flow(&t[1]);
        assert!(modes.iter().all(|b| !b.unavailable));
        // settings, never the off command: a whole frame, which starts the unit
        assert_eq!(posted(&modes[1])["settings"]["mode"], MODES[1]);
        let none = state(r#"{"success":true,"state":null,"message":"none"}"#);
        assert!(setting_tiles(&w, Some("h"), &none["state"]).is_none());
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
