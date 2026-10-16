//! Meross plugs on the dashboard (PlugsGroup, LivePlugTile, PlugTile, meross/units.ts): the icon
//! turns a plug on or off, its live power read every 5 s and what it costs at the Tempo price in
//! force (« 42 W · 0,7 c€/h »); drawing power in a red day's peak hours, its line says so in the
//! warning style. A plug that does not answer is not asked (its tile stays, saying since when).

use chrono::{DateTime, TimeZone};
use maison_api::Tz;
use maison_api::meross::{Plug, ToggleRequest};

use super::tempo::{Period, PriceNow, price_now};
use super::{Cx, MEROSS, loaded, stale};
use crate::commands::{Order, Post, limit};
use crate::i18n::{Words, count, k};
use crate::live::View;
use crate::paths;
use crate::ui::icon;
use crate::ui::model::{Body, Button, Group, Lead, Tile};

/// Above this a plug draws enough to matter during red peak hours (standby leaks do not).
pub const NUDGE_WATTS: f64 = 5.0;
/// Under this, a cost per hour reads better in cents.
const CENTS_BELOW: f64 = 0.1;

/// « 42 W ».
pub fn watts(w: &Words, watts: f64) -> String {
    w.t(k::MEROSS_WATTS, &[("power", w.num(watts, 0).into())])
}

/// « 0,7 c€/h », « 0,76 €/h » (price.ts `perHour`).
pub fn per_hour(w: &Words, eur_per_hour: f64) -> String {
    if eur_per_hour < CENTS_BELOW {
        w.t(
            k::PRICE_CENTS_PER_HOUR,
            &[("cents", w.num(eur_per_hour * 100.0, 1).into())],
        )
    } else {
        w.t(
            k::PRICE_EUROS_PER_HOUR,
            &[("euros", w.num(eur_per_hour, 2).into())],
        )
    }
}

fn cost_per_hour(watts: f64, eur_per_kwh: f64) -> f64 {
    watts / 1000.0 * eur_per_kwh
}

/// « 42 W · 0,7 c€/h »: the power and what it costs at the price in force (none known: the
/// power alone).
pub fn power_and_cost(w: &Words, p: f64, eur_per_kwh: Option<f64>) -> String {
    match eur_per_kwh {
        Some(price) if p > 0.0 => w.t(
            k::MEROSS_POWER_COST,
            &[
                ("power", watts(w, p).into()),
                ("cost", per_hour(w, cost_per_hour(p, price)).into()),
            ],
        ),
        _ => watts(w, p),
    }
}

/// What a plug on and drawing power costs per hour during a red day's peak hours; `None` at
/// any other time, or when it draws next to nothing.
pub fn red_peak_cost(
    w: &Words,
    on: bool,
    power: Option<f64>,
    price: Option<&PriceNow>,
) -> Option<String> {
    let (p, price) = (power?, price?);
    let red_peak = price.color == maison_api::tempo::Color::Red && price.period == Period::Peak;
    (on && p >= NUDGE_WATTS && red_peak).then(|| per_hour(w, cost_per_hour(p, price.price)))
}

/// One plug's tile: `power` its live watts when it answers.
pub fn tile(
    w: &Words,
    plug: &Plug,
    power: Option<f64>,
    price: Option<&PriceNow>,
    now: &DateTime<Tz>,
) -> Tile {
    let toggle = |on: bool| {
        Order::one(
            format!("{MEROSS}:{}:toggle", plug.id),
            plug.name.clone(),
            Post::json(
                paths::of(paths::MEROSS, &plug.id, "toggle"),
                &ToggleRequest { on },
            ),
        )
        .toggle(on)
        .within(limit::PLUG)
        .then_refresh(&[MEROSS])
    };
    let nudge = if plug.is_online {
        red_peak_cost(w, plug.is_on, power, price)
            .map(|cost| w.t(k::MEROSS_RED_PEAK, &[("cost", cost.into())]))
    } else {
        None
    };
    let state = if !plug.is_online {
        let at = (plug.last_ping > 0)
            .then(|| now.timezone().timestamp_millis_opt(plug.last_ping).single())
            .flatten();
        w.unreachable_since(at, now)
    } else if plug.is_on {
        nudge.clone().unwrap_or_else(|| w.s(k::STATE_ON))
    } else {
        w.s(k::STATE_OFF)
    };
    Tile {
        name: plug.name.clone(),
        state,
        warn: nudge.is_some() && plug.is_on,
        lead: Lead::Toggle {
            icon: if plug.is_on {
                icon::PLUG_ZAP
            } else {
                icon::PLUG
            },
            on: plug.is_on,
            offline: !plug.is_online,
            turn_on: Box::new(toggle(true)),
            turn_off: Box::new(toggle(false)),
        },
        fact: if plug.is_online {
            power.map(|p| {
                power_and_cost(
                    w,
                    p,
                    if plug.is_on {
                        price.map(|p| p.price)
                    } else {
                        None
                    },
                )
            })
        } else {
            None
        },
        parts: Vec::new(),
    }
}

