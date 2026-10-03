//! The Tempo rules, as RTE applies them: the CRE deliberation of 30 October 2014 (the
//! calendar) and RTE's « Méthode de choix des jours Tempo » (indice 2 du 7/01/2025, the
//! thresholds). Pure: no I/O, the forecast and the fitter share every line of it.
//!
//! - A season runs from 1 September to 31 August: 22 red days, 43 white, the rest blue.
//! - Red only from 1 November to 31 March, Monday to Friday, never on a public holiday
//!   (RTE never placed one there), at most 5 in a row. White any day but Sunday.
//! - A day whose normalised net consumption crosses the red threshold is red, else the
//!   white one white, else blue. At the end of a period RTE places what is left, threshold
//!   or not (13 reds from 13 to 31 March 2026, the winter was mild).

use std::collections::BTreeMap;

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};

use crate::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Color {
    Blue,
    White,
    Red,
}

impl Color {
    /// Cheapest first, the order of every per-colour array (`[blue, white, red]`).
    pub const ALL: [Color; 3] = [Color::Blue, Color::White, Color::Red];

    /// RTE's names, and api-couleur-tempo's French ones.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_uppercase().as_str() {
            "BLUE" | "BLEU" => Some(Color::Blue),
            "WHITE" | "BLANC" => Some(Color::White),
            "RED" | "ROUGE" => Some(Color::Red),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Color::Blue => "BLUE",
            Color::White => "WHITE",
            Color::Red => "RED",
        }
    }

    /// Its place in `[blue, white, red]` arrays.
    pub fn index(self) -> usize {
        self as usize
    }

    /// Its LED colour, as RGB hex (the rabbit's belly).
    pub fn led_hex(self) -> &'static str {
        match self {
            Color::Blue => "0000ff",
            Color::White => "ffffff",
            Color::Red => "ff0000",
        }
    }

    /// The rabbit's ear position for it (0 to 16: down, level, up).
    pub fn ear_position(self) -> u8 {
        match self {
            Color::Blue => 0,
            Color::White => 8,
            Color::Red => 16,
        }
    }
}

/// A season's colours by day, in date order.
pub type History = BTreeMap<NaiveDate, Color>;

/// The CRE's yearly quotas; EDF's season answer overrides them when it says otherwise.
pub const RED_DAYS: u32 = 22;
pub const WHITE_DAYS: u32 = 43;
pub const MAX_RED_RUN: u32 = 5;
/// The first season RTE's data serves.
pub const FIRST_SEASON: i32 = 2014;

/// RTE's thresholds: `A − B·day − C·stock` (red: red days left; white: white + red left).
const RED_THRESHOLD: (f64, f64, f64) = (3.15, 0.010, 0.031);
const WHITE_THRESHOLD: (f64, f64, f64) = (4.00, 0.015, 0.026);

/// The first year of the season `date` falls in (September starts it).
pub fn season_start_year(date: NaiveDate) -> i32 {
    if date.month() >= 9 {
        date.year()
    } else {
        date.year() - 1
    }
}

/// The first and last days of the season starting in `start_year`.
pub fn season_bounds(start_year: i32) -> (NaiveDate, NaiveDate) {
    (ymd(start_year, 9, 1), ymd(start_year + 1, 8, 31))
}

/// As RTE names it: `2026-2027`.
pub fn season_name(start_year: i32) -> String {
    format!("{start_year}-{}", start_year + 1)
}

/// The season `today` falls in.
pub fn current_season(today: NaiveDate) -> String {
    season_name(season_start_year(today))
}

