//! The forecast beyond what RTE has published: RTE's own rule fed with an estimate of the
//! net consumption (consumption − wind − solar), as `fit_tempo` measured it against past
//! seasons (`cache/tempo/model.json`).
//!
//! - Consumption, wind and solar are each a linear model of the weather and the calendar,
//!   as a ratio of their level over the past year (the grid changes: −8 % since 2022).
//! - The net consumption is normalised as RTE does: quantiles 40 % and 80 % of the past
//!   year's, corrected by its 30 % temperature quantile.
//! - Its error grows with the horizon: a seeded Monte Carlo (correlated from day to day)
//!   runs RTE's rule, stocks and calendar included, and counts the colours.
//!
//! Pure and allocation-free in the loop: a refresh is far under 100 ms on the Pi 1.

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

use super::rules::{is_holiday, Color, DayRules, Stock};

/// RTE's temperature correction of the normalisation (γ, k).
const GAMMA: f64 = -0.1176;
const K: f64 = 8.3042;
/// Fewer past days than this and the normalisation is a guess: no forecast.
pub const MIN_LEVEL_DAYS: usize = 200;

/// A weather point: a city (weighted by its population) or a wind-farm area.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Place {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    #[serde(default = "one")]
    pub weight: f64,
}

fn one() -> f64 {
    1.0
}

/// The fitted model and how it scored (`cache/tempo/model.json`, written by `fit_tempo`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    /// When it was fitted (`2026-10-03`).
    pub version: String,
    /// The last day of data it learnt from.
    pub fitted_through: NaiveDate,
    pub cities: Vec<Place>,
    pub wind_sites: Vec<Place>,
    /// Coefficients of [`consumption_features`], [`wind_features`], [`solar_features`].
    pub consumption: Vec<f64>,
    pub wind: Vec<f64>,
    pub solar: Vec<f64>,
    /// The error of the normalised net consumption, by horizon (J+1 first), and its
    /// correlation from one day to the next.
    pub sigma: Vec<f64>,
    pub rho: f64,
    pub runs: u32,
    pub backtest: Backtest,
}

/// The model replayed on past seasons with the weather forecasts of the time.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Backtest {
    pub seasons: Vec<String>,
    pub horizons: Vec<HorizonScore>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct HorizonScore {
    pub horizon: u32,
    pub days: u32,
    /// Share of days right, every day / November to March only.
    pub accuracy: f64,
    pub winter_accuracy: f64,
    pub red_f1: f64,
    pub white_f1: f64,
    /// Brier score of the probabilities (0 is perfect).
    pub brier: f64,
    /// « Always blue » on the same days, for scale.
    pub always_blue: f64,
}

impl Model {
    /// The error at `horizon` days; beyond the measured ones it keeps growing.
    pub fn sigma(&self, horizon: u32) -> f64 {
        let measured = self.sigma.len() as u32;
        match (horizon, self.sigma.last()) {
            (_, None) => 0.5,
            (0, Some(_)) => self.sigma[0],
            (h, Some(last)) if h > measured => last * f64::from(h) / f64::from(measured),
            (h, Some(_)) => self.sigma[h as usize - 1],
        }
    }

    pub fn score(&self, horizon: u32) -> Option<HorizonScore> {
        self.backtest.horizons.iter().find(|s| s.horizon == horizon).copied()
    }
}

pub const CONSUMPTION_FEATURES: usize = 18;
pub const WIND_FEATURES: usize = 3;
pub const SOLAR_FEATURES: usize = 5;

/// The year as two harmonics: [cos, sin, cos 2, sin 2].
fn harmonics(date: NaiveDate) -> [f64; 4] {
    let w = 2.0 * std::f64::consts::PI * f64::from(date.ordinal()) / 365.25;
    [w.cos(), w.sin(), (2.0 * w).cos(), (2.0 * w).sin()]
}

