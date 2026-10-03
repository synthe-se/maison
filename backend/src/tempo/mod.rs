//! Tempo, EDF's tariff whose days RTE colours blue, white or red: the official colours
//! (today, tomorrow from about 10:40, the seasons), the days left, the prices, and a
//! forecast to J+7 that is labelled as one (`forecast.rs`, measured in `model.json`).
//!
//! Everything lives in `cache/tempo/`: the seasons (`tempo_history_<season>.json`), the
//! fitted model (`model.json`, versioned, written by `fit_tempo`), the past year of net
//! consumption (`netload.json`, kept up to date from ODRE) and the last weather forecast
//! (`weather.json`). A source down means older data, said as such, never nothing.

pub mod forecast;
pub mod inputs;
pub mod rules;
pub mod source;
pub mod tariffs;

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::Serialize;
use tokio::sync::{Mutex, MutexGuard};

pub use rules::{current_season, Color};

use crate::{
    config::Config,
    error::AppError,
    store::{self, Access, Corrupt},
    util::{house_today, HOUSE_TZ},
};
use forecast::{expected, likeliest, seed, simulate, Ahead, Levels, Model};
use inputs::{fetch_load, fetch_weather, NetLoad, Weather};
use rules::{blue_days, parse_season, season_bounds, season_name, season_start_year, DayRules, History, Quotas, Stock};
use source::{Sources, Values};
use tariffs::Tariffs;

const USER_AGENT: &str = concat!("Maison/", env!("CARGO_PKG_VERSION"));
/// A source that failed is asked again after this; so is tomorrow until RTE publishes it.
const RETRY: Duration = Duration::minutes(15);
const WEATHER_EVERY: Duration = Duration::hours(3);
const LOAD_EVERY: Duration = Duration::hours(3);
const TARIFFS_EVERY: Duration = Duration::hours(24);
/// éCO2mix's dataset of the current months (the consolidated years come with `fit_tempo`).
const LOAD_DATASET: &str = "eco2mix-national-tr";
/// How far back a gap in `netload.json` is filled.
const LOAD_REACH_DAYS: i64 = 370;
/// The forecast's reach.
pub const HORIZON: i64 = 7;
/// A forecast tomorrow this likely goes on the rabbit (marked as one).
pub const RABBIT_CONFIDENCE: f64 = 0.8;

#[derive(Clone)]
pub struct TempoService {
    inner: Arc<Inner>,
}

struct Inner {
    client: reqwest::Client,
    sources: Sources,
    tariffs_url: String,
    edf_url: String,
    odre_url: String,
    open_meteo_url: String,
    dir: PathBuf,
    model: Option<Model>,
    /// One refresh at a time: the dashboard and the page ask together, upstream once. Held
    /// across the sources' answers (seconds, up to their timeouts).
    state: Mutex<State>,
    /// The last answers, served while a refresh holds `state`: a reader never waits on
    /// RTE when it has today's already.
    last_today: Last<Today>,
    last_forecast: Last<Forecast>,
}

/// An answer and the day it is for.
type Last<T> = std::sync::Mutex<Option<(NaiveDate, T)>>;

/// Who answers: the caller, holding the refresh lock, or the last answer (a refresh runs).
enum Turn<'a, T> {
    Refresh(MutexGuard<'a, State>),
    Last(T),
}

/// When a source was last asked, and whether it answered.
#[derive(Debug, Clone, Copy)]
struct Asked {
    at: DateTime<Utc>,
    ok: bool,
}

impl Asked {
    /// Whether to ask again: `every` after an answer, [`RETRY`] after a failure.
    fn due(asked: Option<Asked>, now: DateTime<Utc>, every: Duration) -> bool {
        asked.is_none_or(|a| now - a.at >= if a.ok { every } else { RETRY })
    }
}

