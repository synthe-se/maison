//! EDF Tempo on the dashboard (TempoGroup, TempoNow; the words of tempo/now.ts, the price of
//! tempo/price.ts): line 1 the color and the price in force until when, line 2 what comes next.
//! Before 06:00 it says it as it is: yesterday's color is still in force (« Encore blanc
//! jusqu'à 6:00 »), today's starts at 06:00.

use chrono::NaiveDate;
use chrono::{DateTime, Timelike};
use maison_api::Tz;
use maison_api::tempo::{Color, Forecast, ForecastDay, SeasonStock, Today};

use super::{Cx, TEMPO, loaded, stale};
use crate::i18n::{Arg, Words, k};
use crate::ui::icon;
use crate::ui::model::{Act, Body, Button, Group, Lead, Part, Screen, Tile, View};

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
    Tile::new(
        now_line(w, data, current.as_ref()),
        if next.is_empty() {
            w.s(k::TEMPO_OPEN)
        } else {
            next
        },
        Lead::Swatch(current.as_ref().map(|c| c.color).or(data.today.color)),
    )
    .open(Some(Act::Open(View::Tempo)))
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
    Some(Group::new(w.s(k::NAV_TEMPO), body).stale(stale(cx, &today, &[TEMPO])))
}

// ── the Tempo page (routes/tempo-predictions: the week, the prices, the season) ──

/// Tempo's colours in their order (colors.ts `TEMPO_COLORS`).
const COLORS: [Color; 3] = [Color::Blue, Color::White, Color::Red];

/// « 0,1612 €/kWh »: TempoPrices' `price`.
pub fn price(w: &Words, eur_per_kwh: f64) -> String {
    w.t(k::TEMPO_PRICE, &[("price", w.num(eur_per_kwh, 4).into())])
}

/// « HP bleu »: price.ts `periodColor`.
pub fn period_color(w: &Words, period: Period, color: Color) -> String {
    let p = w.s(if period == Period::Peak {
        k::TEMPO_HP
    } else {
        k::TEMPO_HC
    });
    w.t(
        k::TEMPO_PERIOD_COLOR,
        &[
            ("period", p.into()),
            ("color", w.lower(&name(w, color)).into()),
        ],
    )
}

/// One day of the week (ForecastCard): RTE's colour once published; else Maison's forecast,
/// its probabilities as one bar, and how often it was right this far ahead.
pub fn day_tile(w: &Words, d: &ForecastDay, today: NaiveDate) -> Tile {
    let parts = if d.official {
        vec![Part::Hint(w.s(k::TEMPO_OFFICIAL))]
    } else {
        let p = d.probabilities;
        let mut parts = vec![Part::Bar {
            shares: vec![
                (Color::Blue, p.blue),
                (Color::White, p.white),
                (Color::Red, p.red),
            ],
            thin: false,
        }];
        if let Some(r) = d.reliability {
            parts.push(Part::Hint(w.t(
                k::TEMPO_RELIABILITY_DAY,
                &[
                    ("percent", w.percent(r.accuracy).into()),
                    ("n", Arg::Count(f64::from(d.horizon))),
                    ("winter", w.percent(r.winter_accuracy).into()),
                ],
            )));
        }
        parts
    };
    Tile::new(
        w.day_label(d.date, today),
        day_words(w, Some(d.color), !d.official, Some(d.confidence)),
        Lead::Swatch(Some(d.color)),
    )
    .fact(Some(w.short_day(d.date)))
    .parts(parts)
}

/// The season's days of colour `c` (the page's stock): used of the quota, as a bar, and left.
pub fn stock_tile(w: &Words, s: &SeasonStock, c: Color) -> Tile {
    let n = match c {
        Color::Blue => s.blue,
        Color::White => s.white,
        Color::Red => s.red,
    };
    let of = |key, count: u32| {
        w.t(
            key,
            &[
                ("count", Arg::Count(f64::from(count))),
                ("total", Arg::Count(f64::from(n.total))),
            ],
        )
    };
    let share = if n.total > 0 {
        f64::from(n.used) / f64::from(n.total)
    } else {
        0.0
    };
    Tile::new(
        name(w, c),
        of(k::TEMPO_LEFT, n.remaining),
        Lead::Swatch(Some(c)),
    )
    .fact(Some(of(k::TEMPO_DAYS_USED, n.used)))
    .parts(vec![Part::Bar {
        shares: vec![(c, share)],
        thin: true,
    }])
}

