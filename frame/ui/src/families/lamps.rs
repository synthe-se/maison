//! Every lamp in one group, whatever its radio (LampsGroup, LampTile, lamps/lamp.ts): Hue over
//! Bluetooth and Zigbee lamps on the server's coordinator, a family left out when the server
//! runs without its radio. The icon turns a lamp on or off, line 2 says « Allumée, 80 % »; the
//! brightness slider only under a lamp that is on and reachable. « Tout éteindre » when one is.
//! A lamp with more to set opens its page (LampPage, ZigbeeLampPage): its white, a Zigbee lamp's
//! colors and effects. Renaming stays the web app's.

use chrono::DateTime;
use maison_api::Tz;
use maison_api::lamps::{
    BrightnessRequest, ColorRequest, EffectRequest, HueLamp, LampList, PowerRequest,
    TemperatureRequest, ZigbeeLamp,
};

use super::{Cx, HUE, LAMPS_GROUP, ZIGBEE, loaded, stale};
use crate::commands::{Order, Outcome, Post, Say, Toast, json, limit};
use crate::i18n::{Arg, Words, k};
use crate::live::View;
use crate::paths;
use crate::theme::Rgba;
use crate::ui::icon;
use crate::ui::model::{Act, Body, Button, Group, Lead, Part, Screen, Slider, Tile, View as Page};

/// One lamp, whatever its radio (lamp.ts `Lamp`).
#[derive(Debug, Clone, PartialEq)]
pub struct Lamp {
    pub id: String,
    pub name: String,
    pub reachable: bool,
    pub connecting: bool,
    pub last_seen: Option<String>,
    pub is_on: bool,
    pub brightness: u8,
    /// Its white, 0 (warm) to 100 (cool), when it has one.
    pub temperature: Option<u8>,
    /// It takes a color and effects.
    pub color: bool,
}

/// A family's API (lamp.ts `LampDriver`): its key and its paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Driver {
    pub key: &'static str,
    /// `/hue-lamps`, `/zigbee/lamps`.
    pub base: &'static str,
}

pub const HUE_DRIVER: Driver = Driver {
    key: HUE,
    base: paths::HUE_LAMPS,
};
pub const ZIGBEE_DRIVER: Driver = Driver {
    key: ZIGBEE,
    base: paths::ZIGBEE_LAMPS,
};
/// Both families, read again together.
pub const LAMPS: &[&str] = &[HUE, ZIGBEE];

pub fn from_hue(l: &HueLamp) -> Lamp {
    Lamp {
        id: l.id.clone(),
        name: l.name.clone(),
        reachable: l.connected,
        connecting: l.connecting,
        last_seen: l.last_seen.clone(),
        is_on: l.state.is_on,
        brightness: l.state.brightness,
        temperature: l.state.temperature,
        color: false,
    }
}

pub fn from_zigbee(l: &ZigbeeLamp) -> Lamp {
    Lamp {
        id: l.id.clone(),
        name: l.name.clone(),
        reachable: l.reachable,
        connecting: false,
        last_seen: l.last_seen.clone(),
        is_on: l.state.is_on,
        brightness: l.state.brightness,
        temperature: l.state.temperature,
        color: l.supports_color,
    }
}

/// Line 2: « Allumée, 80 % », « Éteinte », « Injoignable depuis 12 min ».
pub fn state(w: &Words, l: &Lamp, now: &DateTime<Tz>) -> String {
    if l.connecting {
        return w.s(k::LAMPS_CONNECTING);
    }
    if !l.reachable {
        let seen = l
            .last_seen
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.with_timezone(&now.timezone()));
        return w.unreachable_since(seen, now);
    }
    if l.is_on {
        w.t(
            k::LAMPS_ON_PERCENT,
            &[("percent", Arg::Count(f64::from(l.brightness)))],
        )
    } else {
        w.s(k::STATE_OFF)
    }
}

fn power_post(d: Driver, l: &Lamp, on: bool) -> Post {
    Post::json(
        paths::of(d.base, &l.id, "power"),
        &PowerRequest { enabled: on },
    )
}

fn power(d: Driver, l: &Lamp, on: bool) -> Order {
    Order::one(
        format!("{}:{}:power", d.key, l.id),
        l.name.clone(),
        power_post(d, l, on),
    )
    .toggle(on)
    .within(limit::LAMP)
    .then_refresh(&[d.key])
}

