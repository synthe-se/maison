//! EDF Tempo as `/api/tempo` and `/api/tempo/forecast` say it: the colours of yesterday (in
//! force until 06:00), today and tomorrow, the prices and their hours, the season's days left,
//! and the forecast to J+7.

use std::borrow::Cow;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Day {
    pub date: NaiveDate,
    /// `None` until RTE publishes it.
    pub color: Option<Color>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Count {
    pub used: u32,
    pub total: u32,
    pub remaining: u32,
}

/// A season's days per colour: used (published, tomorrow included), quota, left.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonStock {
    pub season: String,
    pub blue: Count,
    pub white: Count,
    pub red: Count,
}

/// The hour (house time) a Tempo day starts, said once: a Tempo day runs 06:00 to 06:00, and
/// before 06:00 the colour in force is the day before's. The peak hours start with it.
pub const DAY_START_HOUR: u32 = 6;

/// Peak hours (« heures pleines », « HH:MM »), local time: the rest is off-peak. They start
/// with the Tempo day ([`DAY_START_HOUR`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hours {
    pub peak_start: Cow<'static, str>,
    pub peak_end: Cow<'static, str>,
}

/// The regulated Tempo's peak hours: 06:00 to 22:00.
pub const HOURS: Hours = Hours {
    peak_start: Cow::Borrowed("06:00"),
    peak_end: Cow::Borrowed("22:00"),
};

/// Peak or off-peak.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    Peak,
    OffPeak,
}

/// The price in force at a moment ([`Today::price_at`]).
#[derive(Debug, Clone, PartialEq)]
pub struct PriceNow {
    pub color: Color,
    pub period: Period,
    /// €/kWh.
    pub price: f64,
    /// When the period ends (« 22:00 »).
    pub until: String,
    /// False before the peak hours start: the colour in force is still yesterday's, and
    /// today's (`next`) starts at `until`.
    pub day_started: bool,
    pub next: Option<Color>,
}

/// « HH:MM » as minutes after midnight (0 when it does not read).
fn minutes(hhmm: &str) -> u32 {
    chrono::NaiveTime::parse_from_str(hhmm, "%H:%M").map_or(0, |t| {
        chrono::Timelike::hour(&t) * 60 + chrono::Timelike::minute(&t)
    })
}

/// One colour's prices (€/kWh, tax included): off-peak (HC) and peak (HP).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Price {
    pub off_peak: f64,
    pub peak: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tariffs {
    pub blue: Price,
    pub white: Price,
    pub red: Price,
    /// The yearly subscription, tax included (€).
    pub subscription: Option<f64>,
    /// When these prices came into force.
    pub starts_on: NaiveDate,
}

impl Tariffs {
    /// The prices of a colour.
    pub fn of(&self, color: Color) -> Price {
        match color {
            Color::Blue => self.blue,
            Color::White => self.white,
            Color::Red => self.red,
        }
    }
}

/// Yesterday (in force until 06:00), today, tomorrow (no colour until RTE publishes it), the
/// prices and their hours, and the days left (`GET /api/tempo`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Today {
    pub yesterday: Day,
    pub today: Day,
    pub tomorrow: Day,
    pub tariffs: Option<Tariffs>,
    pub hours: Hours,
    pub stock: SeasonStock,
    /// When the colours were last fetched (`None`: only from the file so far).
    pub last_updated: Option<DateTime<Utc>>,
    /// The last ask failed: what is shown is older.
    pub cached: bool,
}

impl Today {
    /// What a kWh costs at `now` (read at the house): the colour in force (yesterday's
    /// until the peak hours start), peak or off-peak, until when. `None` without prices or
    /// without the colour in force.
    pub fn price_at(&self, now: DateTime<Utc>) -> Option<PriceNow> {
        let local = now.with_timezone(&crate::HOUSE_TZ);
        let t = chrono::Timelike::hour(&local) * 60 + chrono::Timelike::minute(&local);
        let (start, end) = (
            minutes(&self.hours.peak_start),
            minutes(&self.hours.peak_end),
        );
        let day_started = t >= start;
        let period = if day_started && t < end {
            Period::Peak
        } else {
            Period::OffPeak
        };
        let color = if day_started {
            self.today.color
        } else {
            self.yesterday.color
        }?;
        let prices = self.tariffs.as_ref()?.of(color);
        let (price, until) = match period {
            Period::Peak => (prices.peak, &self.hours.peak_end),
            Period::OffPeak => (prices.off_peak, &self.hours.peak_start),
        };
        Some(PriceNow {
            color,
            period,
            price,
            until: until.to_string(),
            day_started,
            next: if day_started { None } else { self.today.color },
        })
    }
}