/// TempoPrices: the price in force until when, the other period's, tomorrow's once published;
/// then each colour's two rates.
pub fn price_tiles(w: &Words, data: &Today, now: &DateTime<Tz>) -> Vec<Tile> {
    let mut tiles = Vec::new();
    if let (Some(c), Some(t)) = (price_now(now, data), &data.tariffs) {
        let rates = t.of(c.color);
        let (other, other_price) = if c.period == Period::Peak {
            (Period::OffPeak, rates.off_peak)
        } else {
            (Period::Peak, rates.peak)
        };
        let mut parts = vec![Part::Hint(format!(
            "{} · {}",
            period_color(w, other, c.color),
            price(w, other_price)
        ))];
        if let Some(color) = data.tomorrow.color {
            let r = t.of(color);
            parts.push(Part::Hint(w.t(
                k::TEMPO_TOMORROW_PRICES,
                &[
                    ("color", w.lower(&name(w, color)).into()),
                    ("hc", price(w, r.off_peak).into()),
                    ("hp", price(w, r.peak).into()),
                ],
            )));
        }
        tiles.push(
            Tile::new(
                format!(
                    "{} · {}",
                    period_color(w, c.period, c.color),
                    price(w, c.price)
                ),
                w.t(k::TEMPO_UNTIL, &[("time", w.hhmm(&c.until).into())]),
                Lead::Swatch(Some(c.color)),
            )
            .fact(Some(w.s(k::TEMPO_NOW)))
            .parts(parts),
        );
    }
    if let Some(t) = &data.tariffs {
        for c in COLORS {
            let r = t.of(c);
            tiles.push(Tile::new(
                name(w, c),
                format!(
                    "{} {} · {} {}",
                    w.s(k::TEMPO_HC),
                    price(w, r.off_peak),
                    w.s(k::TEMPO_HP),
                    price(w, r.peak)
                ),
                Lead::Swatch(Some(c)),
            ));
        }
    }
    tiles
}

