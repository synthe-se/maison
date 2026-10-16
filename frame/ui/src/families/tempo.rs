//! EDF Tempo on the dashboard (TempoGroup, TempoNow; the words of tempo/now.ts, the price of
//! tempo/price.ts): line 1 the color and the price in force until when, line 2 what comes next.
//! Before 06:00 it says it as it is: yesterday's color is still in force (« Encore blanc
//! jusqu'à 6:00 »), today's starts at 06:00.

use chrono::{DateTime, Timelike};
use maison_api::Tz;
use maison_api::tempo::{Color, Forecast, Today};

use super::{Cx, TEMPO, loaded, stale};
use crate::i18n::{Arg, Words, k};
use crate::ui::model::{Body, Group, Lead, Tile};

/// RTE publishes tomorrow's color around 10:40 (minutes after midnight).
pub const ANNOUNCED_AT: u32 = 10 * 60 + 40;

pub use maison_api::tempo::{Period, PriceNow};

/// What a kWh costs at `now`; `None` without prices or without the color in force
/// (maison-api's `Today::price_at`, the backend's rule).
pub fn price_now(now: &DateTime<Tz>, data: &Today) -> Option<PriceNow> {
    data.price_at(now.with_timezone(&chrono::Utc))
}

/// A color's name (colors.ts `TEMPO[c].name`).
pub fn name(w: &Words, c: Color) -> String {
    w.s(match c {
        Color::Blue => k::COLOR_BLUE,
        Color::White => k::COLOR_WHITE,
        Color::Red => k::COLOR_RED,
    })
}

/// A day as said: published (« Rouge »), forecast (« Rouge probable · 62 % ») or unknown.
pub fn day_words(w: &Words, color: Option<Color>, forecast: bool, p: Option<f64>) -> String {
    let Some(c) = color else {
        return w.s(k::COMMON_UNKNOWN);
    };
    match (forecast, p) {
        (true, Some(p)) => w.t(
            k::TEMPO_COLOR_PROBABLE,
            &[
                ("color", name(w, c).into()),
                ("percent", w.percent(p).into()),
            ],
        ),
        _ => name(w, c),
    }
}

/// « 19,2 c€/kWh ».
pub fn cents(w: &Words, eur_per_kwh: f64) -> String {
    w.t(
        k::TEMPO_CENTS,
        &[("cents", w.num(eur_per_kwh * 100.0, 1).into())],
    )
}

/// Line 1: the color and the price in force until when.
pub fn now_line(w: &Words, data: &Today, current: Option<&PriceNow>) -> String {
    let Some(c) = current else {
        return day_words(w, data.today.color, false, None);
    };
    let period = w.s(if c.period == Period::Peak {
        k::TEMPO_HP
    } else {
        k::TEMPO_HC
    });
    let price = format!("{period} {}", cents(w, c.price));
    let time = w.hhmm(&c.until);
    if c.day_started {
        w.t(
            k::TEMPO_IN_FORCE,
            &[
                ("color", name(w, c.color).into()),
                ("price", price.into()),
                ("time", time.into()),
            ],
        )
    } else {
        w.t(
            k::TEMPO_STILL,
            &[
                ("color", w.lower(&name(w, c.color)).into()),
                ("price", price.into()),
                ("time", time.into()),
            ],
        )
    }
}

/// Line 2: what comes next. Before 06:00 today's color, from 06:00; after, tomorrow: RTE's,
/// else « annoncé vers 10 h 40 » (before then) and Maison's forecast; empty when nothing is
/// known.
pub fn next_line(
    w: &Words,
    data: &Today,
    current: Option<&PriceNow>,
    forecast: Option<&Forecast>,
    now: &DateTime<Tz>,
) -> String {
    if let Some(c) = current
        && !c.day_started
    {
        return match c.next {
            Some(n) => w.t(
                k::TEMPO_FROM,
                &[
                    ("color", name(w, n).into()),
                    ("time", w.hhmm(&c.until).into()),
                ],
            ),
            None => String::new(),
        };
    }
    if let Some(color) = data.tomorrow.color {
        return w.t(
            k::TEMPO_TOMORROW_IS,
            &[("words", day_words(w, Some(color), false, None).into())],
        );
    }
    let f = forecast.and_then(|f| f.days.iter().find(|d| d.date == data.tomorrow.date));
    let mut parts = Vec::new();
    if now.hour() * 60 + now.minute() < ANNOUNCED_AT {
        parts.push(w.s(k::TEMPO_ANNOUNCED_AT));
    }
    if let Some(f) = f {
        parts.push(day_words(w, Some(f.color), !f.official, Some(f.confidence)));
    }
    let words = if parts.is_empty() {
        day_words(w, None, false, None)
    } else {
        parts.join(" · ")
    };
    w.t(k::TEMPO_TOMORROW_IS, &[("words", Arg::Text(words))])
}

/// The Tempo tile, from what /tempo and /tempo/forecast said.
pub fn tile(w: &Words, data: &Today, forecast: Option<&Forecast>, now: &DateTime<Tz>) -> Tile {
    let current = price_now(now, data);
    let next = next_line(w, data, current.as_ref(), forecast, now);
    Tile {
        name: now_line(w, data, current.as_ref()),
        state: if next.is_empty() {
            w.s(k::TEMPO_OPEN)
        } else {
            next
        },
        warn: false,
        lead: Lead::Swatch(current.as_ref().map(|c| c.color).or(data.today.color)),
        fact: None,
        parts: Vec::new(),
    }
}

