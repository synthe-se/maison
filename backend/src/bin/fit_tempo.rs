//! Fits the Tempo forecast, on the Mac (never on the Pi), and writes `cache/tempo/`:
//! `model.json` (coefficients, errors by horizon, the backtest's scores), `netload.json`
//! (the past year RTE's normalisation needs) and every season's colours.
//!
//! ```text
//! cargo run --release --bin fit_tempo            # downloads kept in target/tempo-fit/
//! cargo run --release --bin fit_tempo -- --fresh # downloads everything again
//! ```
//!
//! Once a year (the grid changes), then deploy. What it does:
//!
//! 1. RTE's seasons from 2014, ODRE's éCO2mix (consumption, wind, solar) as Tempo days,
//!    Open-Meteo's archive (the model's cities and wind sites) and the ECMWF forecasts as
//!    they were issued (Open-Meteo's « previous runs », from 2024).
//! 2. Least squares of consumption, wind and solar (as ratios of their past year) on the
//!    weather and the calendar.
//! 3. The backtest: each day of the last full seasons, a model fitted before the season
//!    forecasts J+1..J+7 from that day's weather forecasts, through the service's own
//!    `outlook`, and is scored against RTE's colours.

use std::{collections::BTreeMap, path::PathBuf, time::Instant};

use chrono::{Duration, NaiveDate};
use maison_backend::{
    config::Config,
    store::{self, Access},
    tempo::{
        forecast::{
            consumption_features, dot, expected, solar_features, wind_features, Backtest, HorizonScore, Model,
            WeatherPoint, CONSUMPTION_FEATURES,
        },
        inputs::{coordinates, daily_temperature, daily_wind_power, parse_hourly, tempo_days, NetLoad, OdreRow, Series, Weather, WEATHER_MODEL},
        outlook,
        rules::{is_winter, season_bounds, season_name, season_start_year, Color, History, Quotas, FIRST_SEASON},
        source::{Sources, Values},
        HORIZON,
    },
};
use serde_json::Value;

const ARCHIVE_URL: &str = "https://archive-api.open-meteo.com/v1/archive";
const PREVIOUS_RUNS_URL: &str = "https://previous-runs-api.open-meteo.com/v1/forecast";
/// The ECMWF forecasts Open-Meteo kept: J+7 from February 2024.
const FIRST_FORECAST: &str = "2024-01-01";
const RIDGE: f64 = 1e-3;
const RHO: f64 = 0.7;
const RUNS: u32 = 400;

/// The twelve largest cities, weighted by their area's population (millions).
fn cities() -> Vec<WeatherPoint> {
    [
        ("Paris", 48.85, 2.35, 13.0),
        ("Lyon", 45.76, 4.84, 2.3),
        ("Marseille", 43.30, 5.37, 1.9),
        ("Lille", 50.63, 3.06, 1.5),
        ("Toulouse", 43.60, 1.44, 1.5),
        ("Bordeaux", 44.84, -0.58, 1.4),
        ("Nantes", 47.22, -1.55, 1.0),
        ("Strasbourg", 48.57, 7.75, 0.8),
        ("Rennes", 48.11, -1.68, 0.75),
        ("Nice", 43.70, 7.27, 1.0),
        ("Clermont-Ferrand", 45.78, 3.08, 0.5),
        ("Dijon", 47.32, 5.04, 0.4),
    ]
    .map(|(name, latitude, longitude, weight)| WeatherPoint::new(name, latitude, longitude, weight))
    .into()
}

/// Where France's wind farms are (the north and east plains, the Channel, the Atlantic
/// coast, the Aude).
fn wind_sites() -> Vec<WeatherPoint> {
    [
        ("Somme", 49.90, 2.30),
        ("Aisne", 49.85, 3.29),
        ("Marne", 48.96, 4.36),
        ("Haute-Marne", 48.64, 4.95),
        ("Aude", 43.18, 3.00),
        ("Côtes-d'Armor", 48.50, -2.76),
        ("Indre", 46.80, 1.70),
        ("Pays de Caux", 49.90, 0.20),
        ("Saint-Nazaire offshore", 47.15, -2.55),
        ("Vendée", 46.70, -1.40),
        ("Beauce", 48.30, 1.70),
        ("Aveyron", 44.30, 2.60),
    ]
    .map(|(name, latitude, longitude)| WeatherPoint::new(name, latitude, longitude, 1.0))
    .into()
}

type Daily = BTreeMap<NaiveDate, f64>;
type Failure = Box<dyn std::error::Error>;
/// A backtest season: its first year, the model fitted before it, its days with a forecast.
type Season = (i32, Model, Vec<(NaiveDate, Weather)>);