#[derive(Default)]
struct State {
    seasons: HashMap<i32, History>,
    current: Option<Asked>,
    tariffs: Option<Tariffs>,
    tariffs_asked: Option<Asked>,
    /// EDF's quotas, asked once a day.
    quotas: Option<(NaiveDate, Quotas)>,
    weather: Option<Weather>,
    weather_asked: Option<Asked>,
    netload: Option<NetLoad>,
    load_asked: Option<Asked>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Day {
    pub date: NaiveDate,
    pub color: Option<Color>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct Count {
    pub used: u32,
    pub total: u32,
    pub remaining: u32,
}

/// A season's days per colour: used (published, tomorrow included), quota, left.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SeasonStock {
    pub season: String,
    pub blue: Count,
    pub white: Count,
    pub red: Count,
}

impl SeasonStock {
    fn new(start_year: i32, history: &History, quotas: Quotas) -> Self {
        let (first, last) = season_bounds(start_year);
        let used = |color: Color| history.range(first..=last).filter(|(_, c)| **c == color).count() as u32;
        let count = |color: Color, total: u32| Count { used: used(color), total, remaining: total.saturating_sub(used(color)) };
        Self {
            season: season_name(start_year),
            blue: count(Color::Blue, blue_days(start_year, quotas.red, quotas.white)),
            white: count(Color::White, quotas.white),
            red: count(Color::Red, quotas.red),
        }
    }
}

/// Peak hours (« heures pleines »), local time: the rest is off-peak. A Tempo day runs
/// 06:00 to 06:00: before 06:00 the colour in force is the day before's.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hours {
    pub peak_start: &'static str,
    pub peak_end: &'static str,
}

pub const HOURS: Hours = Hours { peak_start: "06:00", peak_end: "22:00" };

/// Yesterday (in force until 06:00), today, tomorrow (`None` until RTE publishes it), the
/// prices and their hours, and the days left.
#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct Probabilities {
    #[serde(rename = "BLUE")]
    pub blue: f64,
    #[serde(rename = "WHITE")]
    pub white: f64,
    #[serde(rename = "RED")]
    pub red: f64,
}

impl From<[f64; 3]> for Probabilities {
    fn from([blue, white, red]: [f64; 3]) -> Self {
        Self { blue, white, red }
    }
}

/// How the model did at this horizon on past seasons (`model.json`).
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Reliability {
    pub accuracy: f64,
    pub winter_accuracy: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reliability: Option<Reliability>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub version: String,
    pub fitted_through: NaiveDate,
    /// How it did on past seasons, by horizon (what « reliability » is said from).
    pub backtest: forecast::Backtest,
}

/// The days after today up to J+7: RTE's when published, the forecast's after.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Forecast {
    pub issued: NaiveDate,
    /// The day of the weather forecast used (older than today: Open-Meteo is down, the
    /// errors are counted from that day).
    pub weather_issued: Option<NaiveDate>,
    pub stale: bool,
    pub model: Option<ModelInfo>,
    pub days: Vec<ForecastDay>,
    pub stock: SeasonStock,
    /// Why the forecast stops short of J+7.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarDay {
    pub date: NaiveDate,
    pub color: Color,
    pub is_actual: bool,
    pub is_prediction: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub probabilities: Option<Probabilities>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
}

/// A season's days, the forecast's after the published ones for the current season.
#[derive(Debug, Clone, Serialize)]
pub struct Calendar {
    pub season: String,
    pub calendar: Vec<CalendarDay>,
    pub stock: SeasonStock,
}

