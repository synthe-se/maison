//! « Maintenant », the top of the dashboard (NowStrip, CatChip, PlugChip): one row of chips
//! summing up the house, in a fixed order: the Tempo price in force until when (a warning in a
//! red day's peak hours), the lamps lit, the next shutter move, what needs a hand (a cat
//! device, a plug drawing power in red peak hours). The same reads as the groups (the same
//! sources: no request of its own); a summary that never reorders them.

use chrono::DateTime;
use maison_api::Tz;
use maison_api::matter::Cover;
use maison_api::meross::Plug;
use maison_api::tempo::{Color, Period, PriceNow};
use serde_json::Value;

use super::lamps::Lamp;
use super::shutters::next_event;
use super::tempo::{name, price_now};
use super::{Cx, LAMPS_GROUP, MEROSS, SHUTTERS, TEMPO, TUYA, cats, lamps, plugs};
use crate::i18n::{Words, count, k};
use crate::ui::model::Chip;
use maison_api::matter::SunEvent;

/// A word inside a sentence: its first letter lower (the web's `inSentence`).
fn in_sentence(text: &str) -> String {
    let mut c = text.chars();
    match c.next() {
        Some(first) => first.to_lowercase().chain(c).collect(),
        None => String::new(),
    }
}

/// « Rouge HP · 70,6 c€ jusqu'à 22:00 ».
pub fn tempo_chip(w: &Words, p: &PriceNow) -> Chip {
    let period = w.s(if p.period == Period::Peak {
        k::TEMPO_HP
    } else {
        k::TEMPO_HC
    });
    Chip {
        text: w.t(
            k::NOW_TEMPO,
            &[
                ("color", name(w, p.color).into()),
                ("period", period.into()),
                ("cents", w.num(p.price * 100.0, 1).into()),
                ("time", w.hhmm(&p.until).into()),
            ],
        ),
        warn: p.color == Color::Red && p.period == Period::Peak,
        to: TEMPO,
    }
}

/// « 2 lampes allumées »: those reachable and on; none lit, no chip.
pub fn lamps_chip(w: &Words, all: &[Lamp]) -> Option<Chip> {
    let lit = all.iter().filter(|l| l.reachable && l.is_on).count();
    (lit > 0).then(|| Chip {
        text: w.t(k::NOW_LAMPS_ON, &[count(lit)]),
        warn: false,
        to: LAMPS_GROUP,
    })
}

/// « Volet salon ferme à 20:24 »: the soonest move, among the shutters online, not skipped.
pub fn shutter_chip(w: &Words, covers: &[Cover]) -> Option<Chip> {
    let (c, (event, at, _)) = covers
        .iter()
        .filter(|c| c.online)
        .filter_map(|c| {
            next_event(c)
                .filter(|(_, _, skipped)| !skipped)
                .map(|e| (c, e))
        })
        .min_by_key(|(_, (_, at, _))| *at)?;
    let time = w.clock(at.with_timezone(&maison_api::HOUSE_TZ).time());
    let key = if event == SunEvent::Open {
        k::SHUTTERS_WILL_OPEN
    } else {
        k::SHUTTERS_WILL_CLOSE
    };
    Some(Chip {
        text: w.t(
            key,
            &[("name", c.name.as_str().into()), ("time", time.into())],
        ),
        warn: false,
        to: SHUTTERS,
    })
}

/// « Fontaine : eau basse »: a cat device online whose status needs a hand.
pub fn cat_chip(w: &Words, device: &Value, status: Option<&Value>) -> Option<Chip> {
    if device["connected"].as_bool() != Some(true) {
        return None;
    }
    let kind = device["type"].as_str()?;
    let (text, warn) = cats::status_line(w, kind, status?)?;
    warn.then(|| Chip {
        text: w.t(
            k::NOW_ALERT,
            &[
                ("name", device["name"].as_str().unwrap_or_default().into()),
                ("what", in_sentence(&text).into()),
            ],
        ),
        warn: true,
        to: TUYA,
    })
}