/// A season as asked (`?season=`, a file name): `YYYY-YYYY`, two following years, from RTE's
/// first season to the current one; its first year. Anything else is refused before it
/// reaches a source, a file name or a day-by-day loop.
pub fn parse_season(season: &str, today: NaiveDate) -> Result<i32, AppError> {
    let invalid = || {
        AppError::bad_request(format!(
            "Invalid season: expected YYYY-YYYY, from {} to {}",
            season_name(FIRST_SEASON),
            current_season(today)
        ))
    };
    let year = |text: &str| {
        (text.len() == 4 && text.bytes().all(|b| b.is_ascii_digit()))
            .then(|| text.parse::<i32>().ok())
            .flatten()
    };
    let (start, end) = season.split_once('-').ok_or_else(invalid)?;
    match (year(start), year(end)) {
        (Some(start), Some(end)) if end == start + 1 && (FIRST_SEASON..=season_start_year(today)).contains(&start) => {
            Ok(start)
        }
        _ => Err(invalid()),
    }
}

/// The season's blue days: what the year leaves after the white and red ones (300, 301 in a
/// leap season).
pub fn blue_days(start_year: i32, red: u32, white: u32) -> u32 {
    let (first, last) = season_bounds(start_year);
    (last - first).num_days() as u32 + 1 - red - white
}

/// RTE's « jour Tempo »: 1 on 1 September.
pub fn tempo_day(date: NaiveDate) -> i32 {
    (date - season_bounds(season_start_year(date)).0).num_days() as i32 + 1
}

fn ymd(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).expect("a real date")
}

/// Easter Sunday (the anonymous Gregorian computus).
fn easter(year: i32) -> NaiveDate {
    let (a, b, c) = (year % 19, year / 100, year % 100);
    let (d, e) = (b / 4, b % 4);
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let (i, k) = (c / 4, c % 4);
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let month = (h + l - 7 * m + 114) / 31;
    let day = (h + l - 7 * m + 114) % 31 + 1;
    ymd(year, month as u32, day as u32)
}

/// A national public holiday in France (Alsace-Moselle's own are not: RTE reds them).
pub fn is_holiday(date: NaiveDate) -> bool {
    let fixed = [(1, 1), (5, 1), (5, 8), (7, 14), (8, 15), (11, 1), (11, 11), (12, 25)];
    if fixed.contains(&(date.month(), date.day())) {
        return true;
    }
    let easter = easter(date.year());
    [1, 39, 50].iter().any(|days| easter + Duration::days(*days) == date)
}

/// November to March: the red days' period, the winter the colours are decided in.
pub fn is_winter(date: NaiveDate) -> bool {
    matches!(date.month(), 11 | 12 | 1 | 2 | 3)
}

/// Whether the calendar lets the day be red.
pub fn red_allowed(date: NaiveDate) -> bool {
    is_winter(date)
        && !matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
        && !is_holiday(date)
}

/// Whether the calendar lets the day be white.
pub fn white_allowed(date: NaiveDate) -> bool {
    date.weekday() != Weekday::Sun
}

/// The season's quotas (EDF's, else the CRE's).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Quotas {
    pub red: u32,
    pub white: u32,
}

impl Default for Quotas {
    fn default() -> Self {
        Self { red: RED_DAYS, white: WHITE_DAYS }
    }
}

/// What is left before a day: red and white days, and the reds just before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stock {
    pub red: u32,
    pub white: u32,
    pub red_run: u32,
}

impl Stock {
    /// The stock before `day`, from the season's colours up to the day before (a missing
    /// day counts as blue).
    pub fn before(history: &History, quotas: Quotas, day: NaiveDate) -> Self {
        let first = season_bounds(season_start_year(day)).0;
        let mut stock = Self { red: quotas.red, white: quotas.white, red_run: 0 };
        for (_, color) in history.range(first..day) {
            stock.take(*color);
        }
        // the run is calendar days: a gap (the weekend) ends it
        stock.red_run = (1..=MAX_RED_RUN as i64)
            .take_while(|back| history.get(&(day - Duration::days(*back))) == Some(&Color::Red))
            .count() as u32;
        stock
    }

    /// The stock after a day of `color`.
    pub fn take(&mut self, color: Color) {
        match color {
            Color::Red => {
                self.red = self.red.saturating_sub(1);
                self.red_run += 1;
            }
            Color::White => {
                self.white = self.white.saturating_sub(1);
                self.red_run = 0;
            }
            Color::Blue => self.red_run = 0,
        }
    }
}