/// The days after today up to J+7: the published ones, then the forecast's from `weather`
/// and the past year's `levels`. Pure: the service and the tests call it.
pub fn outlook(
    model: &Model,
    history: &History,
    quotas: Quotas,
    weather: Option<&Weather>,
    levels: Option<Levels>,
    today: NaiveDate,
) -> (Vec<ForecastDay>, Option<String>) {
    let end = today + Duration::days(HORIZON);
    let mut days = Vec::new();
    let mut start = today + Duration::days(1);
    while start <= end {
        let Some(color) = history.get(&start) else { break };
        let mut p = [0.0; 3];
        p[color.index()] = 1.0;
        days.push(ForecastDay {
            date: start,
            horizon: (start - today).num_days() as u32,
            official: true,
            color: *color,
            probabilities: p.into(),
            confidence: 1.0,
            reliability: None,
        });
        start += Duration::days(1);
    }
    if start > end {
        return (days, None);
    }
    let (Some(weather), Some(levels)) = (weather, levels) else {
        let why = if weather.is_none() { "no weather forecast yet" } else { "not enough past consumption to normalise" };
        return (days, Some(why.into()));
    };
    let issued = weather.issued.unwrap_or(today);
    let mut ahead = Vec::new();
    let mut horizons = Vec::new();
    for date in start.iter_days().take_while(|d| *d <= end) {
        let Some(day) = weather.day(date) else { break };
        let horizon = (date - issued).num_days().max(1) as u32;
        ahead.push(Ahead { rules: DayRules::new(date), x: expected(model, &levels, &day), sigma: model.sigma(horizon) });
        horizons.push(horizon);
    }
    let stock = Stock::before(history, quotas, start);
    let probabilities = simulate(&ahead, stock, model.rho, model.runs, seed(today));
    for ((a, p), horizon) in ahead.iter().zip(&probabilities).zip(&horizons) {
        let color = likeliest(p);
        days.push(ForecastDay {
            date: a.rules.date,
            horizon: (a.rules.date - today).num_days() as u32,
            official: false,
            color,
            probabilities: (*p).into(),
            confidence: p[color.index()],
            reliability: model
                .score(*horizon)
                .map(|s| Reliability { accuracy: s.accuracy, winter_accuracy: s.winter_accuracy }),
        });
    }
    let short = days.last().is_none_or(|d| d.date < end);
    (days, short.then(|| "the weather forecast does not reach J+7".into()))
}

impl TempoService {
    pub fn from_config(config: &Config) -> Result<Self, AppError> {
        let client = crate::net::web_client(USER_AGENT, std::time::Duration::from_secs(30))?;
        let dir = config.source_root.join("cache").join("tempo");
        let model = match store::read_json::<Option<Model>>(&dir.join("model.json"), Corrupt::Fail) {
            Ok(Some(model)) => Some(model),
            Ok(None) => {
                tracing::warn!("no cache/tempo/model.json: no Tempo forecast (run fit_tempo)");
                None
            }
            Err(error) => {
                tracing::warn!(%error, "unreadable Tempo model: no forecast");
                None
            }
        };
        let url = |u: &str| u.trim_end_matches('/').to_string();
        Ok(Self {
            inner: Arc::new(Inner {
                client,
                sources: Sources::from_config(config),
                tariffs_url: config.tempo_tarifs_url.clone(),
                edf_url: url(&config.tempo_edf_url),
                odre_url: url(&config.tempo_odre_url),
                open_meteo_url: url(&config.open_meteo_url),
                dir,
                model,
                state: Mutex::new(State::default()),
                last_today: Last::default(),
                last_forecast: Last::default(),
            }),
        })
    }