/// A brightness slider's body (1–100, whole percents).
fn brightness(value: f64) -> serde_json::Value {
    json(&BrightnessRequest {
        brightness: value.round().clamp(0.0, 100.0) as u8,
    })
}

pub fn tile(w: &Words, d: Driver, l: &Lamp, now: &DateTime<Tz>) -> Tile {
    let mut parts = Vec::new();
    if l.reachable && l.is_on {
        parts.push(Part::Slider(Slider {
            key: format!("{}:{}:brightness", d.key, l.id),
            value: f64::from(l.brightness),
            min: 1.0,
            max: 100.0,
            step: 1.0,
            mark: None,
            live: true,
            put: false,
            off: false,
            near: 2.0,
            ends: None,
            path: paths::of(d.base, &l.id, "brightness"),
            body: brightness,
            refresh: d.key,
            name: l.name.clone(),
        }));
    }
    Tile::new(
        l.name.clone(),
        state(w, l, now),
        Lead::Toggle {
            icon: if l.reachable {
                icon::LIGHTBULB
            } else {
                icon::LIGHTBULB_OFF
            },
            on: l.is_on,
            offline: !l.reachable,
            turn_on: Box::new(power(d, l, true)),
            turn_off: Box::new(power(d, l, false)),
        },
    )
    .warn(!l.reachable && !l.connecting)
    .parts(parts)
    .open((l.temperature.is_some() || l.color).then(|| Act::Open(Page::Lamp(d.key, l.id.clone()))))
}

fn all_off_said(w: &Words, o: &Outcome) -> Option<Toast> {
    Some(super::outcome(w, o, |count| {
        w.t(k::LAMPS_ALL_OFF_DONE, &[crate::i18n::count(count)])
    }))
}

/// The group from the two families' reads (`None` when the server runs neither radio).
pub fn lamps_group(
    w: &Words,
    families: &[(Driver, &View<Vec<Lamp>>)],
    now: &DateTime<Tz>,
    stale: Option<(String, Button)>,
) -> Option<Group> {
    if families.is_empty() {
        return None;
    }
    let rows: Vec<(Driver, &Lamp)> = families
        .iter()
        .flat_map(|(d, v)| {
            v.data
                .as_deref()
                .into_iter()
                .flatten()
                .map(move |l| (*d, l))
        })
        .collect();
    let lit: Vec<_> = rows
        .iter()
        .filter(|(_, l)| l.reachable && l.is_on)
        .collect();
    let mut actions = Vec::new();
    if !lit.is_empty() {
        let posts = lit
            .iter()
            .map(|(d, l)| (l.name.clone(), power_post(*d, l, false)))
            .collect();
        actions.push(Button::new(
            Some(icon::LIGHTBULB_OFF),
            w.s(k::LAMPS_ALL_OFF),
            Act::Order(
                Order::many("lamps:all-off", posts)
                    .within(limit::LAMP)
                    .then_refresh(LAMPS)
                    .said(Say::With(all_off_said)),
            ),
        ));
    }
    // the two lists read as one: loading while one is, failed when one did, and nothing known
    let unknown = rows.is_empty() && families.iter().any(|(_, v)| v.loading || v.failed());
    let loading = families.iter().any(|(_, v)| v.loading);
    let both = View {
        data: (!unknown).then(|| std::sync::Arc::new(rows)),
        loading,
        // still reading one: no failure said yet
        failure: if loading {
            None
        } else {
            families.iter().find_map(|(_, v)| v.failure.clone())
        },
        stale: false,
        at: None,
    };
    let body = loaded(
        w,
        &both,
        LAMPS,
        (k::LAMPS_NONE, k::LAMPS_NONE_HINT),
        |rows| {
            (!rows.is_empty())
                .then(|| Body::Tiles(rows.iter().map(|(d, l)| tile(w, *d, l, now)).collect()))
        },
    );
    Some(
        Group::new(w.s(k::ZIGBEE_LAMPS_TITLE), body)
            .anchor(LAMPS_GROUP)
            .actions(actions)
            .stale(stale),
    )
}

// ── a lamp's page ──

