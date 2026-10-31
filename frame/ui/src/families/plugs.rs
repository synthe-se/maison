//! Meross plugs on the dashboard (PlugsGroup, LivePlugTile, PlugTile, meross/units.ts): the icon
//! turns a plug on or off, its live power read every 5 s and what it costs at the Tempo price in
//! force (« 42 W · 0,7 c€/h »); drawing power in a red day's peak hours, its line says so in the
//! warning style. A plug that does not answer is not asked (its tile stays, saying since when).
//! Its row opens its page (PlugPage): volts, amps, watts, its last days, the month's estimate.

use chrono::{DateTime, Datelike, TimeZone};
use maison_api::Tz;
use maison_api::meross::{Consumption, ElectricityRaw, Plug, ToggleRequest};
use maison_api::tempo::{Calendar, Today, season_of};

use super::tempo::{Period, PriceNow, color_on, month_estimate, price_now};
use super::{Cx, MEROSS, fact, loaded, stale};
use crate::commands::{Order, Post, limit};
use crate::i18n::{Words, count, k};
use crate::live::View;
use crate::paths;
use crate::ui::icon;
use crate::ui::model::{Act, Body, Button, Group, Lead, Part, Screen, Tile, View as Page};

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
    let fact = plug
        .is_online
        .then(|| {
            let price = price.filter(|_| plug.is_on).map(|p| p.price);
            power.map(|p| power_and_cost(w, p, price))
        })
        .flatten();
    Tile::new(
        plug.name.clone(),
        state,
        Lead::Toggle {
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
    )
    .warn(nudge.is_some() && plug.is_on)
    .fact(fact)
    .open(Some(Act::Open(Page::Plug(plug.id.clone()))))
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
    Group::new(w.s(k::MEROSS_TITLE), body)
        .anchor(MEROSS)
        .fact(fact)
        .stale(stale)
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

/// « 4,12 € ».
fn euros(w: &Words, e: f64) -> String {
    w.t(k::PRICE_EUROS, &[("euros", w.num(e, 2).into())])
}

/// « 0,123 kWh ».
fn kwh(w: &Words, energy: f64) -> String {
    w.t(k::MEROSS_KWH, &[("energy", w.num(energy, 3).into())])
}

/// What was read for a plug's page.
pub struct Reads<'a> {
    pub electricity: Option<&'a ElectricityRaw>,
    pub days: Option<&'a Consumption>,
    pub today: Option<&'a Today>,
    pub season: Option<&'a Calendar>,
}

/// A plug's days, newest first (the last week), with the month's estimate above them.
fn consumption(w: &Words, c: &Consumption, r: &Reads, now: &DateTime<Tz>) -> Option<Group> {
    if c.consumption.is_empty() {
        return None;
    }
    let days: Vec<(String, String)> = c
        .consumption
        .iter()
        .rev()
        .take(7)
        .map(|d| {
            let day = chrono::NaiveDate::parse_from_str(&d.date, "%Y-%m-%d")
                .map_or_else(|_| d.date.clone(), |day| w.short_day(day));
            (day, kwh(w, f64::from(d.value) / 1000.0))
        })
        .collect();
    let month = now.format("%Y-%m").to_string();
    let estimate = r.today.and_then(|t| {
        let e = month_estimate(
            &c.consumption,
            |day| color_on(day, r.season, Some(t)),
            t.tariffs.as_ref()?,
            &t.hours,
            &month,
        );
        (e.days > 0).then_some((e, t.hours.peak_share()))
    });
    let days_said = format!(
        "{} · {}",
        w.t(k::MEROSS_CONSUMPTION_DAYS, &[count(c.summary.days)]),
        kwh(w, c.summary.total_kwh)
    );
    let mut parts = Vec::new();
    let tile = match estimate {
        Some((e, peak)) => {
            parts.push(Part::Hint(w.t(
                k::MEROSS_ESTIMATE_HINT,
                &[count(e.days), ("peak", w.percent(peak).into())],
            )));
            fact(
                icon::EURO,
                &w.t(
                    k::MEROSS_MONTH_SO_FAR,
                    &[("cost", euros(w, e.euros).into())],
                ),
                format!("{} · {days_said}", w.s(k::MEROSS_ESTIMATE)),
                false,
                Vec::new(),
            )
        }
        None => fact(
            icon::GAUGE,
            &kwh(w, c.summary.total_kwh),
            days_said,
            false,
            Vec::new(),
        ),
    };
    parts.push(Part::Facts(days));
    Some(Group::new(
        w.s(k::MEROSS_CONSUMPTION),
        Body::Tiles(vec![tile.parts(parts)]),
    ))
}