/// An hourly download: its file name, the API, the places, the variables, the weather model.
struct Hourly<'a> {
    name: &'a str,
    url: &'a str,
    places: &'a [WeatherPoint],
    keys: &'a [String],
    model: Option<&'a str>,
}

struct Downloads {
    client: reqwest::Client,
    dir: PathBuf,
}

impl Downloads {
    /// The answer, from `target/tempo-fit/<name>` when already there; Open-Meteo's « too
    /// many requests » is waited out.
    async fn get(&self, name: &str, url: &str, query: &[(&str, String)]) -> Result<String, Failure> {
        let path = self.dir.join(name);
        if let Ok(text) = std::fs::read_to_string(&path) {
            return Ok(text);
        }
        for _ in 0..6 {
            eprintln!("downloading {name}…");
            let response = self.client.get(url).query(query).send().await?;
            if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                eprintln!("  rate limited: waiting a minute");
                tokio::time::sleep(std::time::Duration::from_secs(65)).await;
                continue;
            }
            if !response.status().is_success() {
                let status = response.status();
                return Err(format!("{name}: {status} {}", response.text().await?).into());
            }
            let text = response.text().await?;
            store::write_bytes(&path, text.as_bytes(), Access::Shared)?;
            return Ok(text);
        }
        Err(format!("{name}: still rate limited").into())
    }

    /// Hourly `keys` at `places` from `url`, in chunks of a few years (the answers are big).
    async fn hourly(&self, ask: Hourly<'_>, from: NaiveDate, to: NaiveDate) -> Result<Vec<Value>, Failure> {
        let Hourly { name, url, places, keys, model } = ask;
        let mut bodies = Vec::new();
        let mut start = from;
        while start <= to {
            let end = (start + Duration::days(4 * 365)).min(to);
            let mut query = coordinates(places).to_vec();
            query.extend([
                ("hourly", keys.join(",")),
                ("timezone", "Europe/Paris".into()),
                ("start_date", start.to_string()),
                ("end_date", end.to_string()),
            ]);
            if let Some(model) = model {
                query.push(("models", model.into()));
            }
            let text = self.get(&format!("{name}-{start}-{end}.json"), url, &query).await?;
            bodies.push(serde_json::from_str(&text)?);
            start = end + Duration::days(1);
        }
        Ok(bodies)
    }
}

/// `daily` of every chunk, merged.
fn merged(bodies: &[Value], key: &str, daily: impl Fn(&[Series]) -> Daily) -> Result<Daily, Failure> {
    let mut out = Daily::new();
    for body in bodies {
        out.extend(daily(&parse_hourly(body, key)?));
    }
    Ok(out)
}