/// What a day's decision needs that does not depend on the stock, worked out once: the
/// forecast decides each day hundreds of times.
#[derive(Debug, Clone, Copy)]
pub struct DayRules {
    pub date: NaiveDate,
    red_ok: bool,
    white_ok: bool,
    day: f64,
    /// Days that may still be red (white) from this one to the end of their period.
    red_left: u32,
    white_left: u32,
}

impl DayRules {
    pub fn new(date: NaiveDate) -> Self {
        let start = season_start_year(date);
        let left = |last: NaiveDate, allowed: fn(NaiveDate) -> bool| {
            date.iter_days().take_while(|d| *d <= last).filter(|d| allowed(*d)).count() as u32
        };
        Self {
            date,
            red_ok: red_allowed(date),
            white_ok: white_allowed(date),
            day: f64::from(tempo_day(date)),
            red_left: if red_allowed(date) { left(ymd(start + 1, 3, 31), red_allowed) } else { 0 },
            white_left: left(season_bounds(start).1, white_allowed),
        }
    }

    pub fn red_threshold(&self, stock: &Stock) -> f64 {
        let (a, b, c) = RED_THRESHOLD;
        a - b * self.day - c * f64::from(stock.red)
    }

    pub fn white_threshold(&self, stock: &Stock) -> f64 {
        let (a, b, c) = WHITE_THRESHOLD;
        a - b * self.day - c * f64::from(stock.red + stock.white)
    }