/// The Tempo page: the week (RTE's days, then Maison's forecast), the prices, the season.
pub fn page(cx: &mut Cx) -> Screen {
    let today = cx.live.want(&cx.sources.tempo);
    let forecast = cx.live.want(&cx.sources.forecast);
    let now = cx.house_now();
    let w = cx.words;
    let day = now.date_naive();
    let week = Group::new(
        w.s(k::TEMPO_PREDICTION_WEEK_FORECAST),
        loaded(w, &forecast, &[TEMPO], ("", ""), |f| {
            (!f.days.is_empty())
                .then(|| Body::Tiles(f.days.iter().map(|d| day_tile(w, d, day)).collect()))
        }),
    )
    .stale(stale(cx, &forecast, &[TEMPO]));
    let prices = Group::new(
        w.s(k::TEMPO_PRICES_TITLE),
        loaded(w, &today, &[TEMPO], ("", ""), |t| {
            let tiles = price_tiles(w, t, &now);
            (!tiles.is_empty()).then_some(Body::Tiles(tiles))
        }),
    )
    .stale(stale(cx, &today, &[TEMPO]));
    let season = Group::new(
        match today.data.as_deref() {
            Some(t) => w.t(
                k::TEMPO_SEASON,
                &[("season", t.stock.season.as_str().into())],
            ),
            None => w.s(k::NAV_TEMPO),
        },
        loaded(w, &today, &[TEMPO], ("", ""), |t| {
            Some(Body::Tiles(
                COLORS.iter().map(|c| stock_tile(w, &t.stock, *c)).collect(),
            ))
        }),
    );
    Screen::Page {
        title: w.s(k::NAV_TEMPO),
        back: Box::new(Button::new(
            Some(icon::ARROW_LEFT),
            w.s(k::BACK_HOME),
            Act::Open(View::House),
        )),
        groups: vec![week, prices, season],
    }
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

    #[test]
    fn the_tile_leads_to_the_tempo_page() {
        let w = Words::new(Lang::Fr);
        let t = tile(&w, &today("BLUE", "BLUE", ""), None, &at(14, 0));
        assert!(matches!(t.open, Some(Act::Open(View::Tempo))));
    }

    #[test]
    fn the_week_says_rtes_days_then_maisons_forecast() {
        let w = Words::new(Lang::Fr);
        let f: Forecast = serde_json::from_str::<Answer<Forecast>>(
            r#"{"success":true,"issued":"2026-11-12","weatherIssued":null,"stale":false,"model":null,"days":[{"date":"2026-11-13","horizon":1,"official":true,"color":"BLUE","probabilities":{"BLUE":1,"WHITE":0,"RED":0},"confidence":1},{"date":"2026-11-14","horizon":2,"official":false,"color":"WHITE","probabilities":{"BLUE":0.3,"WHITE":0.62,"RED":0.08},"confidence":0.62,"reliability":{"accuracy":0.81,"winterAccuracy":0.7}}],"stock":{"season":"2026-2027","blue":{"used":40,"total":300,"remaining":260},"white":{"used":3,"total":43,"remaining":40},"red":{"used":0,"total":22,"remaining":22}}}"#,
        )
        .unwrap()
        .body;
        let day = at(14, 0).date_naive();
        let rte = day_tile(&w, &f.days[0], day);
        assert_eq!(rte.name, "Demain");
        assert_eq!(rte.state, "Bleu");
        assert_eq!(rte.fact.as_deref(), Some("ven. 13 nov."));
        assert!(matches!(&rte.parts[..], [Part::Hint(h)] if h == "Publié par RTE"));
        let ours = day_tile(&w, &f.days[1], day);
        assert_eq!(ours.name, "samedi");
        assert_eq!(ours.state, "Blanc probable · 62\u{202f}%");
        let [
            Part::Bar {
                shares,
                thin: false,
            },
            Part::Hint(r),
        ] = &ours.parts[..]
        else {
            panic!("{:?}", ours.parts)
        };
        assert_eq!(shares.len(), 3);
        assert_eq!(shares[1], (Color::White, 0.62));
        assert!(r.starts_with("Juste 81\u{202f}% des jours à J+2"), "{r}");
    }

    #[test]
    fn the_season_says_each_colours_days() {
        let w = Words::new(Lang::Fr);
        let d = today("BLUE", "BLUE", "");
        let white = stock_tile(&w, &d.stock, Color::White);
        assert_eq!(white.name, "Blanc");
        assert_eq!(white.fact.as_deref(), Some("3 utilisés sur 43"));
        assert_eq!(white.state, "40 restants sur 43");
        let [Part::Bar { shares, thin: true }] = &white.parts[..] else {
            panic!()
        };
        assert!((shares[0].1 - 3.0 / 43.0).abs() < 1e-9);
    }

    #[test]
    fn the_prices_say_the_one_in_force_then_every_rate() {
        let w = Words::new(Lang::Fr);
        let d = today("WHITE", "RED", "BLUE");
        let tiles = price_tiles(&w, &d, &at(14, 0));
        assert_eq!(tiles.len(), 4, "now, then the three colours");
        assert_eq!(tiles[0].name, "HP rouge · 0,7062\u{a0}€/kWh");
        assert_eq!(tiles[0].state, "jusqu’à 22:00");
        let [Part::Hint(other), Part::Hint(tomorrow)] = &tiles[0].parts[..] else {
            panic!("{:?}", tiles[0].parts)
        };
        assert_eq!(other, "HC rouge · 0,1575\u{a0}€/kWh");
        assert!(tomorrow.contains("bleu"), "{tomorrow}");
        assert_eq!(
            tiles[1].state,
            "HC 0,1325\u{a0}€/kWh · HP 0,1612\u{a0}€/kWh"
        );
        let mut none = d.clone();
        none.tariffs = None;
        assert!(price_tiles(&w, &none, &at(14, 0)).is_empty());
    }
}