/// Solves `a x = b` (Gauss, partial pivoting): the normal equations are small.
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Vec<f64> {
    let n = b.len();
    for col in 0..n {
        let pivot = (col..n).max_by(|i, j| a[*i][col].abs().total_cmp(&a[*j][col].abs())).expect("rows");
        a.swap(col, pivot);
        b.swap(col, pivot);
        for row in col + 1..n {
            let f = a[row][col] / a[col][col];
            let (above, below) = a.split_at_mut(row);
            for (x, p) in below[0][col..].iter_mut().zip(&above[col][col..]) {
                *x -= f * p;
            }
            b[row] -= f * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let s: f64 = (row + 1..n).map(|k| a[row][k] * x[k]).sum();
        x[row] = (b[row] - s) / a[row][row];
    }
    x
}

/// Ridge least squares of `y` on the rows of `x`; the residuals' standard deviation.
fn least_squares(x: &[Vec<f64>], y: &[f64]) -> (Vec<f64>, f64) {
    let n = x[0].len();
    let mut a = vec![vec![0.0; n]; n];
    let mut b = vec![0.0; n];
    for (row, target) in x.iter().zip(y) {
        for i in 0..n {
            b[i] += row[i] * target;
            for j in 0..n {
                a[i][j] += row[i] * row[j];
            }
        }
    }
    for (i, row) in a.iter_mut().enumerate() {
        row[i] += RIDGE;
    }
    let beta = solve(a, b);
    let residuals: Vec<f64> = x.iter().zip(y).map(|(row, target)| target - dot(row, &beta)).collect();
    let sd = (residuals.iter().map(|r| r * r).sum::<f64>() / residuals.len() as f64).sqrt();
    (beta, sd)
}

struct Data {
    colors: History,
    net: NetLoad,
    temperature: Daily,
    wind: Daily,
    /// Forecast temperatures and wind power by lead (J+1 first).
    forecast_temperature: Vec<Daily>,
    forecast_wind: Vec<Daily>,
}

impl Data {
    /// What the weather service had to say on `issued`: past days as observed, coming ones
    /// as forecast that day.
    fn weather(&self, issued: NaiveDate) -> Option<Weather> {
        let mut weather = Weather { issued: Some(issued), ..Default::default() };
        for back in 0..3 {
            let day = issued - Duration::days(back);
            weather.temperature.insert(day, *self.temperature.get(&day)?);
        }
        for lead in 1..=HORIZON {
            let day = issued + Duration::days(lead);
            let i = lead as usize - 1;
            weather.temperature.insert(day, *self.forecast_temperature[i].get(&day)?);
            weather.wind_power.insert(day, *self.forecast_wind[i].get(&day)?);
        }
        Some(weather)
    }

    /// The observed weather of a past day.
    fn observed(&self, date: NaiveDate) -> Option<Weather> {
        let mut weather = Weather { issued: Some(date), ..Default::default() };
        for back in 0..3 {
            let day = date - Duration::days(back);
            weather.temperature.insert(day, *self.temperature.get(&day)?);
        }
        weather.wind_power.insert(date, *self.wind.get(&date)?);
        Some(weather)
    }

    /// RTE's normalised net consumption of a past day.
    fn actual(&self, date: NaiveDate) -> Option<f64> {
        let [c, w, s] = *self.net.load.get(&date)?;
        Some(self.net.levels(date)?.normalise(c - w - s))
    }
}

/// Fits on the days before `until`.
fn fit(data: &Data, until: NaiveDate, base: &Model) -> Model {
    let (mut xc, mut yc, mut xw, mut yw, mut xs, mut ys) = (vec![], vec![], vec![], vec![], vec![], vec![]);
    let first = season_bounds(FIRST_SEASON).0;
    for date in first.iter_days().take_while(|d| *d < until) {
        let (Some(weather), Some([c, w, s]), Some(levels)) = (data.observed(date), data.net.load.get(&date), data.net.levels(date)) else {
            continue;
        };
        let day = weather.day(date).expect("observed days are whole");
        xc.push(consumption_features(date, day.temperature).to_vec());
        yc.push(c / levels.consumption);
        xw.push(wind_features(day.wind_power).to_vec());
        yw.push(w / levels.wind);
        xs.push(solar_features(date).to_vec());
        ys.push(s / levels.solar);
    }
    let (consumption, c_sd) = least_squares(&xc, &yc);
    let (wind, w_sd) = least_squares(&xw, &yw);
    let (solar, s_sd) = least_squares(&xs, &ys);
    eprintln!("fit before {until}: {} days, residual sd consumption {c_sd:.4}, wind {w_sd:.4}, solar {s_sd:.4}", yc.len());
    assert_eq!(consumption.len(), CONSUMPTION_FEATURES);
    Model { consumption, wind, solar, fitted_through: until - Duration::days(1), ..base.clone() }
}

/// The days a season's backtest forecasts from: every day with its weather forecast.
fn issues(data: &Data, start_year: i32) -> Vec<(NaiveDate, Weather)> {
    let (first, last) = season_bounds(start_year);
    first
        .iter_days()
        .take_while(|d| *d <= last - Duration::days(HORIZON))
        .filter_map(|d| Some((d, data.weather(d)?)))
        .collect()
}

#[derive(Default, Clone, Copy)]
struct Tally {
    days: u32,
    right: u32,
    winter_days: u32,
    winter_right: u32,
    blue_right: u32,
    brier: f64,
    /// [actual][predicted]
    confusion: [[u32; 3]; 3],
}

impl Tally {
    fn add(&mut self, date: NaiveDate, actual: Color, predicted: Color, p: [f64; 3]) {
        let ok = u32::from(actual == predicted);
        self.days += 1;
        self.right += ok;
        if is_winter(date) {
            self.winter_days += 1;
            self.winter_right += ok;
        }
        self.blue_right += u32::from(actual == Color::Blue);
        self.brier += Color::ALL.iter().map(|c| (p[c.index()] - f64::from(u32::from(*c == actual))).powi(2)).sum::<f64>();
        self.confusion[actual.index()][predicted.index()] += 1;
    }

    fn f1(&self, color: Color) -> f64 {
        let i = color.index();
        let tp = f64::from(self.confusion[i][i]);
        let predicted: u32 = (0..3).map(|a| self.confusion[a][i]).sum();
        let actual: u32 = self.confusion[i].iter().sum();
        if tp == 0.0 { 0.0 } else { 2.0 * tp / f64::from(predicted + actual) }
    }

    fn score(&self, horizon: u32) -> HorizonScore {
        let share = |a: u32, b: u32| if b == 0 { 0.0 } else { f64::from(a) / f64::from(b) };
        let round = |x: f64| (x * 1000.0).round() / 1000.0;
        HorizonScore {
            horizon,
            days: self.days,
            accuracy: round(share(self.right, self.days)),
            winter_accuracy: round(share(self.winter_right, self.winter_days)),
            red_f1: round(self.f1(Color::Red)),
            white_f1: round(self.f1(Color::White)),
            brier: round(self.brier / f64::from(self.days.max(1))),
            always_blue: round(share(self.blue_right, self.days)),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Failure> {
    let fresh = std::env::args().any(|a| a == "--fresh");
    let config = Config::from_env();
    let out = config.source_root.join("cache/tempo");
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tempo-fit");
    if fresh {
        let _ = std::fs::remove_dir_all(&dir);
    }
    std::fs::create_dir_all(&dir)?;
    let client = maison_backend::net::web_client(concat!("Maison-fit/", env!("CARGO_PKG_VERSION")), std::time::Duration::from_secs(600))?;
    let downloads = Downloads { client: client.clone(), dir };
    let today = maison_backend::util::house_today();
    let yesterday = today - Duration::days(1);
    let (cities, wind_sites) = (cities(), wind_sites());

    // 1. RTE's seasons, kept in cache/tempo/ as the service reads them
    let sources = Sources::from_config(&config);
    let mut colors = History::new();
    for start in FIRST_SEASON..=season_start_year(today) {
        let season = sources.season(&client, start, today).await?;
        store::write_json(&out.join(format!("tempo_history_{}.json", season_name(start))), &Values::from_history(&season), Access::Shared)?;
        colors.extend(season);
    }

    // 2. éCO2mix as Tempo days: the consolidated years, then the current months
    let odre = config.tempo_odre_url.trim_end_matches('/');
    let mut load = BTreeMap::new();
    for (dataset, since) in [("eco2mix-national-cons-def", season_bounds(FIRST_SEASON - 1).0), ("eco2mix-national-tr", today - Duration::days(400))] {
        let query = [
            ("select", "date_heure,consommation,eolien,solaire".to_string()),
            ("where", format!("date_heure >= '{since}T00:00:00Z'")),
            ("order_by", "date_heure".to_string()),
        ];
        let text = downloads.get(&format!("{dataset}.json"), &format!("{odre}/{dataset}/exports/json"), &query).await?;
        let rows: Vec<OdreRow> = serde_json::from_str(&text)?;
        // the consolidated data first; the current months only add what it lacks
        for (day, values) in tempo_days(&rows) {
            load.entry(day).or_insert(values);
        }
    }

    // 3. the weather: observed, and forecast as it was
    let from = season_bounds(FIRST_SEASON - 1).1 - Duration::days(5);
    let keys = |k: &str| vec![k.to_string()];
    let (temperature_keys, wind_keys) = (keys("temperature_2m"), keys("wind_speed_100m"));
    let ask = |name, url, places, keys, model| Hourly { name, url, places, keys, model };
    let temps = downloads.hourly(ask("archive-temperature", ARCHIVE_URL, &cities, &temperature_keys, None), from, yesterday).await?;
    let winds = downloads.hourly(ask("archive-wind", ARCHIVE_URL, &wind_sites, &wind_keys, None), from, yesterday).await?;
    let leads = |k: &str| (1..=HORIZON).map(|h| format!("{k}_previous_day{h}")).collect::<Vec<_>>();
    let first_forecast: NaiveDate = FIRST_FORECAST.parse()?;
    let (temperature_leads, wind_leads) = (leads("temperature_2m"), leads("wind_speed_100m"));
    let ftemps = downloads
        .hourly(ask("forecast-temperature", PREVIOUS_RUNS_URL, &cities, &temperature_leads, Some(WEATHER_MODEL)), first_forecast, yesterday)
        .await?;
    let fwinds = downloads
        .hourly(ask("forecast-wind", PREVIOUS_RUNS_URL, &wind_sites, &wind_leads, Some(WEATHER_MODEL)), first_forecast, yesterday)
        .await?;
    let temperature = merged(&temps, "temperature_2m", |s| daily_temperature(s, &cities))?;
    let data = Data {
        colors,
        net: NetLoad { load, temperature: temperature.clone() },
        wind: merged(&winds, "wind_speed_100m", daily_wind_power)?,
        temperature,
        forecast_temperature: temperature_leads.iter().map(|k| merged(&ftemps, k, |s| daily_temperature(s, &cities))).collect::<Result<_, _>>()?,
        forecast_wind: wind_leads.iter().map(|k| merged(&fwinds, k, daily_wind_power)).collect::<Result<_, _>>()?,
    };

    // 4. the backtest: the full seasons with a week of forecasts every day
    let base = Model {
        version: today.to_string(),
        fitted_through: yesterday,
        cities,
        wind_sites,
        consumption: vec![],
        wind: vec![],
        solar: vec![],
        sigma: vec![],
        rho: RHO,
        runs: RUNS,
        backtest: Backtest::default(),
    };
    let seasons: Vec<i32> = (season_start_year(first_forecast) + 1..season_start_year(today))
        .filter(|start| issues(&data, *start).len() > 300)
        .collect();
    let fitted: Vec<Season> =
        seasons.iter().map(|start| (*start, fit(&data, season_bounds(*start).0, &base), issues(&data, *start))).collect();

    // the error of x by horizon, in winter (the summer is far from every threshold)
    let mut errors: Vec<Vec<f64>> = vec![vec![]; HORIZON as usize];
    for (_, model, issued) in &fitted {
        for (issue, weather) in issued {
            let Some(levels) = data.net.levels(*issue) else { continue };
            for lead in 1..=HORIZON {
                let date = *issue + Duration::days(lead);
                if let (true, Some(day), Some(actual)) = (is_winter(date), weather.day(date), data.actual(date)) {
                    errors[lead as usize - 1].push(expected(model, &levels, &day) - actual);
                }
            }
        }
    }
    let sigma: Vec<f64> = errors
        .iter()
        .map(|e| {
            let mean = e.iter().sum::<f64>() / e.len() as f64;
            let sd = (e.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / e.len() as f64).sqrt();
            (sd * 1000.0).round() / 1000.0
        })
        .collect();
    eprintln!("error of x by horizon (winter): {sigma:?}");

    let mut tallies = vec![Tally::default(); HORIZON as usize];
    let mut timing = (std::time::Duration::ZERO, 0u32);
    for (_, model, issued) in &fitted {
        let model = Model { sigma: sigma.clone(), ..model.clone() };
        for (issue, weather) in issued {
            // what was known before 10:40: up to the day itself
            let known: History = data.colors.range(season_bounds(season_start_year(*issue)).0..=*issue).map(|(d, c)| (*d, *c)).collect();
            let levels = data.net.levels(*issue);
            let started = Instant::now();
            let (days, _) = outlook(&model, &known, Quotas::default(), Some(weather), levels, *issue);
            timing = (timing.0 + started.elapsed(), timing.1 + 1);
            for day in days {
                if let Some(actual) = data.colors.get(&day.date) {
                    let p = [day.probabilities.blue, day.probabilities.white, day.probabilities.red];
                    tallies[day.horizon as usize - 1].add(day.date, *actual, day.color, p);
                }
            }
        }
    }
    let horizons: Vec<HorizonScore> = tallies.iter().enumerate().map(|(i, t)| t.score(i as u32 + 1)).collect();
    println!("backtest on {}:", seasons.iter().map(|s| season_name(*s)).collect::<Vec<_>>().join(", "));
    println!("  J+h  days  accuracy  winter  red F1  white F1  Brier  always blue");
    for s in &horizons {
        println!(
            "  J+{}  {:4}  {:8.3}  {:6.3}  {:6.3}  {:8.3}  {:5.3}  {:11.3}",
            s.horizon, s.days, s.accuracy, s.winter_accuracy, s.red_f1, s.white_f1, s.brier, s.always_blue
        );
    }
    println!("one forecast (normalisation, rules, {RUNS} draws × 7 days): {:?} on this machine", timing.0 / timing.1.max(1));

    // 5. the model shipped: every day up to yesterday
    let mut model = fit(&data, today, &base);
    model.sigma = sigma;
    model.backtest = Backtest { seasons: seasons.iter().map(|s| season_name(*s)).collect(), horizons };
    model.fitted_through = *data.net.load.keys().next_back().ok_or("no load")?;
    store::write_json(&out.join("model.json"), &model, Access::Shared)?;

    let mut net = NetLoad::default();
    net.merge(data.net.load.clone(), &data.net.temperature, today);
    store::write_json(&out.join("netload.json"), &net, Access::Shared)?;
    println!("wrote {}", out.display());
    Ok(())
}