/// The colors offered, as color.ts `PRESETS`.
const PRESETS: [(&str, [u8; 3]); 8] = [
    (k::COLOR_RED, [239, 68, 68]),
    (k::ZIGBEE_LAMPS_COLOR_ORANGE, [249, 115, 22]),
    (k::ZIGBEE_LAMPS_COLOR_YELLOW, [234, 179, 8]),
    (k::ZIGBEE_LAMPS_COLOR_GREEN, [34, 197, 94]),
    (k::COLOR_BLUE, [59, 130, 246]),
    (k::ZIGBEE_LAMPS_COLOR_PURPLE, [168, 85, 247]),
    (k::ZIGBEE_LAMPS_COLOR_PINK, [236, 72, 153]),
    (k::COLOR_WHITE, [245, 245, 244]),
];

/// The effects Hue's Zigbee lamps know, by the id the server expects (color.ts `EFFECTS`).
const EFFECTS: [(&str, &str); 10] = [
    ("candle", k::ZIGBEE_LAMPS_EFFECT_CANDLE),
    ("fireplace", k::ZIGBEE_LAMPS_EFFECT_FIREPLACE),
    ("colorloop", k::ZIGBEE_LAMPS_EFFECT_COLORLOOP),
    ("sunrise", k::ZIGBEE_LAMPS_EFFECT_SUNRISE),
    ("sparkle", k::ZIGBEE_LAMPS_EFFECT_SPARKLE),
    ("opal", k::ZIGBEE_LAMPS_EFFECT_OPAL),
    ("glisten", k::ZIGBEE_LAMPS_EFFECT_GLISTEN),
    ("blink", k::ZIGBEE_LAMPS_EFFECT_BLINK),
    ("breathe", k::ZIGBEE_LAMPS_EFFECT_BREATHE),
    ("okay", k::ZIGBEE_LAMPS_EFFECT_OKAY),
];
const STOP_EFFECT: &str = "stop_hue_effect";

/// sRGB to the CIE point a lamp speaks (color.ts `rgbToXy`).
pub fn rgb_to_xy(rgb: [u8; 3]) -> (f32, f32) {
    let linear = |c: u8| {
        let c = f32::from(c) / 255.0;
        if c > 0.04045 {
            ((c + 0.055) / 1.055).powf(2.4)
        } else {
            c / 12.92
        }
    };
    let [r, g, b] = rgb.map(linear);
    let x = r * 0.664_511 + g * 0.154_324 + b * 0.162_028;
    let y = r * 0.283_881 + g * 0.668_433 + b * 0.047_685;
    let z = r * 0.000_088 + g * 0.072_31 + b * 0.986_039;
    let sum = x + y + z;
    if sum == 0.0 {
        return (0.3127, 0.329); // D65 white
    }
    (x / sum, y / sum)
}

/// Why a lamp's white and colors cannot act now (lamp.ts `lampWhy`); `None` when they can.
fn why(w: &Words, l: &Lamp, now: &DateTime<Tz>) -> Option<String> {
    if !l.reachable {
        return Some(state(w, l, now));
    }
    (!l.is_on).then(|| w.s(k::LAMPS_OFF_REASON))
}

/// Its white in words (`temperatureText`): Zigbee's in kelvin, between the bounds
/// `backend/src/zigbee/zcl.rs` sends, Bluetooth's a percentage.
fn white_words(w: &Words, d: Driver, percent: u8) -> String {
    let tone = w.s(match percent {
        0..34 => k::LAMPS_TONE_WARM,
        34..67 => k::LAMPS_TONE_NEUTRAL,
        _ => k::LAMPS_TONE_COOL,
    });
    let p = f64::from(percent);
    if d.key == ZIGBEE {
        const MIRED_WARM: f64 = 500.0;
        const MIRED_COOL: f64 = 153.0;
        let mired = MIRED_WARM - p * (MIRED_WARM - MIRED_COOL) / 100.0;
        let kelvin = (1_000_000.0 / mired / 100.0).round() * 100.0;
        w.t(
            k::LAMPS_TEMPERATURE_KELVIN,
            &[("kelvin", Arg::Count(kelvin)), ("tone", tone.into())],
        )
    } else {
        w.t(
            k::LAMPS_TEMPERATURE_PERCENT,
            &[("percent", Arg::Count(p)), ("tone", tone.into())],
        )
    }
}

