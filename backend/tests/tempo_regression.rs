//! Tempo, offline: every test runs on a temp copy of the repository's `cache/tempo/` and talks
//! to a local stub of RTE (open data and API), api-couleur-tempo, EDF, data.gouv, ODRE and
//! Open-Meteo, so `cargo test` neither needs the internet nor rewrites tracked files. The
//! stub answers around today's date, whatever it is.
mod common;

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use axum::{
    extract::{Path, Query, State},
    http::{Method, StatusCode},
    routing::{get, post},
    Json, Router,
};
use chrono::{Datelike, Duration, NaiveDate, Utc, Weekday};
use serde_json::{json, Value};

use maison_backend::{
    config::Config,
    tempo::{current_season, Color, TempoService},
    util::house_today,
};

/// What the stub was asked, and which upstreams are down.
#[derive(Clone, Default)]
struct Stub {
    calls: Arc<Mutex<Vec<String>>>,
    down: Arc<Mutex<HashSet<&'static str>>>,
}

impl Stub {
    /// Logs the call; `Err(503)` when `upstream` is down.
    fn ask(&self, upstream: &'static str, call: impl Into<String>) -> Result<(), StatusCode> {
        self.calls.lock().unwrap().push(call.into());
        if self.down.lock().unwrap().contains(upstream) {
            return Err(StatusCode::SERVICE_UNAVAILABLE);
        }
        Ok(())
    }

    fn count(&self, prefix: &str) -> usize {
        self.calls.lock().unwrap().iter().filter(|c| c.starts_with(prefix)).count()
    }

    fn down(&self, upstream: &'static str) {
        self.down.lock().unwrap().insert(upstream);
    }
}

type Answer = Result<Json<Value>, StatusCode>;

fn key(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// A plausible colour: red on some winter weekdays, white on some others, blue otherwise.
fn color(date: NaiveDate) -> &'static str {
    let weekday = !matches!(date.weekday(), Weekday::Sat | Weekday::Sun);
    match date.month() {
        12 | 1 | 2 if weekday && date.day().is_multiple_of(5) => "RED",
        11..=12 | 1..=3 if date.weekday() != Weekday::Sun && date.day().is_multiple_of(3) => "WHITE",
        _ => "BLUE",
    }
}

/// The season's days up to `last` (today, or tomorrow once published).
fn season_days(season: &str, last: NaiveDate) -> Vec<NaiveDate> {
    let start: i32 = season[..4].parse().unwrap();
    let first = NaiveDate::from_ymd_opt(start, 9, 1).unwrap();
    let end = NaiveDate::from_ymd_opt(start + 1, 8, 31).unwrap().min(last);
    first.iter_days().take_while(|d| *d <= end).collect()
}

async fn rte_season(State(stub): State<Stub>, Query(q): Query<HashMap<String, String>>) -> Answer {
    let season = q.get("season").cloned().unwrap_or_default();
    stub.ask("rte", format!("rte/tempo?season={season}"))?;
    let mut values: BTreeMap<String, &str> = season_days(&season, house_today()).into_iter().map(|d| (key(d), color(d))).collect();
    values.insert(format!("{}-fallback", key(house_today())), "false");
    Ok(Json(json!({ "values": values })))
}

async fn rte_light(State(stub): State<Stub>) -> Answer {
    stub.ask("rte", "rte/tempoLight")?;
    let today = house_today();
    Ok(Json(json!({ "values": { key(today): color(today), key(today + Duration::days(1)): "WHITE" } })))
}

async fn rte_token(State(stub): State<Stub>) -> Answer {
    stub.ask("rteapi", "rteapi/token")?;
    Ok(Json(json!({ "access_token": "t0k3n", "token_type": "Bearer", "expires_in": 7200 })))
}