pub fn plugs_group(
    w: &Words,
    v: &View<Vec<Plug>>,
    total: Option<usize>,
    tiles: impl FnOnce(&[Plug]) -> Vec<Tile>,
    stale: Option<(String, Button)>,
) -> Group {
    let plugs = v.data.as_deref().map(Vec::as_slice).unwrap_or_default();
    let online = plugs.iter().filter(|p| p.is_online).count();
    let fact = (!plugs.is_empty()).then(|| {
        format!(
            "{} · {}",
            w.t(k::MEROSS_PLUG_COUNT, &[count(total.unwrap_or(plugs.len()))]),
            w.t(k::MEROSS_ONLINE_COUNT, &[count(online)])
        )
    });
    let body = loaded(
        w,
        v,
        &[MEROSS],
        (k::MEROSS_NONE, k::MEROSS_NONE_HINT),
        |plugs| (!plugs.is_empty()).then(|| Body::Tiles(tiles(plugs))),
    );
    Group {
        title: w.s(k::MEROSS_TITLE),
        fact,
        actions: Vec::new(),
        stale,
        body,
    }
}

pub fn group(cx: &mut Cx) -> Option<Group> {
    let list = cx.live.want(&cx.sources.plugs);
    let total = list.data.as_ref().map(|d| d.total);
    let v = list.map(|d| d.devices.clone());
    let now = cx.house_now();
    let tempo = cx.live.want(&cx.sources.tempo).data;
    let price = tempo.as_deref().and_then(|t| price_now(&now, t));
    // only a plug that answers is asked for its power
    let mut powers = std::collections::HashMap::new();
    for p in v
        .data
        .as_deref()
        .into_iter()
        .flatten()
        .filter(|p| p.is_online)
    {
        let source = cx.sources.electricity(&p.id).clone();
        if let Some(e) = cx.live.want(&source).data {
            powers.insert(p.id.clone(), e.electricity.raw.watts());
        }
    }
    let old = stale(cx, &v, &[MEROSS]);
    let w = cx.words;
    Some(plugs_group(
        w,
        &v,
        total,
        |plugs| {
            plugs
                .iter()
                .map(|p| tile(w, p, powers.get(&p.id).copied(), price.as_ref(), &now))
                .collect()
        },
        old,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;
    use crate::ui::model::Lead;
    use chrono::TimeZone;
    use maison_api::meross::{ElectricityAnswer, PlugList};
    use maison_api::tempo::Color;
    use maison_api::{Answer, HOUSE_TZ};

    const PLUGS: &str = r#"{"success":true,"devices":[{"id":"p1","name":"Bouilloire","ip":"192.168.1.20","isOnline":true,"isOn":true,"lastPing":1763000000000},{"id":"p2","name":"Lampe bureau","ip":"192.168.1.21","isOnline":false,"isOn":false,"lastPing":0}],"total":2,"message":"ok"}"#;

    fn now() -> DateTime<Tz> {
        HOUSE_TZ.with_ymd_and_hms(2026, 11, 12, 18, 0, 0).unwrap()
    }

    fn red_peak() -> PriceNow {
        PriceNow {
            color: Color::Red,
            period: Period::Peak,
            price: 0.7062,
            until: "22:00".into(),
            day_started: true,
            next: None,
        }
    }

    #[test]
    fn power_and_cost_in_words() {
        let w = Words::new(Lang::Fr);
        assert_eq!(watts(&w, 42.4), "42\u{a0}W");
        assert_eq!(
            power_and_cost(&w, 42.0, Some(0.1612)),
            "42\u{a0}W · 0,7\u{a0}c€/h"
        );
        assert_eq!(
            power_and_cost(&w, 1500.0, Some(0.7062)),
            "1500\u{a0}W · 1,06\u{a0}€/h".replace("1500", "1\u{202f}500")
        );
        assert_eq!(power_and_cost(&w, 0.0, Some(0.2)), "0\u{a0}W");
        assert_eq!(
            red_peak_cost(&w, true, Some(3.0), Some(&red_peak())),
            None,
            "standby does not count"
        );
    }

    #[test]
    fn plugs_from_the_backends_answers() {
        let w = Words::new(Lang::Fr);
        let list: PlugList = serde_json::from_str::<Answer<PlugList>>(PLUGS)
            .unwrap()
            .body;
        let e: ElectricityAnswer = serde_json::from_str::<Answer<ElectricityAnswer>>(
            r#"{"success":true,"device":{"id":"p1","name":"Bouilloire"},"message":"ok","electricity":{"voltage":"230.0V","current":"6.5A","power":"1500.0W","raw":{"channel":0,"current":6500,"voltage":2300,"power":1500000,"config":null}}}"#,
        )
        .unwrap()
        .body;
        let price = red_peak();
        let on = tile(
            &w,
            &list.devices[0],
            Some(e.electricity.raw.watts()),
            Some(&price),
            &now(),
        );
        assert_eq!(on.state, "Allumée en HP rouge · 1,06\u{a0}€/h");
        assert!(on.warn);
        let Lead::Toggle {
            turn_off: order, ..
        } = &on.lead
        else {
            panic!()
        };
        assert_eq!(order.posts[0].1.path, "/meross/p1/toggle");
        assert_eq!(
            order.posts[0].1.body,
            Some(serde_json::json!({"on": false}))
        );
        let off = tile(&w, &list.devices[1], None, Some(&price), &now());
        assert_eq!(off.state, "Injoignable");
        assert!(off.fact.is_none());
        let v = View::ready(list.devices.clone());
        let g = plugs_group(&w, &v, Some(2), |_| Vec::new(), None);
        assert_eq!(g.fact.as_deref(), Some("2 prises · 1 en ligne"));
    }
}