/// A temperature slider's body.
fn white(value: f64) -> serde_json::Value {
    json(&TemperatureRequest {
        temperature: value.round().clamp(0.0, 100.0) as u8,
    })
}

/// One order of the lamp's page, under its one key: one at a time, the latest waits.
fn light(d: Driver, l: &Lamp, action: &str, body: &impl serde::Serialize) -> Order {
    Order::one(
        format!("{}:{}:light", d.key, l.id),
        l.name.clone(),
        Post::json(paths::of(d.base, &l.id, action), body),
    )
    .within(limit::LAMP)
    .then_refresh(&[d.key])
}

/// What a lamp's page sets, each out of reach while it cannot act, saying why.
pub fn lamp_groups(w: &Words, d: Driver, l: &Lamp, now: &DateTime<Tz>) -> Vec<Group> {
    let why = why(w, l, now);
    let off = why.is_some();
    let mut groups = vec![Group::new(
        w.s(k::LAMPS_LAMP),
        Body::Tiles(vec![tile(w, d, l, now).open(None)]),
    )];
    let said = |parts: &mut Vec<Part>| parts.extend(why.clone().map(Part::Hint));
    if let Some(t) = l.temperature {
        let mut parts = vec![Part::Slider(Slider {
            key: format!("{}:{}:temperature", d.key, l.id),
            value: f64::from(t),
            min: 0.0,
            max: 100.0,
            step: 1.0,
            mark: None,
            live: true,
            put: false,
            off,
            near: 2.0,
            ends: Some((w.s(k::LAMPS_WARM), w.s(k::LAMPS_COOL))),
            path: paths::of(d.base, &l.id, "temperature"),
            body: white,
            refresh: d.key,
            name: l.name.clone(),
        })];
        said(&mut parts);
        groups.push(Group::new(
            w.s(k::LAMPS_TEMPERATURE),
            Body::Tiles(vec![
                Tile::new(
                    white_words(w, d, t),
                    String::new(),
                    Lead::Glyph {
                        icon: icon::SUN,
                        on: !off,
                    },
                )
                .parts(parts),
            ]),
        ));
    }
    if l.color {
        let colors = PRESETS
            .iter()
            .map(|(name, rgb)| {
                let (x, y) = rgb_to_xy(*rgb);
                let order = light(d, l, "color", &ColorRequest { x, y });
                Button::new(None, w.s(name), Act::Order(order))
                    .dot(Rgba(rgb[0], rgb[1], rgb[2], 255))
                    .unavailable(off)
            })
            .collect();
        let effect = |id: &str, label: String| {
            let body = EffectRequest { effect: id.into() };
            Button::new(None, label, Act::Order(light(d, l, "effect", &body))).unavailable(off)
        };
        let mut effects: Vec<_> = EFFECTS
            .iter()
            .map(|(id, name)| effect(id, w.s(name)))
            .collect();
        effects.push(effect(STOP_EFFECT, w.s(k::ZIGBEE_LAMPS_EFFECT_STOP_EFFECT)));
        let mut color = vec![Part::Flow(colors)];
        said(&mut color);
        groups.push(Group::new(w.s(k::ZIGBEE_LAMPS_COLOR), Body::Parts(color)));
        groups.push(Group::new(
            w.s(k::ZIGBEE_LAMPS_EFFECTS),
            Body::Parts(vec![Part::Flow(effects)]),
        ));
    }
    groups
}

/// A lamp's page, from its family's list: not read yet it says so, a lamp the list does not
/// have is « introuvable ».
pub fn page(cx: &mut Cx, family: &str, id: &str) -> Screen {
    let families = families(cx);
    let now = cx.house_now();
    let w = cx.words;
    let Some((d, v)) = families.iter().find(|(d, _)| d.key == family) else {
        return super::page(w, w.s(k::LAMPS_NOT_FOUND), Vec::new());
    };
    let found = v
        .data
        .as_deref()
        .and_then(|l| l.iter().find(|l| l.id == id));
    if let Some(l) = found {
        return super::page(w, l.name.clone(), lamp_groups(w, *d, l, &now));
    }
    let title = if v.loading {
        k::COMMON_LOADING
    } else {
        k::LAMPS_NOT_FOUND
    };
    let body = loaded(
        w,
        v,
        LAMPS,
        (k::LAMPS_NOT_FOUND, k::LAMPS_NONE_HINT),
        |_| None,
    );
    super::page(w, w.s(title), vec![Group::new(w.s(k::LAMPS_LAMP), body)])
}