/// « Lave-linge en HP rouge · 0,76 €/h »: a plug on, drawing power in red peak hours.
pub fn plug_chip(
    w: &Words,
    plug: &Plug,
    power: Option<f64>,
    price: Option<&PriceNow>,
) -> Option<Chip> {
    if !plug.is_online {
        return None;
    }
    let cost = plugs::red_peak_cost(w, plug.is_on, power, price)?;
    Some(Chip {
        text: w.t(
            k::NOW_RED_PEAK,
            &[("name", plug.name.as_str().into()), ("cost", cost.into())],
        ),
        warn: true,
        to: MEROSS,
    })
}

/// The chips, from what the groups read.
pub fn chips(cx: &mut Cx, now: &DateTime<Tz>) -> Vec<Chip> {
    let w = cx.words;
    let mut out = Vec::new();
    let price = cx
        .live
        .want(&cx.sources.tempo)
        .data
        .as_deref()
        .and_then(|t| price_now(now, t));
    if let Some(p) = &price {
        out.push(tempo_chip(w, p));
    }
    out.extend(lamps_chip(w, &lamps::all(cx)));
    if let Some(c) = cx.live.want(&cx.sources.covers).data.as_deref() {
        out.extend(shutter_chip(w, &c.covers));
    }
    let devices: Vec<Value> = cx
        .live
        .want(&cx.sources.cats)
        .data
        .as_deref()
        .and_then(|d| d["devices"].as_array().cloned())
        .unwrap_or_default();
    for d in &devices {
        let (id, kind) = (
            d["id"].as_str().unwrap_or_default(),
            d["type"].as_str().unwrap_or("unknown"),
        );
        if kind == "unknown" || id.is_empty() {
            continue;
        }
        let source = cx.sources.cat_status(id, kind).clone();
        let status = cx.live.want(&source).data;
        out.extend(cat_chip(w, d, status.as_deref()));
    }
    let list = cx.live.want(&cx.sources.plugs).data;
    for p in list
        .as_deref()
        .map(|l| l.devices.as_slice())
        .unwrap_or_default()
    {
        let power = if p.is_online {
            let source = cx.sources.electricity(&p.id).clone();
            cx.live
                .want(&source)
                .data
                .map(|e| e.electricity.raw.watts())
        } else {
            None
        };
        out.extend(plug_chip(w, p, power, price.as_ref()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;

    fn lamp(on: bool, reachable: bool) -> Lamp {
        Lamp {
            id: "1".into(),
            name: "Salon".into(),
            reachable,
            connecting: false,
            last_seen: None,
            is_on: on,
            brightness: 80,
            temperature: None,
            color: false,
        }
    }

    #[test]
    fn the_price_in_force_a_warning_in_red_peak_hours() {
        let w = Words::new(Lang::Fr);
        let p = PriceNow {
            color: Color::Red,
            period: Period::Peak,
            price: 0.7062,
            until: "22:00".into(),
            day_started: true,
            next: None,
        };
        let c = tempo_chip(&w, &p);
        assert_eq!(c.text, "Rouge HP · 70,6\u{a0}c€ jusqu’à 22:00");
        assert!(c.warn);
        assert_eq!(c.to, TEMPO, "each chip leads to its group");
        let blue = tempo_chip(
            &w,
            &PriceNow {
                color: Color::Blue,
                ..p
            },
        );
        assert!(!blue.warn);
    }

    #[test]
    fn only_lamps_reachable_and_on_count() {
        let w = Words::new(Lang::Fr);
        let all = [
            lamp(true, true),
            lamp(true, false),
            lamp(false, true),
            lamp(true, true),
        ];
        assert_eq!(lamps_chip(&w, &all).unwrap().text, "2 lampes allumées");
        assert!(lamps_chip(&w, &[lamp(false, true)]).is_none());
    }

    #[test]
    fn a_cat_device_appears_only_when_it_needs_a_hand() {
        let w = Words::new(Lang::Fr);
        let fountain: Value = serde_json::from_str(
            r#"{"id":"f1","name":"Fontaine","type":"fountain","connected":true}"#,
        )
        .unwrap();
        let low: Value = serde_json::from_str(r#"{"waterLevel":"low"}"#).unwrap();
        let ok: Value = serde_json::from_str(r#"{"waterLevel":"ok"}"#).unwrap();
        let c = cat_chip(&w, &fountain, Some(&low)).unwrap();
        assert_eq!(c.text, "Fontaine\u{a0}: eau basse");
        assert!(c.warn);
        assert!(cat_chip(&w, &fountain, Some(&ok)).is_none());
    }
}