/// A forecast's chances per color, keyed as the colors are said (`BLUE`, `WHITE`, `RED`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub struct Probabilities {
    pub blue: f64,
    pub white: f64,
    pub red: f64,
}

impl From<[f64; 3]> for Probabilities {
    fn from([blue, white, red]: [f64; 3]) -> Self {
        Self { blue, white, red }
    }
}

/// How the model did at this horizon on past seasons (`model.json`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reliability {
    pub accuracy: f64,
    pub winter_accuracy: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForecastDay {
    pub date: NaiveDate,
    /// Days after today.
    pub horizon: u32,
    /// RTE's colour, not a forecast.
    pub official: bool,
    pub color: Color,
    pub probabilities: Probabilities,
    /// The colour's probability.
    pub confidence: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reliability: Option<Reliability>,
}

/// The model replayed on past seasons with the weather forecasts of the time. Also in
/// `model.json` (persisted: the snake_case aliases read a file from before).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Backtest {
    pub seasons: Vec<String>,
    pub horizons: Vec<HorizonScore>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HorizonScore {
    pub horizon: u32,
    pub days: u32,
    /// Share of days right, every day / November to March only.
    pub accuracy: f64,
    #[serde(alias = "winter_accuracy")]
    pub winter_accuracy: f64,
    #[serde(alias = "red_f1")]
    pub red_f1: f64,
    #[serde(alias = "white_f1")]
    pub white_f1: f64,
    /// Brier score of the probabilities (0 is perfect).
    pub brier: f64,
    /// « Always blue » on the same days, for scale.
    #[serde(alias = "always_blue")]
    pub always_blue: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub version: String,
    pub fitted_through: NaiveDate,
    /// How it did on past seasons, by horizon (what « reliability » is said from).
    pub backtest: Backtest,
}

