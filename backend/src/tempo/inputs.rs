//! The forecast's inputs, and their parsing (the service and `fit_tempo` share it):
//!
//! - Open-Meteo: hourly temperatures at the model's cities and wind speeds at its wind
//!   sites, one request each (ECMWF, the weather the backtest replayed);
//! - ODRE's éCO2mix (RTE's open data, no key): consumption, wind and solar every quarter of
//!   an hour, averaged over Tempo days (06:00 → 06:00), as RTE does;
//! - `cache/tempo/netload.json`: the past year of both, which RTE's normalisation needs.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
use chrono_tz::Europe::Paris;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::forecast::{effective_temperature, wind_power, DayWeather, Levels, Model, PastDay, Place};
use crate::error::AppError;

/// The weather model the forecast was fitted and replayed with.
pub const WEATHER_MODEL: &str = "ecmwf_ifs025";
/// A day needs this many hours of data at every place (a day with a gap is no day).
const MIN_HOURS: usize = 20;
/// How long `netload.json` keeps days: a year, and a margin.
const KEEP_DAYS: i64 = 400;

/// One place's hourly values, at Paris local times.
#[derive(Debug, Clone, Default)]
pub struct Series {
    pub times: Vec<NaiveDateTime>,
    pub values: Vec<Option<f64>>,
}