/// What a plug's page shows: its tile, what it reports now, its days.
pub fn plug_groups(
    w: &Words,
    plug: &Plug,
    r: &Reads,
    price: Option<&PriceNow>,
    now: &DateTime<Tz>,
) -> Vec<Group> {
    let power = r.electricity.map(ElectricityRaw::watts);
    let mut head = tile(w, plug, power, price, now).open(None);
    if !plug.is_online {
        head.parts
            .push(Part::Hint(w.s(k::MEROSS_NOT_CONNECTED_DESCRIPTION)));
    }
    let mut groups = vec![Group::new(
        w.s(k::MEROSS_PLUG_CONTROL),
        Body::Tiles(vec![head]),
    )];
    if let Some(e) = r.electricity {
        let volts = w.t(k::MEROSS_VOLTS, &[("voltage", w.num(e.volts(), 1).into())]);
        let amps = w.t(
            k::MEROSS_MILLIAMPS,
            &[("current", w.num(e.amperes() * 1000.0, 0).into())],
        );
        let power = w.t(k::MEROSS_WATTS, &[("power", w.num(e.watts(), 1).into())]);
        groups.push(Group::new(
            w.s(k::MEROSS_ELECTRICITY),
            Body::Parts(vec![Part::Facts(vec![
                (w.s(k::MEROSS_VOLTAGE), volts),
                (w.s(k::MEROSS_CURRENT), amps),
                (w.s(k::MEROSS_POWER), power),
            ])]),
        ));
    }
    groups.extend(r.days.and_then(|c| consumption(w, c, r, now)));
    groups
}