/// A list's lamps, its read state kept.
fn as_lamps<L>(v: View<LampList<L>>, from: fn(&L) -> Lamp) -> View<Vec<Lamp>> {
    v.map(|d| d.lamps.iter().map(from).collect())
}

/// The radios the server runs (its stats say whether Bluetooth is there at all), each with its
/// lamps as read.
fn families(cx: &mut Cx) -> Vec<(Driver, View<Vec<Lamp>>)> {
    let hue_on = cx
        .live
        .want(&cx.sources.hue_stats)
        .data
        .is_none_or(|s| !s.disabled);
    let zigbee_on = cx
        .live
        .want(&cx.sources.zigbee_stats)
        .data
        .is_none_or(|s| !s.disabled);
    let mut families = Vec::new();
    if hue_on {
        families.push((
            HUE_DRIVER,
            as_lamps(cx.live.want(&cx.sources.hue), from_hue),
        ));
    }
    if zigbee_on {
        families.push((
            ZIGBEE_DRIVER,
            as_lamps(cx.live.want(&cx.sources.zigbee), from_zigbee),
        ));
    }
    families
}

/// Every lamp read, whatever its radio (« Maintenant » counts those lit).
pub fn all(cx: &mut Cx) -> Vec<Lamp> {
    families(cx)
        .into_iter()
        .flat_map(|(_, v)| v.data.as_deref().cloned().unwrap_or_default())
        .collect()
}