/// Open-Meteo's answer (an array for several places, an object for one): each place's
/// `hourly.<key>` series.
pub fn parse_hourly(body: &Value, key: &str) -> Result<Vec<Series>, AppError> {
    let places = match body {
        Value::Array(places) => places.iter().collect(),
        place => vec![place],
    };
    places
        .into_iter()
        .map(|place| {
            let hourly = &place["hourly"];
            let bad = || AppError::service_unavailable(format!("Open-Meteo answered without hourly {key}"));
            let times = hourly["time"].as_array().ok_or_else(bad)?;
            let values = hourly[key].as_array().ok_or_else(bad)?;
            Ok(Series {
                times: times
                    .iter()
                    .map(|t| t.as_str().and_then(|t| NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M").ok()).ok_or_else(bad))
                    .collect::<Result<_, _>>()?,
                values: values.iter().map(Value::as_f64).collect(),
            })
        })
        .collect()
}

/// The Tempo day a local time belongs to: 06:00 to 06:00.
pub fn tempo_day_of(local: NaiveDateTime) -> NaiveDate {
    (local - Duration::hours(6)).date()
}

/// Per day (`day_of` a time), each place's mean of `f(value)`; days missing hours anywhere
/// are left out.
fn daily_means(series: &[Series], day_of: fn(NaiveDateTime) -> NaiveDate, f: fn(f64) -> f64) -> BTreeMap<NaiveDate, Vec<f64>> {
    let mut sums: BTreeMap<NaiveDate, Vec<(f64, usize)>> = BTreeMap::new();
    for (place, s) in series.iter().enumerate() {
        for (time, value) in s.times.iter().zip(&s.values) {
            if let Some(value) = value {
                let day = sums.entry(day_of(*time)).or_insert_with(|| vec![(0.0, 0); series.len()]);
                day[place].0 += f(*value);
                day[place].1 += 1;
            }
        }
    }
    sums.into_iter()
        .filter(|(_, places)| places.iter().all(|(_, n)| *n >= MIN_HOURS))
        .map(|(day, places)| (day, places.iter().map(|(sum, n)| sum / *n as f64).collect()))
        .collect()
}

/// The cities' mean temperature per calendar day, weighted by population.
pub fn daily_temperature(series: &[Series], cities: &[Place]) -> BTreeMap<NaiveDate, f64> {
    let total: f64 = cities.iter().map(|c| c.weight).sum();
    daily_means(series, |t| t.date(), |t| t)
        .into_iter()
        .map(|(day, means)| (day, means.iter().zip(cities).map(|(t, c)| t * c.weight).sum::<f64>() / total))
        .collect()
}

/// The sites' mean wind power (0 to 1) per Tempo day.
pub fn daily_wind_power(series: &[Series]) -> BTreeMap<NaiveDate, f64> {
    daily_means(series, tempo_day_of, wind_power)
        .into_iter()
        .map(|(day, means)| (day, means.iter().sum::<f64>() / means.len() as f64))
        .collect()
}

/// `latitude=…&longitude=…` for places.
pub fn coordinates(places: &[Place]) -> [(&'static str, String); 2] {
    let join = |f: fn(&Place) -> f64| places.iter().map(|p| f(p).to_string()).collect::<Vec<_>>().join(",");
    [("latitude", join(|p| p.latitude)), ("longitude", join(|p| p.longitude))]
}

/// The weather around the forecast: past days' and coming days' temperatures, coming days'
/// wind. Saved, so an Open-Meteo outage still has yesterday's forecast to go on.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Weather {
    /// The day it was forecast (the horizons, hence the errors, count from it).
    pub issued: Option<NaiveDate>,
    pub temperature: BTreeMap<NaiveDate, f64>,
    pub wind_power: BTreeMap<NaiveDate, f64>,
}

impl Weather {
    /// A day's inputs, when the weather covers it and the two days before.
    pub fn day(&self, date: NaiveDate) -> Option<DayWeather> {
        let t = |back: i64| self.temperature.get(&(date - Duration::days(back))).copied();
        Some(DayWeather {
            date,
            temperature: effective_temperature(t(0)?, t(1)?, t(2)?),
            wind_power: *self.wind_power.get(&date)?,
        })
    }
}

/// Today's forecast from Open-Meteo: 3 past days (the temperature lags), 9 ahead (J+7 ends
/// at 06:00 on J+8).
pub async fn fetch_weather(client: &reqwest::Client, url: &str, model: &Model, today: NaiveDate) -> Result<Weather, AppError> {
    let ask = |places: &[Place], key: &'static str| {
        let mut query = coordinates(places).to_vec();
        query.extend([
            ("hourly", key.to_string()),
            ("past_days", "3".into()),
            ("forecast_days", "9".into()),
            ("timezone", "Europe/Paris".into()),
            ("models", WEATHER_MODEL.into()),
        ]);
        async move {
            let response = client.get(format!("{url}/v1/forecast")).query(&query).send().await?;
            if !response.status().is_success() {
                return Err(AppError::service_unavailable(format!("Open-Meteo: {}", response.status())));
            }
            parse_hourly(&response.json::<Value>().await?, key)
        }
    };
    let temperature = daily_temperature(&ask(&model.cities, "temperature_2m").await?, &model.cities);
    let wind_power = daily_wind_power(&ask(&model.wind_sites, "wind_speed_100m").await?);
    if temperature.is_empty() || wind_power.is_empty() {
        return Err(AppError::service_unavailable("Open-Meteo answered without full days"));
    }
    Ok(Weather { issued: Some(today), temperature, wind_power })
}

/// One éCO2mix row (MW over the quarter of an hour; a gap is null).
#[derive(Debug, Deserialize)]
pub struct OdreRow {
    pub date_heure: DateTime<FixedOffset>,
    pub consommation: Option<f64>,
    pub eolien: Option<f64>,
    pub solaire: Option<f64>,
}

/// éCO2mix rows as Tempo days' means `[consumption, wind, solar]`; days with fewer than 22
/// hours of data (today, a gap) are left out.
pub fn tempo_days(rows: &[OdreRow]) -> BTreeMap<NaiveDate, [f64; 3]> {
    // sums, rows, and the hours seen (a bit each)
    let mut days: BTreeMap<NaiveDate, ([f64; 3], usize, u32)> = BTreeMap::new();
    for row in rows {
        let (Some(c), Some(w), Some(s)) = (row.consommation, row.eolien, row.solaire) else { continue };
        let local = row.date_heure.with_timezone(&Paris).naive_local();
        let day = days.entry(tempo_day_of(local)).or_default();
        day.0[0] += c;
        day.0[1] += w;
        day.0[2] += s;
        day.1 += 1;
        day.2 |= 1 << chrono::Timelike::hour(&local);
    }
    days.into_iter()
        .filter(|(_, (_, _, hours))| hours.count_ones() >= 22)
        .map(|(day, (sum, n, _))| (day, sum.map(|v| v / n as f64)))
        .collect()
}

/// 06:00 in Paris on `day`, in UTC (where ODRE's rows start a Tempo day).
pub fn tempo_day_start(day: NaiveDate) -> DateTime<chrono::Utc> {
    let six = day.and_time(NaiveTime::from_hms_opt(6, 0, 0).expect("06:00"));
    Paris
        .from_local_datetime(&six)
        .earliest()
        .expect("06:00 exists in Paris")
        .with_timezone(&chrono::Utc)
}

/// The Tempo days of `dataset` (`eco2mix-national-tr`: the current months) since `since`.
pub async fn fetch_load(
    client: &reqwest::Client,
    url: &str,
    dataset: &str,
    since: NaiveDate,
) -> Result<BTreeMap<NaiveDate, [f64; 3]>, AppError> {
    let start = tempo_day_start(since).format("%Y-%m-%dT%H:%M:%SZ");
    let response = client
        .get(format!("{url}/{dataset}/exports/json"))
        .query(&[
            ("select", "date_heure,consommation,eolien,solaire".to_string()),
            ("where", format!("date_heure >= '{start}'")),
            ("order_by", "date_heure".to_string()),
        ])
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(AppError::service_unavailable(format!("ODRE éCO2mix: {}", response.status())));
    }
    Ok(tempo_days(&response.json::<Vec<OdreRow>>().await?))
}

