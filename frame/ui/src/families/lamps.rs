//! Every lamp in one group, whatever its radio (LampsGroup, LampTile, lamps/lamp.ts): Hue over
//! Bluetooth and Zigbee lamps on the server's coordinator, a family left out when the server
//! runs without its radio. The icon turns a lamp on or off, line 2 says « Allumée, 80 % »; the
//! brightness slider only under a lamp that is on and reachable. « Tout éteindre » when one is.

use chrono::DateTime;
use maison_api::Tz;
use maison_api::lamps::{BrightnessRequest, HueLamp, LampList, PowerRequest, ZigbeeLamp};

use super::{Cx, HUE, ZIGBEE, loaded, stale};
use crate::commands::{Order, Outcome, Post, Toast, json, limit};
use crate::i18n::{Arg, Words, k};
use crate::live::View;
use crate::paths;
use crate::ui::icon;
use crate::ui::model::{Act, Body, Button, Group, Lead, Part, Slider, Tile};

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
            near: 2.0,
            ends: None,
            path: paths::of(d.base, &l.id, "brightness"),
            body: brightness,
            refresh: d.key,
            name: l.name.clone(),
        }));
    }
    Tile {
        name: l.name.clone(),
        state: state(w, l, now),
        warn: !l.reachable && !l.connecting,
        lead: Lead::Toggle {
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
        fact: None,
        parts,
    }
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
                    .said(all_off_said),
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
    Some(Group {
        title: w.s(k::ZIGBEE_LAMPS_TITLE),
        fact: None,
        actions,
        stale,
        body,
    })
}

/// A list's lamps, its read state kept.
fn as_lamps<L>(v: View<LampList<L>>, from: fn(&L) -> Lamp) -> View<Vec<Lamp>> {
    v.map(|d| d.lamps.iter().map(from).collect())
}

pub fn group(cx: &mut Cx) -> Option<Group> {
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
