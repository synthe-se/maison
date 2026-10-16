//! Every lamp in one group, whatever its radio (LampsGroup, LampTile, lamps/lamp.ts): Hue over
//! Bluetooth and Zigbee lamps on the server's coordinator, a family left out when the server
//! runs without its radio. The icon turns a lamp on or off, line 2 says « Allumée, 80 % »; the
//! brightness slider only under a lamp that is on and reachable. « Tout éteindre » when one is.

use chrono::DateTime;
use chrono_tz::Tz;
use maison_api::lamps::{HueLamp, LampList, ZigbeeLamp};
use serde_json::json;

use super::{Cx, HUE, ZIGBEE, path_segment, stale};
use crate::commands::{Order, Outcome, Toast, limit};
use crate::i18n::{Arg, Words, k};
use crate::live::View;
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
    base: "/hue-lamps",
};
pub const ZIGBEE_DRIVER: Driver = Driver {
    key: ZIGBEE,
    base: "/zigbee/lamps",
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

fn power(d: Driver, l: &Lamp, on: bool) -> Order {
    Order::one(
        format!("{}:{}:power", d.key, l.id),
        l.name.clone(),
        format!("{}/{}/power", d.base, path_segment(&l.id)),
        Some(json!({ "enabled": on })),
    )
    .toggle(on)
    .within(limit::LAMP)
    .then_refresh(&[d.key])
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
            path: format!("{}/{}/brightness", d.base, path_segment(&l.id)),
            field: "brightness",
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
                "lightbulb"
            } else {
                "lightbulb-off"
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
            .map(|(d, l)| (l.name.clone(), power(*d, l, false).posts.remove(0).1))
            .collect();
        actions.push(Button::new(
            Some("lightbulb-off"),
            w.s(k::LAMPS_ALL_OFF),
            Act::Order(Order {
                key: "lamps:all-off".into(),
                posts,
                target: None,
                limit: limit::LAMP,
                refresh: vec![HUE, ZIGBEE],
                say: Some(all_off_said),
            }),
        ));
    }
    // the two lists read as one: loading while one is, failed when one did and nothing is known
    let loading = families.iter().any(|(_, v)| v.loading) && rows.is_empty();
    let failed = families.iter().any(|(_, v)| v.failed) && rows.is_empty();
    let body = if loading {
        Body::Loading
    } else if failed {
        Body::Failed {
            text: w.s(k::ERROR_NETWORK),
            retry: Button::new(
                Some("refresh-cw"),
                w.s(k::COMMON_RETRY),
                Act::Refresh(LAMPS),
            ),
        }
    } else if rows.is_empty() {
        Body::Empty {
            text: w.s(k::LAMPS_NONE),
            hint: w.s(k::LAMPS_NONE_HINT),
        }
    } else {
        Body::Tiles(rows.iter().map(|(d, l)| tile(w, *d, l, now)).collect())
    };
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
    View {
        data: v
            .data
            .map(|d| std::sync::Arc::new(d.lamps.iter().map(from).collect())),
        loading: v.loading,
        failed: v.failed,
        stale: v.stale,
        at: v.at,
    }
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
    use std::sync::Arc;

    fn view(lamps: Vec<Lamp>) -> View<Vec<Lamp>> {
        View {
            data: Some(Arc::new(lamps)),
            loading: false,
            failed: false,
            stale: false,
            at: None,
        }
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
        assert_eq!(order.posts[0].1.body, Some(json!({"enabled": false})));
        assert_eq!(order.target, Some(false));
        let Part::Slider(s) = &tiles[0].parts[0] else {
            panic!("slider")
        };
        assert_eq!(s.value, 80.0);
        assert!(s.live);
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
            failed: false,
            stale: false,
            at: None,
        };
        let g = lamps_group(&w, &[(HUE_DRIVER, &loading)], &now(), None).unwrap();
        assert!(matches!(g.body, Body::Loading));
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