/// The days after today up to J+7: RTE's when published, the forecast's after
/// (`GET /api/tempo/forecast`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Forecast {
    pub issued: NaiveDate,
    /// The day of the weather forecast used (older than today: Open-Meteo is down).
    pub weather_issued: Option<NaiveDate>,
    pub stale: bool,
    pub model: Option<ModelInfo>,
    pub days: Vec<ForecastDay>,
    pub stock: SeasonStock,
    /// Why the forecast stops short of J+7.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Answer;

    #[test]
    fn today_round_trips() {
        let json = r#"{"success":true,"yesterday":{"date":"2026-10-15","color":"BLUE"},"today":{"date":"2026-10-16","color":"WHITE"},"tomorrow":{"date":"2026-10-17","color":null},"tariffs":{"blue":{"offPeak":0.1325,"peak":0.1612},"white":{"offPeak":0.1499,"peak":0.1871},"red":{"offPeak":0.1575,"peak":0.7062},"subscription":null,"startsOn":"2026-08-01"},"hours":{"peakStart":"06:00","peakEnd":"22:00"},"stock":{"season":"2026-2027","blue":{"used":40,"total":300,"remaining":260},"white":{"used":2,"total":43,"remaining":41},"red":{"used":0,"total":22,"remaining":22}},"lastUpdated":"2026-10-16T08:00:00Z","cached":false}"#;
        let t: Answer<Today> = serde_json::from_str(json).unwrap();
        assert_eq!(t.body.today.color, Some(Color::White));
        assert_eq!(t.body.tariffs.as_ref().unwrap().of(Color::Red).peak, 0.7062);
        assert_eq!(t.body.hours.peak_end, "22:00");
        assert_eq!(serde_json::to_string(&t).unwrap(), json);
    }

    #[test]
    fn a_forecast_round_trips() {
        let json = r#"{"success":true,"issued":"2026-10-16","weatherIssued":"2026-10-16","stale":false,"model":{"version":"2026-10-03","fittedThrough":"2026-08-31","backtest":{"seasons":["2025-2026"],"horizons":[{"horizon":1,"days":200,"accuracy":0.9,"winterAccuracy":0.8,"redF1":0.7,"whiteF1":0.6,"brier":0.1,"alwaysBlue":0.5}]}},"days":[{"date":"2026-10-17","horizon":1,"official":false,"color":"BLUE","probabilities":{"BLUE":0.9,"WHITE":0.08,"RED":0.02},"confidence":0.9,"reliability":{"accuracy":0.9,"winterAccuracy":0.8}}],"stock":{"season":"2026-2027","blue":{"used":40,"total":300,"remaining":260},"white":{"used":2,"total":43,"remaining":41},"red":{"used":0,"total":22,"remaining":22}}}"#;
        let f: Answer<Forecast> = serde_json::from_str(json).unwrap();
        assert_eq!(f.body.days[0].probabilities.blue, 0.9);
        assert_eq!(serde_json::to_string(&f).unwrap(), json);
        // model.json written before the camelCase move
        let old: HorizonScore = serde_json::from_str(
            r#"{"horizon":1,"days":1,"accuracy":1,"winter_accuracy":1,"red_f1":1,"white_f1":1,"brier":0,"always_blue":1}"#,
        )
        .unwrap();
        assert_eq!(old.red_f1, 1.0);
    }

    fn today() -> Today {
        let json = r#"{"yesterday":{"date":"2026-10-15","color":"RED"},"today":{"date":"2026-10-16","color":"WHITE"},"tomorrow":{"date":"2026-10-17","color":null},"tariffs":{"blue":{"offPeak":0.13,"peak":0.16},"white":{"offPeak":0.15,"peak":0.19},"red":{"offPeak":0.16,"peak":0.73},"subscription":null,"startsOn":"2026-08-01"},"hours":{"peakStart":"06:00","peakEnd":"22:00"},"stock":{"season":"2026-2027","blue":{"used":0,"total":300,"remaining":300},"white":{"used":0,"total":43,"remaining":43},"red":{"used":0,"total":22,"remaining":22}},"lastUpdated":null,"cached":false}"#;
        serde_json::from_str(json).unwrap()
    }

    /// Paris is UTC+2 on 16 October 2026.
    fn paris(hhmm: &str) -> DateTime<Utc> {
        format!("2026-10-16T{hhmm}:00+02:00").parse().unwrap()
    }

    #[test]
    fn the_price_in_force_follows_the_tempo_day() {
        let data = today();
        let early = data.price_at(paris("05:59")).unwrap();
        assert_eq!(
            (early.color, early.period, early.price, early.until.as_str()),
            (Color::Red, Period::OffPeak, 0.16, "06:00"),
            "before 06:00: yesterday's, off-peak"
        );
        assert!(!early.day_started && early.next == Some(Color::White));
        let peak = data.price_at(paris("06:00")).unwrap();
        assert_eq!(
            (peak.color, peak.period, peak.price, peak.until.as_str()),
            (Color::White, Period::Peak, 0.19, "22:00")
        );
        assert!(peak.day_started && peak.next.is_none());
        let night = data.price_at(paris("22:00")).unwrap();
        assert_eq!(
            (night.color, night.period, night.price),
            (Color::White, Period::OffPeak, 0.15)
        );
        let unpriced = Today {
            tariffs: None,
            ..data
        };
        assert_eq!(unpriced.price_at(paris("12:00")), None);
        // the day starts with the peak hours, said once
        assert_eq!(minutes(&HOURS.peak_start), DAY_START_HOUR * 60);
    }

    #[test]
    fn colours_read_both_languages() {
        assert_eq!(Color::parse(" bleu "), Some(Color::Blue));
        assert_eq!(Color::parse("RED"), Some(Color::Red));
        assert_eq!(Color::parse("vert"), None);
        assert_eq!(Color::White.as_str(), "WHITE");
        assert_eq!(Color::ALL[Color::Red.index()], Color::Red);
    }
}