    /// The refresh lock; or, while a refresh holds it, the last answer for today (unless
    /// `force`: then wait for it).
    async fn turn<'a, T: Clone>(&'a self, force: bool, last: &Last<T>) -> Turn<'a, T> {
        if let Ok(state) = self.inner.state.try_lock() {
            return Turn::Refresh(state);
        }
        if !force {
            let kept = last.lock().unwrap_or_else(|p| p.into_inner()).clone();
            if let Some((day, answer)) = kept.filter(|(day, _)| *day == house_today()) {
                tracing::debug!(%day, "tempo: a refresh runs, the last answer is served");
                return Turn::Last(answer);
            }
        }
        Turn::Refresh(self.inner.state.lock().await)
    }

    /// Today, tomorrow, the prices and the days left; `force` asks the sources again.
    pub async fn today(&self, force: bool) -> Result<Today, AppError> {
        let mut state = match self.turn(force, &self.inner.last_today).await {
            Turn::Last(today) => return Ok(today),
            Turn::Refresh(state) => state,
        };
        let answer = self.today_in(&mut state, force).await?;
        remember(&self.inner.last_today, answer.today.date, &answer);
        Ok(answer)
    }

    async fn today_in(&self, state: &mut State, force: bool) -> Result<Today, AppError> {
        let (now, today) = (Utc::now(), house_today());
        let history = self.current_season(state, today, now, force).await?;
        let quotas = self.quotas(state, today, &history).await;
        let tariffs = self.tariffs(state, today, now, force).await;
        let yesterday = today - Duration::days(1);
        // on 1 September, yesterday is the last season's
        let before = if season_start_year(yesterday) == season_start_year(today) {
            history.get(&yesterday).copied()
        } else {
            match self.past_season(state, season_start_year(yesterday), today).await {
                Ok(past) => past.get(&yesterday).copied(),
                Err(error) => {
                    tracing::warn!(%error, "last season unavailable: yesterday's colour unknown");
                    None
                }
            }
        };
        let day = |date: NaiveDate| Day { date, color: history.get(&date).copied() };
        Ok(Today {
            yesterday: Day { date: yesterday, color: before },
            today: day(today),
            tomorrow: day(today + Duration::days(1)),
            tariffs,
            hours: HOURS,
            stock: SeasonStock::new(season_start_year(today), &history, quotas),
            last_updated: state.current.filter(|a| a.ok).map(|a| a.at),
            cached: state.current.is_some_and(|a| !a.ok),
        })
    }

    /// J+1 to J+7: RTE's colours, then the forecast's.
    pub async fn forecast(&self, force: bool) -> Result<Forecast, AppError> {
        let mut state = match self.turn(force, &self.inner.last_forecast).await {
            Turn::Last(forecast) => return Ok(forecast),
            Turn::Refresh(state) => state,
        };
        let answer = self.forecast_in(&mut state, force).await?;
        remember(&self.inner.last_forecast, answer.issued, &answer);
        Ok(answer)
    }

    async fn forecast_in(&self, state: &mut State, force: bool) -> Result<Forecast, AppError> {
        let (now, today) = (Utc::now(), house_today());
        let history = self.current_season(state, today, now, false).await?;
        let quotas = self.quotas(state, today, &history).await;
        let stock = SeasonStock::new(season_start_year(today), &history, quotas);
        let Some(model) = self.inner.model.as_ref() else {
            let (days, _) = outlook(&Model::empty(), &history, quotas, None, None, today);
            return Ok(Forecast {
                issued: today,
                weather_issued: None,
                stale: false,
                model: None,
                days,
                stock,
                note: Some("no forecast model".into()),
            });
        };
        self.refresh_load(state, today, now).await;
        self.refresh_weather(state, model, today, now, force).await;
        let levels = state.netload.as_ref().and_then(|n| n.levels(today));
        let (days, note) = outlook(model, &history, quotas, state.weather.as_ref(), levels, today);
        let weather_issued = state.weather.as_ref().and_then(|w| w.issued);
        Ok(Forecast {
            issued: today,
            weather_issued,
            stale: weather_issued.is_some_and(|d| d < today),
            model: Some(ModelInfo {
                version: model.version.clone(),
                fitted_through: model.fitted_through,
                backtest: model.backtest.clone(),
            }),
            days,
            stock,
            note,
        })
    }

    /// A season's published days, and for the current one the forecast's after them.
    pub async fn calendar(&self, season: Option<&str>) -> Result<Calendar, AppError> {
        let today = house_today();
        let start = season.map_or(Ok(season_start_year(today)), |s| parse_season(s, today))?;
        let current = start == season_start_year(today);
        // one turn of the lock for all of it
        let mut state = self.inner.state.lock().await;
        let forecast = if current { Some(self.forecast_in(&mut state, false).await?) } else { None };
        let history = if current {
            self.current_season(&mut state, today, Utc::now(), false).await?
        } else {
            self.past_season(&mut state, start, today).await?
        };
        let quotas = if current { self.quotas(&mut state, today, &history).await } else { Quotas::default() };
        drop(state);
        let mut calendar: Vec<CalendarDay> = history
            .iter()
            .map(|(date, color)| CalendarDay {
                date: *date,
                color: *color,
                is_actual: true,
                is_prediction: false,
                probabilities: None,
                confidence: None,
            })
            .collect();
        let predicted = forecast.iter().flat_map(|f| &f.days).filter(|d| !d.official);
        calendar.extend(predicted.map(|d| CalendarDay {
            date: d.date,
            color: d.color,
            is_actual: false,
            is_prediction: true,
            probabilities: Some(d.probabilities),
            confidence: Some(d.confidence),
        }));
        Ok(Calendar { season: season_name(start), calendar, stock: SeasonStock::new(start, &history, quotas) })
    }

    /// The rabbit's colours: today's, and tomorrow's when published or forecast at
    /// [`RABBIT_CONFIDENCE`] (`true`: a forecast).
    pub async fn rabbit_colors(&self, force: bool) -> Result<(Color, Option<(Color, bool)>), AppError> {
        let today = self.today(force).await?;
        let color = today.today.color.ok_or_else(|| AppError::service_unavailable("Today's Tempo color is not available yet"))?;
        if let Some(tomorrow) = today.tomorrow.color {
            return Ok((color, Some((tomorrow, false))));
        }
        let forecast = self.forecast(false).await?;
        let tomorrow = forecast
            .days
            .iter()
            .find(|d| d.date == today.tomorrow.date && d.confidence >= RABBIT_CONFIDENCE)
            .map(|d| (d.color, !d.official));
        Ok((color, tomorrow))
    }

    fn season_path(&self, start_year: i32) -> PathBuf {
        self.inner.dir.join(format!("tempo_history_{}.json", season_name(start_year)))
    }

    fn read_season(&self, start_year: i32) -> Result<History, AppError> {
        Ok(store::read_json::<Values>(&self.season_path(start_year), Corrupt::Reset)?.history())
    }

    async fn write_season(&self, start_year: i32, history: &History) -> Result<(), AppError> {
        store::write_json_async(&self.season_path(start_year), &Values::from_history(history), Access::Shared).await
    }

    /// The current season: asked once a day, every [`RETRY`] while tomorrow is unknown or
    /// the sources fail (then the file, said `cached`).
    async fn current_season(&self, state: &mut State, today: NaiveDate, now: DateTime<Utc>, force: bool) -> Result<History, AppError> {
        let start = season_start_year(today);
        if let std::collections::hash_map::Entry::Vacant(entry) = state.seasons.entry(start) {
            entry.insert(self.read_season(start)?);
        }
        let tomorrow_known = state.seasons[&start].contains_key(&(today + Duration::days(1)));
        let asked_today = state.current.is_some_and(|a| a.at.with_timezone(&HOUSE_TZ).date_naive() == today);
        let due = force
            || !asked_today
            || state.current.is_some_and(|a| (!a.ok || !tomorrow_known) && now - a.at >= RETRY);
        if due {
            match self.inner.sources.season(&self.inner.client, start, today).await {
                Ok(fetched) => {
                    let history = state.seasons.entry(start).or_default();
                    history.extend(fetched);
                    self.write_season(start, history).await?;
                    state.current = Some(Asked { at: now, ok: true });
                }
                Err(error) => {
                    tracing::warn!(%error, "Tempo colours unavailable: using the season's file");
                    state.current = Some(Asked { at: now, ok: false });
                }
            }
        }
        let history = &state.seasons[&start];
        if history.is_empty() {
            return Err(AppError::service_unavailable("Tempo colours unavailable"));
        }
        Ok(history.clone())
    }

    /// A finished season: its file once complete, else asked (and the file kept up to date).
    async fn past_season(&self, state: &mut State, start: i32, today: NaiveDate) -> Result<History, AppError> {
        if let Some(history) = state.seasons.get(&start) {
            return Ok(history.clone());
        }
        let mut history = self.read_season(start)?;
        if !history.contains_key(&season_bounds(start).1) {
            match self.inner.sources.season(&self.inner.client, start, today).await {
                Ok(fetched) => {
                    history.extend(fetched);
                    self.write_season(start, &history).await?;
                }
                // not remembered: asked again next time
                Err(error) if !history.is_empty() => {
                    tracing::warn!(%error, season = start, "Tempo season unavailable: using its file");
                    return Ok(history);
                }
                Err(error) => return Err(error),
            }
        }
        state.seasons.insert(start, history.clone());
        Ok(history)
    }

    /// EDF's quotas for the current season (once a day), checked against the colours
    /// counted; the CRE's when EDF does not answer.
    async fn quotas(&self, state: &mut State, today: NaiveDate, history: &History) -> Quotas {
        if let Some((day, quotas)) = state.quotas {
            if day == today {
                return quotas;
            }
        }
        let previous = state.quotas.map(|(_, q)| q).unwrap_or_default();
        let quotas = match source::edf_season(&self.inner.client, &self.inner.edf_url, today).await {
            Ok((quotas, used)) => {
                let ours = SeasonStock::new(season_start_year(today), history, quotas);
                let counted = [ours.blue.used, ours.white.used, ours.red.used];
                // EDF counts tomorrow from when it is published: one day of difference is no news
                if used.iter().zip(counted).any(|(edf, ours)| edf.abs_diff(ours) > 1) {
                    tracing::warn!(?used, ?counted, "EDF and RTE disagree on the days used");
                }
                quotas
            }
            Err(error) => {
                tracing::debug!(%error, "EDF's season unavailable: the usual quotas");
                previous
            }
        };
        state.quotas = Some((today, quotas));
        quotas
    }

    async fn tariffs(&self, state: &mut State, today: NaiveDate, now: DateTime<Utc>, force: bool) -> Option<Tariffs> {
        if force || Asked::due(state.tariffs_asked, now, TARIFFS_EVERY) {
            let fetched = tariffs::fetch(&self.inner.client, &self.inner.tariffs_url, today).await;
            state.tariffs_asked = Some(Asked { at: now, ok: fetched.is_ok() });
            match fetched {
                Ok(Some(t)) => state.tariffs = Some(t),
                Ok(None) => tracing::warn!("no current Tempo prices on data.gouv: keeping the last ones"),
                Err(error) => tracing::warn!(%error, "Tempo prices unavailable: keeping the last ones"),
            }
        }
        state.tariffs.clone()
    }

    fn netload<'a>(&self, state: &'a mut State) -> &'a mut NetLoad {
        let path = self.inner.dir.join("netload.json");
        state.netload.get_or_insert_with(|| {
            store::read_json(&path, Corrupt::Reset).unwrap_or_else(|error| {
                tracing::warn!(%error, "unreadable netload.json: starting empty");
                NetLoad::default()
            })
        })
    }

    async fn save_netload(&self, net: &NetLoad) {
        if let Err(error) = store::write_json_async(&self.inner.dir.join("netload.json"), net, Access::Shared).await {
            tracing::warn!(%error, "netload.json not saved");
        }
    }

    /// Yesterday's net consumption from ODRE, once the day is over (06:00 today).
    async fn refresh_load(&self, state: &mut State, today: NaiveDate, now: DateTime<Utc>) {
        let yesterday = today - Duration::days(1);
        let last = self.netload(state).load.keys().next_back().copied();
        if last >= Some(yesterday) || !Asked::due(state.load_asked, now, LOAD_EVERY) {
            return;
        }
        let since = last.map_or(today, |d| d + Duration::days(1)).max(today - Duration::days(LOAD_REACH_DAYS));
        let fetched = fetch_load(&self.inner.client, &self.inner.odre_url, LOAD_DATASET, since).await;
        state.load_asked = Some(Asked { at: now, ok: fetched.is_ok() });
        match fetched {
            Ok(load) => {
                let net = self.netload(state);
                net.merge(load, &Default::default(), today);
                self.save_netload(net).await;
            }
            Err(error) => tracing::warn!(%error, "ODRE unavailable: the normalisation keeps its past year"),
        }
    }

    /// Today's weather forecast every 3 hours; on failure the last one (`weather.json`),
    /// whose errors grow with its age.
    async fn refresh_weather(&self, state: &mut State, model: &Model, today: NaiveDate, now: DateTime<Utc>, force: bool) {
        let path = self.inner.dir.join("weather.json");
        if state.weather.is_none() {
            let saved: Weather = store::read_json(&path, Corrupt::Reset).unwrap_or_default();
            state.weather = saved.issued.is_some().then_some(saved);
        }
        let fresh = state.weather.as_ref().is_some_and(|w| w.issued == Some(today));
        // one from another day is asked again at once (a failure still waits RETRY)
        let every = if fresh { WEATHER_EVERY } else { Duration::zero() };
        if !force && !Asked::due(state.weather_asked, now, every) {
            return;
        }
        let fetched = fetch_weather(&self.inner.client, &self.inner.open_meteo_url, model, today).await;
        state.weather_asked = Some(Asked { at: now, ok: fetched.is_ok() });
        match fetched {
            Ok(weather) => {
                if let Err(error) = store::write_json_async(&path, &weather, Access::Shared).await {
                    tracing::warn!(%error, "weather.json not saved");
                }
                let net = self.netload(state);
                net.merge(Default::default(), &weather.temperature, today);
                self.save_netload(net).await;
                state.weather = Some(weather);
            }
            Err(error) => tracing::warn!(%error, "Open-Meteo unavailable: the last forecast, older"),
        }
    }
}