/// What drives the day's consumption: the weekday, holidays, the Christmas week and
/// August, heating and cooling degrees (from a smoothed temperature: buildings lag).
pub fn consumption_features(date: NaiveDate, temperature: f64) -> [f64; CONSUMPTION_FEATURES] {
    let mut f = [0.0; CONSUMPTION_FEATURES];
    f[date.weekday().num_days_from_monday() as usize] = 1.0;
    let flag = |b: bool| if b { 1.0 } else { 0.0 };
    f[7] = flag(is_holiday(date));
    f[8] = flag((date.month() == 12 && date.day() >= 24) || (date.month() == 1 && date.day() == 1));
    f[9] = flag(date.month() == 8);
    let heating = (16.0 - temperature).max(0.0);
    f[10] = heating;
    f[11] = heating * heating / 10.0;
    f[12] = (temperature - 20.0).max(0.0);
    let [c1, s1, c2, s2] = harmonics(date);
    f[13..17].copy_from_slice(&[c1, s1, c2, s2]);
    f[17] = heating * c1;
    f
}

/// Wind production from the sites' mean power (0 to 1, see [`wind_power`]).
pub fn wind_features(power: f64) -> [f64; WIND_FEATURES] {
    [1.0, power, power * power]
}

/// Solar production: its seasonal curve (a day's sun is not forecast well enough to help).
pub fn solar_features(date: NaiveDate) -> [f64; SOLAR_FEATURES] {
    let [c1, s1, c2, s2] = harmonics(date);
    [1.0, c1, s1, c2, s2]
}

/// The day's temperature as buildings feel it: half today, the rest the two days before.
pub fn effective_temperature(today: f64, yesterday: f64, before: f64) -> f64 {
    0.5 * today + 0.3 * yesterday + 0.2 * before
}

/// A turbine's share of its rated power at 100 m, for a wind speed in km/h: nothing below
/// 3 m/s, full from 12 m/s, cubic between.
pub fn wind_power(speed_kmh: f64) -> f64 {
    let v = speed_kmh / 3.6;
    ((v.powi(3) - 27.0) / (1728.0 - 27.0)).clamp(0.0, 1.0)
}

pub fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// One past day: consumption, wind and solar (MW, a Tempo day's mean) and the temperature.
#[derive(Debug, Clone, Copy)]
pub struct PastDay {
    pub consumption: f64,
    pub wind: f64,
    pub solar: f64,
    pub temperature: Option<f64>,
}

/// The past year's levels: what the ratios are of, and RTE's normalisation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Levels {
    pub consumption: f64,
    pub wind: f64,
    pub solar: f64,
    q40: f64,
    scale: f64,
}

/// The `p` quantile of sorted values, interpolated.
pub fn quantile(sorted: &[f64], p: f64) -> f64 {
    let at = p * (sorted.len() - 1) as f64;
    let (low, high) = (at.floor() as usize, at.ceil() as usize);
    sorted[low] + (sorted[high] - sorted[low]) * (at - low as f64)
}

fn sorted(mut values: Vec<f64>) -> Vec<f64> {
    values.sort_by(f64::total_cmp);
    values
}

impl Levels {
    /// From the past year's days; `None` with fewer than [`MIN_LEVEL_DAYS`].
    pub fn from_days(days: &[PastDay]) -> Option<Self> {
        if days.len() < MIN_LEVEL_DAYS {
            return None;
        }
        let net = sorted(days.iter().map(|d| d.consumption - d.wind - d.solar).collect());
        let wind = sorted(days.iter().map(|d| d.wind).collect());
        let solar = sorted(days.iter().map(|d| d.solar).collect());
        let temperatures = sorted(days.iter().filter_map(|d| d.temperature).collect());
        // without a year of temperatures, RTE's reference one (no correction)
        let t30 = if temperatures.len() >= MIN_LEVEL_DAYS { quantile(&temperatures, 0.3) } else { K };
        let (q40, q80) = (quantile(&net, 0.4), quantile(&net, 0.8));
        Some(Self {
            consumption: days.iter().map(|d| d.consumption).sum::<f64>() / days.len() as f64,
            wind: quantile(&wind, 0.98).max(1.0),
            solar: quantile(&solar, 0.98).max(1.0),
            q40,
            scale: (q80 - q40) * (-GAMMA * (t30 - K)).exp(),
        })
    }