pub fn group(cx: &mut Cx) -> Option<Group> {
    let families = families(cx);
    let now = cx.house_now();
    let old = families.iter().find_map(|(_, v)| stale(cx, v, LAMPS));
    let refs: Vec<_> = families.iter().map(|(d, v)| (*d, v)).collect();
    lamps_group(cx.words, &refs, &now, old)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;
    use chrono::TimeZone;
    use maison_api::{Answer, HOUSE_TZ};

    fn view(lamps: Vec<Lamp>) -> View<Vec<Lamp>> {
        View::ready(lamps)
    }

    fn now() -> DateTime<Tz> {
        HOUSE_TZ.with_ymd_and_hms(2026, 11, 12, 20, 0, 0).unwrap()
    }

    const HUE_LIST: &str = r#"{"success":true,"lamps":[{"id":"aa:bb","name":"Salon","address":"AA:BB","model":"LCA001","manufacturer":"Signify","firmware":"1.0","connected":true,"connecting":false,"reachable":true,"state":{"isOn":true,"brightness":80,"temperature":null,"temperatureMin":null,"temperatureMax":null},"lastSeen":null}],"total":1,"connected":1,"reachable":1,"message":"ok"}"#;
    const ZIGBEE_LIST: &str = r#"{"success":true,"lamps":[{"id":"0x01","name":"Chambre","address":"0x01","friendlyName":"Chambre","interviewCompleted":true,"model":null,"manufacturer":"IKEA","connected":false,"reachable":false,"supportsBrightness":true,"supportsTemperature":false,"supportsColor":false,"state":{"isOn":false,"brightness":50,"temperature":null,"temperatureMin":null,"temperatureMax":null,"colorX":null,"colorY":null,"colorMode":null},"lastSeen":"2026-11-12T18:50:00Z"}],"total":1,"connected":0,"reachable":0,"message":"ok"}"#;

    #[test]
    fn both_radios_in_one_group_read_from_the_backends_answers() {
        let w = Words::new(Lang::Fr);
        let hue: LampList<HueLamp> = serde_json::from_str::<Answer<_>>(HUE_LIST).unwrap().body;
        let zigbee: LampList<ZigbeeLamp> =
            serde_json::from_str::<Answer<_>>(ZIGBEE_LIST).unwrap().body;
        let h = view(hue.lamps.iter().map(from_hue).collect());
        let z = view(zigbee.lamps.iter().map(from_zigbee).collect());
        let g = lamps_group(&w, &[(HUE_DRIVER, &h), (ZIGBEE_DRIVER, &z)], &now(), None).unwrap();
        assert_eq!(g.title, "Lampes");
        let Body::Tiles(tiles) = &g.body else {
            panic!("tiles")
        };
        assert_eq!(tiles.len(), 2);
        assert_eq!(tiles[0].state, "Allumée, 80\u{202f}%");
        let Lead::Toggle {
            turn_off: order,
            on,
            offline,
            ..
        } = &tiles[0].lead
        else {
            panic!("toggle")
        };
        assert!(*on && !offline);
        assert_eq!(order.key, "hue-lamp:aa:bb:power");
        assert_eq!(order.posts[0].1.path, "/hue-lamps/aa%3Abb/power");
        assert_eq!(
            order.posts[0].1.body,
            Some(serde_json::json!({"enabled": false}))
        );
        assert_eq!(order.target, Some(false));
        let Part::Slider(s) = &tiles[0].parts[0] else {
            panic!("slider")
        };
        assert_eq!(s.value, 80.0);
        assert!(s.live);
        assert_eq!((s.body)(42.4), serde_json::json!({"brightness": 42}));
        // unreachable: said since when, no slider, the gesture shown but not operable
        assert_eq!(tiles[1].state, "Injoignable depuis 10\u{a0}min");
        assert!(tiles[1].warn);
        assert!(tiles[1].parts.is_empty());
        // one lit: « Tout éteindre »
        assert_eq!(g.actions.len(), 1);
        let Act::Order(all) = &g.actions[0].act else {
            panic!("order")
        };
        assert_eq!(all.posts.len(), 1);
        assert_eq!(all.refresh, [HUE, ZIGBEE]);
    }

    fn lit(color: bool, temperature: Option<u8>) -> Lamp {
        Lamp {
            id: "0x01".into(),
            name: "Chambre".into(),
            reachable: true,
            connecting: false,
            last_seen: None,
            is_on: true,
            brightness: 50,
            temperature,
            color,
        }
    }

    #[test]
    fn a_lamp_with_more_to_set_opens_its_page() {
        let w = Words::new(Lang::Fr);
        let t = tile(&w, ZIGBEE_DRIVER, &lit(true, Some(20)), &now());
        assert!(matches!(&t.open, Some(Act::Open(Page::Lamp(ZIGBEE, id))) if id == "0x01"));
        let plain = tile(&w, HUE_DRIVER, &lit(false, None), &now());
        assert!(plain.open.is_none(), "its row stays its on/off");
    }

    #[test]
    fn a_lamps_page_sets_its_white_its_color_its_effect() {
        let w = Words::new(Lang::Fr);
        let g = lamp_groups(&w, ZIGBEE_DRIVER, &lit(true, Some(20)), &now());
        assert_eq!(g.len(), 4);
        let Body::Tiles(head) = &g[0].body else {
            panic!()
        };
        assert!(head[0].open.is_none(), "the page does not lead to itself");
        let Body::Tiles(white) = &g[1].body else {
            panic!()
        };
        assert_eq!(white[0].name, "2\u{202f}300\u{a0}K, blanc chaud");
        let Part::Slider(s) = &white[0].parts[0] else {
            panic!()
        };
        assert_eq!(s.path, "/zigbee/lamps/0x01/temperature");
        assert_eq!((s.body)(42.4), serde_json::json!({"temperature": 42}));
        assert_eq!(s.ends, Some(("Chaud".into(), "Froid".into())));
        let Body::Parts(colors) = &g[2].body else {
            panic!()
        };
        let Part::Flow(c) = &colors[0] else { panic!() };
        assert_eq!(c.len(), 8);
        assert_eq!(c[0].label, "Rouge");
        assert_eq!(c[0].dot, Some(Rgba(239, 68, 68, 255)));
        let Act::Order(o) = &c[0].act else { panic!() };
        assert_eq!(o.posts[0].1.path, "/zigbee/lamps/0x01/color");
        let (x, y) = rgb_to_xy([239, 68, 68]);
        assert!((x - 0.6).abs() < 0.05 && (y - 0.3).abs() < 0.05, "{x} {y}");
        let Body::Parts(effects) = &g[3].body else {
            panic!()
        };
        let Part::Flow(e) = &effects[0] else { panic!() };
        assert_eq!(e.len(), 11);
        let Act::Order(o) = &e[10].act else { panic!() };
        assert_eq!(
            o.posts[0].1.body,
            Some(serde_json::json!({"effect": "stop_hue_effect"}))
        );
    }

    #[test]
    fn off_its_white_and_colors_stay_out_of_reach_saying_why() {
        let w = Words::new(Lang::Fr);
        let mut l = lit(true, Some(80));
        l.is_on = false;
        let g = lamp_groups(&w, ZIGBEE_DRIVER, &l, &now());
        let Body::Tiles(white) = &g[1].body else {
            panic!()
        };
        let [Part::Slider(s), Part::Hint(why)] = &white[0].parts[..] else {
            panic!()
        };
        assert!(s.off);
        assert_eq!(why, "Allume la lampe pour la régler.");
        let Body::Parts(colors) = &g[2].body else {
            panic!()
        };
        let Part::Flow(c) = &colors[0] else { panic!() };
        assert!(c.iter().all(|b| b.unavailable));
    }

    #[test]
    fn bluetooth_has_a_percentage_and_no_colors() {
        let w = Words::new(Lang::Fr);
        let g = lamp_groups(&w, HUE_DRIVER, &lit(false, Some(80)), &now());
        assert_eq!(g.len(), 2);
        let Body::Tiles(white) = &g[1].body else {
            panic!()
        };
        assert_eq!(white[0].name, "80\u{202f}%, blanc froid");
    }

    #[test]
    fn no_radio_no_group_and_reading_holds_the_place() {
        let w = Words::new(Lang::Fr);
        assert!(lamps_group(&w, &[], &now(), None).is_none());
        let loading = View::<Vec<Lamp>> {
            data: None,
            loading: true,
            failure: None,
            stale: false,
            at: None,
        };
        let g = lamps_group(&w, &[(HUE_DRIVER, &loading)], &now(), None).unwrap();
        assert!(matches!(g.body, Body::Loading));
        // one list failed, the other still reading: reading; both done, the failure said
        let failed = View::<Vec<Lamp>> {
            data: None,
            loading: false,
            failure: Some(crate::client::Failure::Unreachable("x".into())),
            stale: false,
            at: None,
        };
        let g = lamps_group(
            &w,
            &[(HUE_DRIVER, &failed), (ZIGBEE_DRIVER, &loading)],
            &now(),
            None,
        )
        .unwrap();
        assert!(matches!(g.body, Body::Loading));
        let g = lamps_group(&w, &[(HUE_DRIVER, &failed)], &now(), None).unwrap();
        assert!(matches!(g.body, Body::Failed { .. }));
        // one list failed, the other read: its lamps
        let one = view(vec![Lamp {
            id: "1".into(),
            name: "Salon".into(),
            reachable: true,
            connecting: false,
            last_seen: None,
            is_on: false,
            brightness: 10,
            temperature: None,
            color: false,
        }]);
        let g = lamps_group(
            &w,
            &[(HUE_DRIVER, &failed), (ZIGBEE_DRIVER, &one)],
            &now(),
            None,
        )
        .unwrap();
        assert!(matches!(&g.body, Body::Tiles(t) if t.len() == 1));
        let none = view(Vec::new());
        let g = lamps_group(&w, &[(HUE_DRIVER, &none)], &now(), None).unwrap();
        assert!(matches!(g.body, Body::Empty { .. }));
        assert!(g.actions.is_empty());
    }

    #[test]
    fn all_off_says_how_many() {
        let w = Words::new(Lang::Fr);
        let ok = all_off_said(
            &w,
            &Outcome {
                done: vec!["a".into(), "b".into()],
                failed: vec![],
                answer: None,
            },
        )
        .unwrap();
        assert_eq!(
            ok,
            Toast {
                text: "2 lampes éteintes".into(),
                warn: false
            }
        );
        let partial = all_off_said(
            &w,
            &Outcome {
                done: vec!["a".into()],
                failed: vec!["Salon".into()],
                answer: None,
            },
        )
        .unwrap();
        assert!(partial.warn);
        assert_eq!(partial.text, "1 lampe éteinte · sans réponse\u{a0}: Salon");
    }
}