/// Keeps `answer` as the last one for `day`.
fn remember<T: Clone>(last: &Last<T>, day: NaiveDate, answer: &T) {
    *last.lock().unwrap_or_else(|p| p.into_inner()) = Some((day, answer.clone()));
}

impl Model {
    /// No model: only RTE's days.
    fn empty() -> Self {
        Self {
            version: String::new(),
            fitted_through: NaiveDate::MIN,
            cities: vec![],
            wind_sites: vec![],
            consumption: vec![],
            wind: vec![],
            solar: vec![],
            sigma: vec![],
            rho: 0.0,
            runs: 0,
            backtest: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tempo::{
        forecast::{tests::model, PastDay},
        rules::tests::{day, season},
    };

    fn levels() -> Levels {
        let days: Vec<PastDay> = (0..365)
            .map(|i| PastDay {
                consumption: 45_000.0 + 12_000.0 * (f64::from(i) / 58.0).sin(),
                wind: 5_000.0,
                solar: 2_000.0,
                temperature: Some(12.0),
            })
            .collect();
        Levels::from_days(&days).unwrap()
    }

    fn weather(issued: NaiveDate, temperature: f64, days: i64) -> Weather {
        Weather {
            issued: Some(issued),
            temperature: (-3..days).map(|i| (issued + Duration::days(i), temperature)).collect(),
            wind_power: (0..days).map(|i| (issued + Duration::days(i), 0.2)).collect(),
        }
    }

    /// The 2025-2026 season as RTE published it up to `last`.
    fn known_until(last: NaiveDate) -> History {
        season(2025).into_iter().filter(|(d, _)| *d <= last).collect()
    }

    #[test]
    fn the_week_is_official_then_forecast() {
        let today = day(2026, 1, 12);
        let history = known_until(today + Duration::days(1));
        let w = weather(today, -3.0, 9);
        let (days, note) = outlook(&model(), &history, Quotas::default(), Some(&w), Some(levels()), today);
        assert_eq!(note, None);
        assert_eq!(days.len(), 7);
        assert!(days[0].official && days[0].confidence == 1.0);
        assert_eq!(days[0].color, history[&day(2026, 1, 13)]);
        assert!(days[1..].iter().all(|d| !d.official));
        assert_eq!(days.iter().map(|d| d.horizon).collect::<Vec<_>>(), [1, 2, 3, 4, 5, 6, 7]);
        // very cold: the weekdays lean red, Sunday is blue for sure
        let sunday = days.iter().find(|d| d.date == day(2026, 1, 18)).unwrap();
        assert_eq!(sunday.probabilities.blue, 1.0);
        let same = outlook(&model(), &history, Quotas::default(), Some(&w), Some(levels()), today).0;
        assert_eq!(days, same, "seeded: the same answer all day");
    }

    #[test]
    fn without_weather_or_levels_only_rte_s_days() {
        let today = day(2026, 1, 12);
        let history = known_until(today + Duration::days(1));
        let (days, note) = outlook(&model(), &history, Quotas::default(), None, Some(levels()), today);
        assert_eq!(days.len(), 1);
        assert!(note.unwrap().contains("weather"));
        let (_, note) = outlook(&model(), &history, Quotas::default(), Some(&weather(today, 0.0, 9)), None, today);
        assert!(note.unwrap().contains("normalise"));
    }

    #[test]
    fn an_old_weather_forecast_is_used_with_wider_errors() {
        let today = day(2026, 1, 12);
        let history = known_until(today);
        // forecast three days ago, reaching J+5 only
        let old = weather(today - Duration::days(3), 2.0, 9);
        let (days, note) = outlook(&model(), &history, Quotas::default(), Some(&old), Some(levels()), today);
        assert_eq!(days.len(), 5);
        assert!(note.is_some());
        let fresh = weather(today, 2.0, 9);
        let (fresh_days, _) = outlook(&model(), &history, Quotas::default(), Some(&fresh), Some(levels()), today);
        let spread = |d: &ForecastDay| 1.0 - d.confidence;
        assert!(spread(&days[0]) >= spread(&fresh_days[0]), "J+1 seen from three days earlier is less sure");
    }

    #[test]
    fn the_stock_counts_published_days_only() {
        let history = known_until(day(2026, 1, 13));
        let stock = SeasonStock::new(2025, &history, Quotas::default());
        assert_eq!(stock.red, Count { used: 5, total: 22, remaining: 17 });
        assert_eq!(stock.blue.total, 300);
        assert_eq!(stock.blue.used + stock.white.used + stock.red.used, history.len() as u32);
    }

    /// A refresh holds the lock across the sources: a reader with today's answer gets it at
    /// once; one forcing, or with only yesterday's, waits for the refresh.
    #[tokio::test]
    async fn a_reader_gets_the_last_answer_while_a_refresh_runs() {
        let dir = crate::util::test_dir();
        let service = TempoService::from_config(&Config::defaults(dir.path().to_path_buf())).unwrap();
        let last: Last<u32> = Last::default();
        let wait = std::time::Duration::from_millis(50);
        assert!(matches!(service.turn(false, &last).await, Turn::Refresh(_)), "nobody refreshing: refresh");
        let refreshing = service.inner.state.lock().await;
        assert!(tokio::time::timeout(wait, service.turn(false, &last)).await.is_err(), "nothing kept: wait");
        remember(&last, house_today(), &7);
        assert!(matches!(service.turn(false, &last).await, Turn::Last(7)));
        assert!(tokio::time::timeout(wait, service.turn(true, &last)).await.is_err(), "forced: wait");
        remember(&last, house_today() - Duration::days(1), &7);
        assert!(tokio::time::timeout(wait, service.turn(false, &last)).await.is_err(), "yesterday's: wait");
        drop(refreshing);
        assert!(matches!(service.turn(false, &last).await, Turn::Refresh(_)));
    }
}