    /// RTE's normalised net consumption.
    pub fn normalise(&self, net: f64) -> f64 {
        (net - self.q40) / self.scale
    }
}

/// A day ahead as the weather sees it.
#[derive(Debug, Clone, Copy)]
pub struct DayWeather {
    pub date: NaiveDate,
    /// [`effective_temperature`] over the cities.
    pub temperature: f64,
    /// [`wind_power`] over the sites.
    pub wind_power: f64,
}

/// The expected normalised net consumption of a day.
pub fn expected(model: &Model, levels: &Levels, day: &DayWeather) -> f64 {
    let consumption = dot(&consumption_features(day.date, day.temperature), &model.consumption) * levels.consumption;
    let wind = dot(&wind_features(day.wind_power), &model.wind) * levels.wind;
    let solar = dot(&solar_features(day.date), &model.solar) * levels.solar;
    levels.normalise(consumption - wind - solar)
}

/// A day to simulate: its rules, the expected `x` and its error.
#[derive(Debug, Clone, Copy)]
pub struct Ahead {
    pub rules: DayRules,
    pub x: f64,
    pub sigma: f64,
}

/// The colours' probabilities `[blue, white, red]` of consecutive days after `stock`: `runs`
/// Monte Carlo draws of an AR(1) error on `x` (correlation `rho`), RTE's rule each day.
/// The same seed gives the same answer.
pub fn simulate(days: &[Ahead], stock: Stock, rho: f64, runs: u32, seed: u64) -> Vec<[f64; 3]> {
    let mut counts = vec![[0u32; 3]; days.len()];
    let mut rng = Rng(seed);
    let fresh = (1.0 - rho * rho).sqrt();
    for _ in 0..runs {
        let mut s = stock;
        let mut z = 0.0;
        for (i, day) in days.iter().enumerate() {
            let n = rng.normal();
            z = if i == 0 { n } else { rho * z + fresh * n };
            let color = day.rules.decide(day.x + day.sigma * z, &s);
            counts[i][color.index()] += 1;
            s.take(color);
        }
    }
    let runs = f64::from(runs.max(1));
    counts.iter().map(|c| c.map(|n| f64::from(n) / runs)).collect()
}

/// The most likely colour (the dearer one on a tie: better warned than surprised).
pub fn likeliest(p: &[f64; 3]) -> Color {
    let mut best = Color::Blue;
    for color in Color::ALL {
        if p[color.index()] >= p[best.index()] {
            best = color;
        }
    }
    best
}

/// SplitMix64 and Box–Muller: deterministic, tiny, enough for a Monte Carlo.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in (0, 1].
    fn uniform(&mut self) -> f64 {
        ((self.next() >> 11) as f64 + 1.0) / (1u64 << 53) as f64
    }

    fn normal(&mut self) -> f64 {
        let (u, v) = (self.uniform(), self.uniform());
        (-2.0 * u.ln()).sqrt() * (2.0 * std::f64::consts::PI * v).cos()
    }
}