/// `cache/tempo/netload.json`: the past Tempo days' `[consumption, wind, solar]` and the
/// cities' temperatures, by day.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetLoad {
    pub load: BTreeMap<NaiveDate, [f64; 3]>,
    pub temperature: BTreeMap<NaiveDate, f64>,
}

impl NetLoad {
    /// The year before `day` (the day itself excluded: it is not over).
    pub fn past_year(&self, day: NaiveDate) -> Vec<PastDay> {
        self.load
            .range(day - Duration::days(365)..day)
            .map(|(date, [consumption, wind, solar])| PastDay {
                consumption: *consumption,
                wind: *wind,
                solar: *solar,
                temperature: self.temperature.get(date).copied(),
            })
            .collect()
    }

    pub fn levels(&self, day: NaiveDate) -> Option<Levels> {
        Levels::from_days(&self.past_year(day))
    }

    /// Adds days (finished ones only: before `today`), forgets those over [`KEEP_DAYS`] old.
    pub fn merge(&mut self, load: BTreeMap<NaiveDate, [f64; 3]>, temperature: &BTreeMap<NaiveDate, f64>, today: NaiveDate) {
        self.load.extend(load.into_iter().filter(|(d, _)| *d < today));
        self.temperature.extend(temperature.range(..today).map(|(d, t)| (*d, *t)));
        let oldest = today - Duration::days(KEEP_DAYS);
        self.load.retain(|d, _| *d >= oldest);
        self.temperature.retain(|d, _| *d >= oldest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tempo::rules::tests::day;
    use serde_json::json;

    fn place(weight: f64) -> Place {
        Place { name: "x".into(), latitude: 48.85, longitude: 2.35, weight }
    }

    fn hours(from: &str, values: &[f64]) -> Value {
        let start = NaiveDateTime::parse_from_str(from, "%Y-%m-%dT%H:%M").unwrap();
        let times: Vec<String> =
            (0..values.len()).map(|i| (start + Duration::hours(i as i64)).format("%Y-%m-%dT%H:%M").to_string()).collect();
        json!({ "hourly": { "time": times, "temperature_2m": values, "wind_speed_100m": values } })
    }

    #[test]
    fn temperatures_are_weighted_calendar_days() {
        // two places: 10 °C and 20 °C all day, weights 3 and 1; the second day is short
        let body = json!([hours("2026-01-10T00:00", &[10.0; 30]), hours("2026-01-10T00:00", &[20.0; 30])]);
        let series = parse_hourly(&body, "temperature_2m").unwrap();
        let daily = daily_temperature(&series, &[place(3.0), place(1.0)]);
        assert_eq!(daily.len(), 1);
        assert!((daily[&day(2026, 1, 10)] - 12.5).abs() < 1e-12);
        // one place: an object, not an array
        assert_eq!(parse_hourly(&hours("2026-01-10T00:00", &[1.0]), "temperature_2m").unwrap().len(), 1);
        assert!(parse_hourly(&json!({"error": true}), "temperature_2m").is_err());
    }

    #[test]
    fn wind_is_a_tempo_day_of_power() {
        // 06:00 on the 10th to 05:00 on the 11th at full power, calm before
        let mut values = vec![0.0; 6];
        values.extend([60.0; 24]);
        let series = parse_hourly(&hours("2026-01-10T00:00", &values), "wind_speed_100m").unwrap();
        let daily = daily_wind_power(&series);
        assert_eq!(daily.get(&day(2026, 1, 10)), Some(&1.0));
        assert!(!daily.contains_key(&day(2026, 1, 9)), "six hours are no day");
    }

    #[test]
    fn odre_rows_make_tempo_days() {
        // quarters of an hour from 06:00 Paris (05:00 UTC in winter) for 24 h, then a lone one
        let start = tempo_day_start(day(2026, 1, 10));
        assert_eq!(start.to_rfc3339(), "2026-01-10T05:00:00+00:00");
        let rows: Vec<OdreRow> = (0..97)
            .map(|i| OdreRow {
                date_heure: (start + Duration::minutes(15 * i)).fixed_offset(),
                consommation: Some(if i < 48 { 60_000.0 } else { 70_000.0 }),
                eolien: Some(5_000.0),
                solaire: if i == 3 { None } else { Some(0.0) },
            })
            .collect();
        let days = tempo_days(&rows);
        assert_eq!(days.len(), 1, "the 11th has one quarter: not a day");
        let [c, w, s] = days[&day(2026, 1, 10)];
        assert!((c - (47.0 * 60_000.0 + 48.0 * 70_000.0) / 95.0).abs() < 1e-6, "{c}");
        assert_eq!((w, s), (5_000.0, 0.0));
    }

    #[test]
    fn the_net_load_keeps_a_year_of_finished_days() {
        let today = day(2026, 10, 3);
        let mut net = NetLoad::default();
        let load = (0..500).map(|i| (today - Duration::days(i), [50_000.0, 5_000.0, 1_000.0])).collect();
        let temps = (0..500).map(|i| (today - Duration::days(i), 10.0)).collect();
        net.merge(load, &temps, today);
        assert!(!net.load.contains_key(&today), "today is not over");
        assert_eq!(net.load.len(), 400);
        assert_eq!(net.past_year(today).len(), 365);
        assert!(net.levels(today).is_some());
        assert!(NetLoad::default().levels(today).is_none());
    }

    #[test]
    fn a_day_needs_its_two_days_before() {
        let weather = Weather {
            issued: Some(day(2026, 1, 10)),
            temperature: [(day(2026, 1, 9), 0.0), (day(2026, 1, 10), 10.0), (day(2026, 1, 11), 20.0)].into(),
            wind_power: [(day(2026, 1, 11), 0.5), (day(2026, 1, 10), 0.5)].into(),
        };
        let d = weather.day(day(2026, 1, 11)).unwrap();
        assert!((d.temperature - 13.0).abs() < 1e-12);
        assert!(weather.day(day(2026, 1, 10)).is_none());
    }
}