async fn rte_calendars(State(stub): State<Stub>, Query(q): Query<HashMap<String, String>>) -> Answer {
    stub.ask("rteapi", format!("rteapi/calendars?start_date={}", q["start_date"]))?;
    let first: NaiveDate = q["start_date"][..10].parse().unwrap();
    let end: NaiveDate = q["end_date"][..10].parse().unwrap();
    let last = (end - Duration::days(1)).min(house_today() + Duration::days(1));
    // RTE's API answers newest first
    let mut values: Vec<Value> = first
        .iter_days()
        .take_while(|d| *d <= last)
        .map(|d| json!({ "start_date": format!("{}T00:00:00+01:00", key(d)), "value": if d > house_today() { "RED" } else { color(d) } }))
        .collect();
    values.reverse();
    Ok(Json(json!({ "tempo_like_calendars": [{ "start_date": q["start_date"], "values": values }] })))
}

fn code(color: &str) -> u8 {
    match color {
        "BLUE" => 1,
        "WHITE" => 2,
        _ => 3,
    }
}

async fn fallback_season(State(stub): State<Stub>, Query(q): Query<HashMap<String, String>>) -> Answer {
    let season = q["periode"].clone();
    stub.ask("fallback", format!("fallback/joursTempo?periode={season}"))?;
    let days: Vec<Value> =
        season_days(&season, house_today()).into_iter().map(|d| json!({ "dateJour": key(d), "codeJour": code(color(d)) })).collect();
    Ok(Json(json!(days)))
}

async fn fallback_tomorrow(State(stub): State<Stub>) -> Answer {
    stub.ask("fallback", "fallback/tomorrow")?;
    Ok(Json(json!({ "dateJour": key(house_today() + Duration::days(1)), "codeJour": 0, "libCouleur": "Inconnu" })))
}

async fn edf(State(stub): State<Stub>) -> Answer {
    stub.ask("edf", "edf")?;
    Ok(Json(json!({ "errors": [], "content": [
        { "typeJourEff": "TEMPO_ROUGE", "nombreJours": 20, "nombreJoursTires": 0 },
        { "typeJourEff": "TEMPO_BLANC", "nombreJours": 45, "nombreJoursTires": 0 },
        { "typeJourEff": "TEMPO_BLEU", "nombreJours": 300, "nombreJoursTires": 0 },
    ] })))
}

async fn tarifs(State(stub): State<Stub>) -> Answer {
    stub.ask("tarifs", "tarifs")?;
    Ok(Json(json!({ "data": [{
        "DATE_DEBUT": "2026-01-08", "DATE_FIN": null, "PART_FIXE_TTC": 189.98,
        "PART_VARIABLE_HCBleu_TTC": 0.1356, "PART_VARIABLE_HPBleu_TTC": 0.1654,
        "PART_VARIABLE_HCBlanc_TTC": 0.1536, "PART_VARIABLE_HPBlanc_TTC": 0.1921,
        "PART_VARIABLE_HCRouge_TTC": 0.1615, "PART_VARIABLE_HPRouge_TTC": 0.7295
    }] })))
}

/// éCO2mix every quarter of an hour from the asked time to now.
async fn odre(State(stub): State<Stub>, Path(dataset): Path<String>, Query(q): Query<HashMap<String, String>>) -> Answer {
    stub.ask("odre", format!("odre/{dataset}"))?;
    let since = q["where"].split('\'').nth(1).unwrap().parse::<chrono::DateTime<Utc>>().unwrap();
    let rows: Vec<Value> = std::iter::successors(Some(since), |t| Some(*t + Duration::minutes(15)))
        .take_while(|t| *t < Utc::now())
        .enumerate()
        .map(|(i, t)| {
            let wave = (i as f64 / 96.0 / 58.0).sin();
            json!({ "date_heure": t.to_rfc3339(), "consommation": 50_000.0 + 12_000.0 * wave, "eolien": 5_000.0, "solaire": 1_000.0 })
        })
        .collect();
    Ok(Json(json!(rows)))
}

/// Open-Meteo: 3 past days and 9 coming ones, hourly, cold and windless, for each place.
async fn meteo(State(stub): State<Stub>, Query(q): Query<HashMap<String, String>>) -> Answer {
    let variable = q["hourly"].clone();
    stub.ask("meteo", format!("meteo/{variable}"))?;
    let first = house_today() - Duration::days(3);
    let times: Vec<String> = (0..12 * 24)
        .map(|h| (first.and_hms_opt(0, 0, 0).unwrap() + Duration::hours(h)).format("%Y-%m-%dT%H:%M").to_string())
        .collect();
    let values = vec![2.0; times.len()];
    let places = q["latitude"].split(',').count();
    let mut hourly = json!({ "time": times });
    hourly[variable.as_str()] = json!(values);
    let place = json!({ "hourly": hourly });
    Ok(Json(json!(vec![place; places])))
}