/// A seed per day: the forecast stays the same all day for the same inputs.
pub fn seed(date: NaiveDate) -> u64 {
    date.num_days_from_ce() as u64
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::tempo::rules::tests::day;

    pub(crate) fn model() -> Model {
        let mut consumption = vec![0.0; CONSUMPTION_FEATURES];
        consumption[..7].copy_from_slice(&[1.0, 1.0, 1.0, 1.0, 1.0, 0.9, 0.85]);
        consumption[10] = 0.03;
        Model {
            version: "test".into(),
            fitted_through: day(2026, 9, 30),
            cities: vec![],
            wind_sites: vec![],
            consumption,
            wind: vec![0.0, 0.8, 0.0],
            solar: vec![0.2, 0.0, 0.0, 0.0, 0.0],
            sigma: vec![0.15, 0.2, 0.25],
            rho: 0.7,
            runs: 400,
            backtest: Backtest::default(),
        }
    }

    fn past_year() -> Vec<PastDay> {
        (0..365)
            .map(|i| {
                let i = f64::from(i);
                PastDay {
                    consumption: 45_000.0 + 10_000.0 * (i / 58.0).sin(),
                    wind: 4_000.0 + 2_000.0 * (i / 7.0).cos(),
                    solar: 2_000.0,
                    temperature: Some(12.0 + 8.0 * (i / 58.0).cos()),
                }
            })
            .collect()
    }

    #[test]
    fn the_normalisation_is_rte_s() {
        let levels = Levels::from_days(&past_year()).unwrap();
        // a net consumption at the 40 % quantile is 0, at the 80 % one is e^(−γ(t30 − k))⁻¹
        assert!(levels.normalise(levels.q40).abs() < 1e-12);
        let mut net: Vec<f64> = past_year().iter().map(|d| d.consumption - d.wind - d.solar).collect();
        net.sort_by(f64::total_cmp);
        let mut temps: Vec<f64> = past_year().iter().filter_map(|d| d.temperature).collect();
        temps.sort_by(f64::total_cmp);
        let expected = (-GAMMA * (quantile(&temps, 0.3) - K)).exp().recip();
        assert!((levels.normalise(quantile(&net, 0.8)) - expected).abs() < 1e-9);
        assert!(Levels::from_days(&past_year()[..MIN_LEVEL_DAYS - 1]).is_none());
        assert_eq!(quantile(&[1.0, 2.0, 3.0, 4.0, 5.0], 0.4), 2.6);
    }

    #[test]
    fn wind_power_follows_the_curve() {
        assert_eq!(wind_power(5.0), 0.0);
        assert_eq!(wind_power(60.0), 1.0);
        assert!((wind_power(28.8) - (512.0 - 27.0) / 1701.0).abs() < 1e-12);
    }

    fn week(x: f64, sigma: f64) -> Vec<Ahead> {
        day(2026, 1, 12)
            .iter_days()
            .take(7)
            .map(|date| Ahead { rules: DayRules::new(date), x, sigma })
            .collect()
    }

    #[test]
    fn the_same_seed_gives_the_same_forecast() {
        let stock = Stock { red: 15, white: 25, red_run: 0 };
        let a = simulate(&week(1.3, 0.3), stock, 0.7, 400, 42);
        assert_eq!(a, simulate(&week(1.3, 0.3), stock, 0.7, 400, 42));
        assert_ne!(a, simulate(&week(1.3, 0.3), stock, 0.7, 400, 43));
        for p in &a {
            assert!((p.iter().sum::<f64>() - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn certainty_without_noise_and_the_rules_hold_in_every_run() {
        let stock = Stock { red: 15, white: 25, red_run: 0 };
        // very cold, no error: red on weekdays, white on Saturday, blue on Sunday
        let p = simulate(&week(5.0, 0.0), stock, 0.7, 50, 1);
        let colors: Vec<Color> = p.iter().map(likeliest).collect();
        use Color::*;
        assert_eq!(colors, [Red, Red, Red, Red, Red, White, Blue]);
        assert!(p.iter().all(|p| p.contains(&1.0)));
        // two reds left: never more than two in the week, whatever the draw
        let p = simulate(&week(1.5, 1.0), Stock { red: 2, ..stock }, 0.7, 400, 7);
        assert!(p.iter().map(|p| p[Red.index()]).sum::<f64>() <= 2.0 + 1e-9);
        assert!(p[6][White.index()] == 0.0 && p[6][Red.index()] == 0.0, "Sunday");
    }

    #[test]
    fn the_error_grows_past_the_measured_horizons() {
        let m = model();
        assert_eq!(m.sigma(1), 0.15);
        assert_eq!(m.sigma(3), 0.25);
        assert!((m.sigma(6) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn a_cold_day_is_expected_higher_than_a_mild_one() {
        let (m, levels) = (model(), Levels::from_days(&past_year()).unwrap());
        let at = |temperature, wind_power| expected(&m, &levels, &DayWeather { date: day(2026, 1, 13), temperature, wind_power });
        assert!(at(-2.0, 0.2) > at(10.0, 0.2));
        assert!(at(5.0, 0.1) > at(5.0, 0.9), "wind lowers the net consumption");
    }
}