pub fn group(cx: &mut Cx) -> Option<Group> {
    let today = cx.live.want(&cx.sources.tempo);
    let forecast = cx.live.want(&cx.sources.forecast);
    let now = cx.house_now();
    let w = cx.words;
    let body = loaded(w, &today, &[TEMPO], ("", ""), |data| {
        Some(Body::Tiles(vec![tile(
            w,
            data,
            forecast.data.as_deref(),
            &now,
        )]))
    });
    Some(Group {
        title: w.s(k::NAV_TEMPO),
        fact: None,
        actions: Vec::new(),
        stale: stale(cx, &today, &[TEMPO]),
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;
    use chrono::TimeZone;
    use maison_api::Answer;
    use maison_api::HOUSE_TZ;

    fn today(yesterday: &str, today: &str, tomorrow: &str) -> Today {
        let color = |c: &str| {
            if c.is_empty() {
                "null".to_string()
            } else {
                format!("\"{c}\"")
            }
        };
        let json = format!(
            r#"{{"success":true,"yesterday":{{"date":"2026-11-11","color":{}}},"today":{{"date":"2026-11-12","color":{}}},"tomorrow":{{"date":"2026-11-13","color":{}}},"tariffs":{{"blue":{{"offPeak":0.1325,"peak":0.1612}},"white":{{"offPeak":0.1499,"peak":0.1871}},"red":{{"offPeak":0.1575,"peak":0.7062}},"subscription":null,"startsOn":"2026-02-01"}},"hours":{{"peakStart":"06:00","peakEnd":"22:00"}},"stock":{{"season":"2026-2027","blue":{{"used":40,"total":300,"remaining":260}},"white":{{"used":3,"total":43,"remaining":40}},"red":{{"used":0,"total":22,"remaining":22}}}},"lastUpdated":null,"cached":false}}"#,
            color(yesterday),
            color(today),
            color(tomorrow)
        );
        serde_json::from_str::<Answer<Today>>(&json).unwrap().body
    }

    fn at(h: u32, m: u32) -> DateTime<Tz> {
        HOUSE_TZ.with_ymd_and_hms(2026, 11, 12, h, m, 0).unwrap()
    }

    #[test]
    fn in_the_day_the_color_and_price_in_force_then_tomorrow() {
        let w = Words::new(Lang::Fr);
        let t = tile(&w, &today("WHITE", "RED", "BLUE"), None, &at(14, 0));
        assert_eq!(t.name, "Rouge · HP 70,6\u{a0}c€/kWh jusqu’à 22:00");
        assert_eq!(t.state, "Demain\u{a0}: Bleu");
        assert!(matches!(t.lead, Lead::Swatch(Some(Color::Red))));
    }

    #[test]
    fn before_six_yesterdays_color_still_holds() {
        let w = Words::new(Lang::Fr);
        let d = today("WHITE", "RED", "");
        let t = tile(&w, &d, None, &at(5, 30));
        assert_eq!(t.name, "Encore blanc jusqu’à 6:00 · HC 15,0\u{a0}c€/kWh");
        assert_eq!(t.state, "Rouge à partir de 6:00");
        assert!(matches!(t.lead, Lead::Swatch(Some(Color::White))));
    }

    #[test]
    fn tomorrow_unknown_is_announced_then_forecast() {
        let w = Words::new(Lang::Fr);
        let d = today("BLUE", "BLUE", "");
        let forecast: Forecast = serde_json::from_str::<Answer<Forecast>>(
            r#"{"success":true,"issued":"2026-11-12","weatherIssued":null,"stale":false,"model":null,"days":[{"date":"2026-11-13","horizon":1,"official":false,"color":"WHITE","probabilities":{"BLUE":0.3,"WHITE":0.62,"RED":0.08},"confidence":0.62}],"stock":{"season":"2026-2027","blue":{"used":40,"total":300,"remaining":260},"white":{"used":3,"total":43,"remaining":40},"red":{"used":0,"total":22,"remaining":22}}}"#,
        )
        .unwrap()
        .body;
        let morning = tile(&w, &d, Some(&forecast), &at(9, 0));
        assert_eq!(
            morning.state,
            "Demain\u{a0}: annoncé vers 10 h 40 · Blanc probable · 62\u{202f}%"
        );
        let noon = tile(&w, &d, Some(&forecast), &at(12, 0));
        assert_eq!(noon.state, "Demain\u{a0}: Blanc probable · 62\u{202f}%");
        let nothing = tile(&w, &d, None, &at(12, 0));
        assert_eq!(nothing.state, "Demain\u{a0}: Inconnu");
    }

    #[test]
    fn without_prices_the_color_alone() {
        let w = Words::new(Lang::Fr);
        let mut d = today("BLUE", "RED", "");
        d.tariffs = None;
        assert_eq!(tile(&w, &d, None, &at(14, 0)).name, "Rouge");
    }
}