/// The test config on a temp root holding a copy of the Tempo cache, every upstream on the
/// stub.
async fn stubbed(name: &str) -> (Config, Stub) {
    let mut config = common::isolated_config(name);
    let from = common::workspace_root().join("cache/tempo");
    let to = tempo_dir(&config);
    std::fs::create_dir_all(&to).unwrap();
    for entry in std::fs::read_dir(&from).unwrap() {
        let path = entry.unwrap().path();
        // the dev machine's own weather is not the stub's
        if path.extension().is_some_and(|e| e == "json") && !path.ends_with("weather.json") {
            std::fs::copy(&path, to.join(path.file_name().unwrap())).unwrap();
        }
    }

    let stub = Stub::default();
    let router = Router::new()
        .route("/rte/tempo", get(rte_season))
        .route("/rte/tempoLight", get(rte_light))
        .route("/rteapi/token/oauth/", post(rte_token))
        .route("/rteapi/open_api/tempo_like_supply_contract/v1/tempo_like_calendars", get(rte_calendars))
        .route("/fallback/api/joursTempo", get(fallback_season))
        .route("/fallback/api/jourTempo/tomorrow", get(fallback_tomorrow))
        .route("/edf/saisons/search", get(edf))
        .route("/tarifs", get(tarifs))
        .route("/odre/{dataset}/exports/json", get(odre))
        .route("/meteo/v1/forecast", get(meteo))
        .with_state(stub.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

    config.tempo_rte_url = format!("{base}/rte");
    config.tempo_rte_api_url = format!("{base}/rteapi");
    config.tempo_fallback_url = format!("{base}/fallback");
    config.tempo_edf_url = format!("{base}/edf");
    config.tempo_tarifs_url = format!("{base}/tarifs");
    config.tempo_odre_url = format!("{base}/odre");
    config.open_meteo_url = format!("{base}/meteo");
    (config, stub)
}

fn tempo_dir(config: &Config) -> PathBuf {
    config.source_root.join("cache/tempo")
}

fn service(config: &Config) -> TempoService {
    TempoService::from_config(config).expect("tempo service should build")
}

fn tomorrow() -> NaiveDate {
    house_today() + Duration::days(1)
}

#[tokio::test]
async fn today_and_tomorrow_come_from_rte_then_from_memory() {
    let (config, stub) = stubbed("maison-tempo-today").await;
    let tempo = service(&config);
    let today = tempo.today(false).await.unwrap();
    assert!(!today.cached);
    assert_eq!(today.today.date, house_today());
    assert_eq!(today.today.color, Color::parse(color(house_today())));
    assert_eq!(today.tomorrow.color, Some(Color::White));
    let yesterday = house_today() - Duration::days(1);
    assert_eq!((today.yesterday.date, today.yesterday.color), (yesterday, Color::parse(color(yesterday))));
    let tariffs = today.tariffs.unwrap();
    assert_eq!(tariffs.red.peak, 0.7295);
    assert_eq!(tariffs.starts_on.to_string(), "2026-08-01", "data.gouv's year-day-month");
    // EDF's quotas
    assert_eq!((today.stock.red.total, today.stock.white.total), (20, 45));
    assert_eq!(today.stock.red.used + today.stock.white.used + today.stock.blue.used, season_days(&current_season(house_today()), tomorrow()).len() as u32);
    tempo.today(false).await.unwrap();
    assert_eq!(stub.count("rte/tempoLight"), 1, "both colours known: the second read is from memory");
    assert_eq!(stub.count("tarifs"), 1);
    // the season's file, without RTE's extra keys
    let file = tempo_dir(&config).join(format!("tempo_history_{}.json", current_season(house_today())));
    let text = std::fs::read_to_string(file).unwrap();
    assert!(!text.contains("fallback") && text.contains(&key(tomorrow())));
}

#[tokio::test]
async fn rte_down_falls_back_to_api_couleur_tempo() {
    let (config, stub) = stubbed("maison-tempo-fallback").await;
    stub.down("rte");
    let today = service(&config).today(false).await.unwrap();
    assert!(stub.count("fallback/joursTempo") == 1 && stub.count("fallback/tomorrow") == 1);
    assert_eq!(today.today.color, Color::parse(color(house_today())));
    assert_eq!(today.tomorrow.color, None, "api-couleur-tempo does not know it yet");
    assert!(!today.cached);
}

#[tokio::test]
async fn rte_s_api_comes_first_when_configured() {
    let (mut config, stub) = stubbed("maison-tempo-official").await;
    config.rte_client_id = Some("id".into());
    config.rte_client_secret = Some("secret".into());
    let tempo = service(&config);
    let today = tempo.today(false).await.unwrap();
    assert_eq!(today.tomorrow.color, Some(Color::Red), "the API's tomorrow");
    tempo.today(true).await.unwrap();
    assert_eq!(stub.count("rteapi/calendars"), 2);
    assert_eq!(stub.count("rteapi/token"), 1, "the token lasts two hours");
    assert_eq!(stub.count("rte/"), 0);
    // the API down: RTE's open data
    stub.down("rteapi");
    assert_eq!(tempo.today(true).await.unwrap().tomorrow.color, Some(Color::White));
}

#[tokio::test]
async fn everything_down_is_the_season_s_file_said_cached() {
    let (config, stub) = stubbed("maison-tempo-offline").await;
    for upstream in ["rte", "fallback", "edf", "tarifs", "odre", "meteo"] {
        stub.down(upstream);
    }
    let file = tempo_dir(&config).join(format!("tempo_history_{}.json", current_season(house_today())));
    std::fs::write(&file, format!(r#"{{"values":{{"{}":"RED"}}}}"#, key(house_today()))).unwrap();
    let today = service(&config).today(false).await.unwrap();
    assert!(today.cached);
    assert_eq!(today.today.color, Some(Color::Red));
    assert_eq!(today.tariffs, None);
    assert_eq!((today.stock.red.total, today.stock.white.total), (22, 43), "the CRE's quotas");

    // and nothing on file: a plain error
    std::fs::remove_file(&file).unwrap();
    assert!(service(&config).today(false).await.is_err());
}

#[tokio::test]
async fn the_forecast_covers_the_week() {
    let (config, stub) = stubbed("maison-tempo-forecast").await;
    let tempo = service(&config);
    let forecast = tempo.forecast(false).await.unwrap();
    assert_eq!(forecast.note, None, "{forecast:?}");
    assert!(!forecast.stale);
    assert_eq!(forecast.days.len(), 7);
    assert!(forecast.days[0].official && forecast.days[0].color == Color::White);
    for (i, day) in forecast.days.iter().enumerate() {
        assert_eq!(day.date, house_today() + Duration::days(i as i64 + 1));
        let p = day.probabilities;
        assert!((p.blue + p.white + p.red - 1.0).abs() < 1e-9);
        assert_eq!(day.official, i == 0);
        assert_eq!(day.reliability.is_some(), i > 0, "the backtest's score for each forecast");
    }
    let backtest = forecast.model.unwrap().backtest;
    assert!(backtest.seasons.len() >= 2 && backtest.horizons.len() == 7);
    // ODRE and Open-Meteo asked once, their answers kept
    assert_eq!((stub.count("odre/eco2mix-national-tr"), stub.count("meteo/")), (1, 2));
    assert!(tempo_dir(&config).join("weather.json").exists());
    let again = tempo.forecast(false).await.unwrap();
    assert_eq!(again.days, forecast.days, "the same inputs, the same forecast");
    assert_eq!(stub.count("meteo/"), 2, "the weather is asked every 3 hours");
}

#[tokio::test]
async fn open_meteo_down_uses_the_last_forecast() {
    let (config, stub) = stubbed("maison-tempo-old-weather").await;
    // a forecast from two days ago, the cold it said then
    let issued = house_today() - Duration::days(2);
    let temperature: BTreeMap<String, f64> = (-3..8).map(|i| (key(issued + Duration::days(i)), 2.0)).collect();
    let wind: BTreeMap<String, f64> = (0..8).map(|i| (key(issued + Duration::days(i)), 0.1)).collect();
    std::fs::write(tempo_dir(&config).join("weather.json"), json!({ "issued": key(issued), "temperature": temperature, "wind_power": wind }).to_string()).unwrap();
    stub.down("meteo");
    let forecast = service(&config).forecast(false).await.unwrap();
    assert!(forecast.stale);
    assert_eq!(forecast.weather_issued, Some(issued));
    // it reaches 7 days after it was made: J+5 from today
    assert_eq!(forecast.days.last().unwrap().date, issued + Duration::days(7));
    assert!(forecast.note.is_some());
}

#[tokio::test]
async fn the_rabbit_gets_official_or_sure_colours() {
    let (config, _) = stubbed("maison-tempo-rabbit").await;
    let (today, tomorrow) = service(&config).rabbit_colors(false).await.unwrap();
    assert_eq!(Some(today), Color::parse(color(house_today())));
    assert_eq!(tomorrow, Some((Color::White, false)));
}

#[tokio::test]
async fn a_finished_season_is_read_from_its_file() {
    let (config, stub) = stubbed("maison-tempo-history").await;
    let calendar = service(&config).calendar(Some("2025-2026")).await.unwrap();
    assert!(calendar.calendar.iter().any(|d| d.date.to_string() == "2026-01-29" && d.color == Color::Red));
    assert!(calendar.calendar.iter().all(|d| d.is_actual));
    assert_eq!(calendar.stock.red.remaining, 0);
    assert_eq!(stub.count("rte/"), 0, "RTE not asked: {:?}", stub.calls.lock().unwrap());
}

#[tokio::test]
async fn the_calendar_and_today_agree_on_the_days_left() {
    let (config, _) = stubbed("maison-tempo-calendar").await;
    let tempo = service(&config);
    let calendar = tempo.calendar(None).await.unwrap();
    let today = tempo.today(false).await.unwrap();
    assert_eq!(calendar.stock, today.stock, "forecast days are not counted as used");
    let predicted = calendar.calendar.iter().filter(|d| d.is_prediction).count();
    assert_eq!(predicted, 6);
    assert!(calendar.calendar.windows(2).all(|w| w[0].date < w[1].date));
}

#[tokio::test]
async fn the_routes_answer_members_and_check_seasons() {
    let (config, _) = stubbed("maison-tempo-routes").await;
    let app = common::app(config);
    let member = common::member_token();
    let (status, body) = common::send(&app, Method::GET, "/api/tempo", Some(&member), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["tomorrow"]["color"], "WHITE");
    assert_eq!(body["tariffs"]["startsOn"], "2026-08-01");
    assert_eq!(body["tariffs"]["red"]["peak"], 0.7295);
    assert_eq!(body["tariffs"]["red"]["offPeak"], 0.1615);
    assert_eq!(body["stock"]["red"]["total"], 20);
    assert_eq!((body["hours"]["peakStart"].as_str(), body["hours"]["peakEnd"].as_str()), (Some("06:00"), Some("22:00")));
    assert!(body["yesterday"]["date"].is_string());

    let (status, body) = common::send(&app, Method::GET, "/api/tempo/forecast", Some(&member), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["days"].as_array().unwrap().len(), 7);
    assert!(body["days"][1]["probabilities"]["RED"].is_number());
    assert!(body["days"][1]["reliability"]["accuracy"].is_number());

    let (status, body) = common::send(&app, Method::POST, "/api/tempo/refresh", Some(&member), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, _) = common::send(&app, Method::GET, "/api/tempo", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    for path in ["/api/tempo/calendar?season=0000-9999", "/api/tempo/calendar?season=2013-2014", "/api/tempo/calendar?season=../x"] {
        let (status, body) = common::send(&app, Method::GET, path, Some(&member), None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}: {body}");
    }
    let (status, body) = common::send(&app, Method::GET, "/api/tempo/calendar?season=2024-2025", Some(&member), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["stock"]["red"]["used"], 22);
    assert_eq!(body["calendar"][0]["date"], "2024-09-01");
}