/// A plug's page, from the list; the list unread says so, a plug it lacks is « introuvable ».
pub fn page(cx: &mut Cx, id: &str) -> Screen {
    let list = cx.live.want(&cx.sources.plugs);
    let now = cx.house_now();
    let w = cx.words;
    let found = list
        .data
        .as_deref()
        .and_then(|l| l.devices.iter().find(|p| p.id == id).cloned());
    let Some(plug) = found else {
        let title = if list.loading {
            k::COMMON_LOADING
        } else {
            k::MEROSS_NOT_FOUND
        };
        let body = loaded(w, &list, &[MEROSS], (k::MEROSS_NOT_FOUND, ""), |_| None);
        return super::page(
            w,
            w.s(title),
            vec![Group::new(w.s(k::MEROSS_PLUG_CONTROL), body)],
        );
    };
    let today = cx.live.want(&cx.sources.tempo).data;
    let price = today.as_deref().and_then(|t| price_now(&now, t));
    let electricity = if plug.is_online {
        let source = cx.sources.electricity(id).clone();
        cx.live.want(&source).data
    } else {
        None
    };
    let source = cx.sources.consumption(id).clone();
    let days = cx.live.want(&source).data;
    let source = cx
        .sources
        .calendar(&season_of(now.year(), now.month0()))
        .clone();
    let season = cx.live.want(&source).data;
    let reads = Reads {
        electricity: electricity.as_deref().map(|e| &e.electricity.raw),
        days: days.as_deref().map(|d| &d.body),
        today: today.as_deref(),
        season: season.as_deref(),
    };
    let old = stale(cx, &list, &[MEROSS]);
    let mut groups = plug_groups(w, &plug, &reads, price.as_ref(), &now);
    groups[0].stale = old;
    super::page(w, plug.name.clone(), groups)
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

    const CONSUMPTION: &str = r#"{"consumption":[{"date":"2026-10-31","time":1,"value":500},{"date":"2026-11-11","time":2,"value":1000},{"date":"2026-11-12","time":3,"value":2000}],"summary":{"days":3,"totalWh":3500,"totalKwh":3.5}}"#;

    fn plug(on: bool, online: bool) -> Plug {
        Plug {
            id: "p1".into(),
            name: "Bouilloire".into(),
            ip: "192.168.1.20".into(),
            is_online: online,
            is_on: on,
            last_ping: 0,
        }
    }

    fn today() -> Today {
        serde_json::from_str(r#"{"yesterday":{"date":"2026-11-11","color":"WHITE"},"today":{"date":"2026-11-12","color":"RED"},"tomorrow":{"date":"2026-11-13","color":null},"tariffs":{"blue":{"offPeak":0.13,"peak":0.16},"white":{"offPeak":0.15,"peak":0.3},"red":{"offPeak":0.16,"peak":0.7},"subscription":null,"startsOn":"2026-08-01"},"hours":{"peakStart":"06:00","peakEnd":"22:00"},"stock":{"season":"2026-2027","blue":{"used":0,"total":300,"remaining":300},"white":{"used":0,"total":43,"remaining":43},"red":{"used":0,"total":22,"remaining":22}},"lastUpdated":null,"cached":false}"#).unwrap()
    }

    #[test]
    fn a_plug_opens_its_page() {
        let w = Words::new(Lang::Fr);
        let t = tile(&w, &plug(true, true), None, None, &now());
        assert!(matches!(&t.open, Some(Act::Open(Page::Plug(id))) if id == "p1"));
    }

    #[test]
    fn its_page_says_volts_amps_its_days_and_the_month() {
        let w = Words::new(Lang::Fr);
        let raw = ElectricityRaw {
            channel: 0,
            current: 183,
            voltage: 2301,
            power: 42_000,
            config: None,
        };
        let days: Consumption = serde_json::from_str(CONSUMPTION).unwrap();
        let t = today();
        let reads = Reads {
            electricity: Some(&raw),
            days: Some(&days),
            today: Some(&t),
            season: None,
        };
        let g = plug_groups(&w, &plug(true, true), &reads, None, &now());
        assert_eq!(g.len(), 3);
        let Body::Tiles(head) = &g[0].body else {
            panic!()
        };
        assert!(head[0].open.is_none());
        let Body::Parts(e) = &g[1].body else { panic!() };
        let Part::Facts(f) = &e[0] else { panic!() };
        assert_eq!(f[0].1, "230,1\u{a0}V");
        assert_eq!(f[1].1, "183\u{a0}mA");
        assert_eq!(f[2].1, "42,0\u{a0}W");
        let Body::Tiles(month) = &g[2].body else {
            panic!()
        };
        // 1 kWh white, 2 kWh red, 2/3 of each at the peak price; October's day is not this month
        let euros_ = (0.3 * 2.0 + 0.15) / 3.0 + 2.0 * (0.7 * 2.0 + 0.16) / 3.0;
        assert_eq!(
            month[0].name,
            format!("Ce mois-ci\u{a0}: {}\u{a0}€", w.num(euros_, 2))
        );
        assert_eq!(
            month[0].state,
            "estimation · 3 derniers jours · 3,500\u{a0}kWh"
        );
        let [Part::Hint(hint), Part::Facts(rows)] = &month[0].parts[..] else {
            panic!("{:?}", month[0].parts)
        };
        assert!(hint.contains('2'), "two days counted: {hint}");
        assert_eq!(rows[0].0, "jeu. 12 nov.", "newest first");
        assert_eq!(rows[0].1, "2,000\u{a0}kWh");
    }

    #[test]
    fn offline_it_says_so_and_without_prices_no_estimate() {
        let w = Words::new(Lang::Fr);
        let days: Consumption = serde_json::from_str(CONSUMPTION).unwrap();
        let reads = Reads {
            electricity: None,
            days: Some(&days),
            today: None,
            season: None,
        };
        let g = plug_groups(&w, &plug(false, false), &reads, None, &now());
        assert_eq!(g.len(), 2, "nothing read now");
        let Body::Tiles(head) = &g[0].body else {
            panic!()
        };
        assert!(matches!(&head[0].parts[..], [Part::Hint(_)]));
        let Body::Tiles(month) = &g[1].body else {
            panic!()
        };
        assert_eq!(month[0].name, "3,500\u{a0}kWh");
    }

    #[test]
    fn plugs_from_the_backends_answers() {
        let w = Words::new(Lang::Fr);
        let list: PlugList = serde_json::from_str::<Answer<PlugList>>(PLUGS)
            .unwrap()
            .body;
        let e: ElectricityAnswer = serde_json::from_str::<Answer<ElectricityAnswer>>(
            r#"{"success":true,"device":{"id":"p1","name":"Bouilloire"},"message":"ok","electricity":{"voltage":"230V","current":"6.5A","power":"1500W","raw":{"channel":0,"current":6500,"voltage":2300,"power":1500000,"config":null}}}"#,
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