    /// The colour RTE gives the day for a normalised net consumption `x`.
    pub fn decide(&self, x: f64, stock: &Stock) -> Color {
        let can_red = self.red_ok && stock.red > 0 && stock.red_run < MAX_RED_RUN;
        let can_white = self.white_ok && stock.white > 0;
        // the end of the period: as many days left as days to place
        if can_red && stock.red >= self.red_left {
            return Color::Red;
        }
        if can_white && !self.red_ok && stock.white >= self.white_left {
            return Color::White;
        }
        if can_red && x > self.red_threshold(stock) {
            Color::Red
        } else if can_white && x > self.white_threshold(stock) {
            Color::White
        } else {
            Color::Blue
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        ymd(y, m, d)
    }

    /// A season as RTE gave it (the repository's file).
    pub(crate) fn season(start_year: i32) -> History {
        let path = format!("{}/../cache/tempo/tempo_history_{}.json", env!("CARGO_MANIFEST_DIR"), season_name(start_year));
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let file: serde_json::Value = serde_json::from_str(&text).unwrap();
        file["values"]
            .as_object()
            .unwrap()
            .iter()
            .filter_map(|(k, v)| Some((k.parse().ok()?, Color::parse(v.as_str()?)?)))
            .collect()
    }

    #[test]
    fn the_season_turns_on_the_first_of_september() {
        assert_eq!(current_season(day(2026, 8, 31)), "2025-2026");
        assert_eq!(current_season(day(2026, 9, 1)), "2026-2027");
        assert_eq!(tempo_day(day(2026, 9, 1)), 1);
        assert_eq!(tempo_day(day(2027, 1, 1)), 123);
        assert_eq!(blue_days(2026, 22, 43), 300);
        assert_eq!(blue_days(2027, 22, 43), 301);
    }

    #[test]
    fn only_real_seasons_are_accepted() {
        let today = day(2026, 10, 3);
        assert_eq!(parse_season("2014-2015", today).unwrap(), 2014);
        assert_eq!(parse_season("2026-2027", today).unwrap(), 2026);
        for bad in ["2013-2014", "2027-2028", "2024-2026", "../../x", "20a4-2025", "2024", "+202-2025"] {
            assert!(parse_season(bad, today).is_err(), "{bad}");
        }
    }

    #[test]
    fn holidays_include_the_moving_ones() {
        for holiday in [day(2025, 11, 11), day(2026, 1, 1), day(2026, 4, 6), day(2026, 5, 14), day(2026, 5, 25)] {
            assert!(is_holiday(holiday), "{holiday}");
        }
        assert!(!is_holiday(day(2024, 3, 29)), "Good Friday is Alsace-Moselle's only");
        assert!(!red_allowed(day(2025, 11, 11)));
        assert!(red_allowed(day(2024, 3, 29)));
        assert!(!red_allowed(day(2026, 1, 3)), "a Saturday");
        assert!(!red_allowed(day(2026, 4, 1)), "April");
        assert!(!white_allowed(day(2026, 1, 4)), "a Sunday");
    }

    #[test]
    fn rte_s_seasons_keep_the_calendar_rules() {
        for start in [2018, 2020, 2022, 2024, 2025] {
            let history = season(start);
            let count = |c: Color| history.values().filter(|v| **v == c).count() as u32;
            assert_eq!((count(Color::Red), count(Color::White)), (RED_DAYS, WHITE_DAYS), "{start}");
            for (date, color) in &history {
                match color {
                    Color::Red => assert!(red_allowed(*date), "{date} red"),
                    Color::White => assert!(white_allowed(*date), "{date} white"),
                    Color::Blue => {}
                }
            }
        }
    }

    #[test]
    fn the_stock_follows_the_history() {
        let history = season(2025);
        let stock = Stock::before(&history, Quotas::default(), day(2026, 1, 8));
        // reds on 29 and 31 December, 5, 6 and 7 January: five used, and a run of three
        assert_eq!(stock.red, 17);
        assert_eq!(stock.red_run, 3);
        assert_eq!(Stock::before(&history, Quotas::default(), day(2025, 9, 1)), Stock { red: 22, white: 43, red_run: 0 });
    }

    #[test]
    fn the_thresholds_are_rte_s() {
        let rules = DayRules::new(day(2025, 1, 13));
        let stock = Stock { red: 11, white: 22, red_run: 0 };
        // day 135: 3.15 − 1.35 − 0.341 and 4.00 − 2.025 − 0.858
        assert!((rules.red_threshold(&stock) - 1.459).abs() < 1e-9);
        assert!((rules.white_threshold(&stock) - 1.117).abs() < 1e-9);
        assert_eq!(rules.decide(1.46, &stock), Color::Red);
        assert_eq!(rules.decide(1.45, &stock), Color::White);
        assert_eq!(rules.decide(1.11, &stock), Color::Blue);
        // no red on a Saturday, even very cold; no white on a Sunday
        assert_eq!(DayRules::new(day(2025, 1, 18)).decide(3.0, &stock), Color::White);
        assert_eq!(DayRules::new(day(2025, 1, 19)).decide(3.0, &stock), Color::Blue);
        // a sixth red in a row is white at most
        assert_eq!(rules.decide(3.0, &Stock { red_run: 5, ..stock }), Color::White);
        // nothing left
        assert_eq!(rules.decide(3.0, &Stock { red: 0, white: 0, red_run: 0 }), Color::Blue);
    }

    #[test]
    fn the_end_of_march_2026_is_red_whatever_the_weather() {
        // RTE placed its 13 last reds on the 13 weekdays from 13 March: the stock, not the cold
        let history = season(2025);
        let mut reds = 0;
        for date in day(2026, 3, 13).iter_days().take_while(|d| *d <= day(2026, 3, 31)) {
            let stock = Stock::before(&history, Quotas::default(), date);
            let decided = DayRules::new(date).decide(-10.0, &stock);
            if red_allowed(date) {
                assert_eq!(decided, Color::Red, "{date}");
                assert_eq!(history[&date], Color::Red, "{date}");
                reds += 1;
            } else {
                assert_eq!(decided, Color::Blue, "{date}");
            }
        }
        assert_eq!(reds, 13);
        // the day before, the stock still waits for the cold
        let before = day(2026, 3, 12);
        assert_eq!(DayRules::new(before).decide(-10.0, &Stock::before(&history, Quotas::default(), before)), Color::Blue);
    }
}
