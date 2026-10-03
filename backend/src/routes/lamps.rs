//! The routes every lamp family answers the same way (list, stats, one lamp, power,
//! brightness, temperature, rename), said once over a [`LampBackend`]. The URL paths and
//! JSON shapes are the web app's contract: each family mounts this router where its
//! routes always were.

use std::future::Future;

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    error::AppError,
    lamps::{LampState, LampStats},
    routes::SimpleResponse,
};

/// A lamp family, as the shared routes drive it.
pub trait LampBackend: Send + Sync + 'static {
    /// « Hue lamp », « Zigbee lamp »: the start of every message.
    const NAME: &'static str;
    type View: Serialize + Send;

    fn of(state: &AppState) -> &Self;
    fn list(&self) -> impl Future<Output = Vec<Self::View>> + Send;
    fn get(&self, id: &str) -> impl Future<Output = Option<Self::View>> + Send;
    fn stats(&self) -> impl Future<Output = LampStats> + Send;
    fn set_power(&self, id: &str, on: bool) -> impl Future<Output = Result<LampState, AppError>> + Send;
    fn set_brightness(&self, id: &str, brightness: u8) -> impl Future<Output = Result<LampState, AppError>> + Send;
    fn set_temperature(&self, id: &str, temperature: u8) -> impl Future<Output = Result<LampState, AppError>> + Send;
    fn rename(&self, id: &str, name: &str) -> impl Future<Output = Result<(), AppError>> + Send;
}

#[derive(Debug, Serialize)]
struct ListResponse<V> {
    success: bool,
    lamps: Vec<V>,
    total: usize,
    connected: usize,
    reachable: usize,
    message: String,
}

#[derive(Debug, Serialize)]
struct StatsResponse {
    success: bool,
    #[serde(flatten)]
    stats: LampStats,
}

#[derive(Debug, Serialize)]
struct LampResponse<V> {
    success: bool,
    lamp: Option<V>,
    message: String,
}

/// The answer of a command that changed a lamp: its new state.
#[derive(Debug, Serialize)]
pub struct ActionResponse {
    success: bool,
    state: LampState,
    message: String,
}

impl ActionResponse {
    pub fn ok(state: LampState, message: String) -> Json<Self> {
        Json(Self { success: true, state, message })
    }
}

#[derive(Debug, Deserialize)]
struct PowerBody {
    enabled: bool,
}

#[derive(Debug, Deserialize)]
struct BrightnessBody {
    brightness: u8,
}

#[derive(Debug, Deserialize)]
struct TemperatureBody {
    temperature: u8,
}

#[derive(Debug, Deserialize)]
struct RenameBody {
    name: String,
}

/// `/`, `/stats`, `/{lamp_id}` and its `power`, `brightness`, `temperature`, `rename`.
pub fn router<B: LampBackend>() -> Router<AppState> {
    Router::new()
        .route("/", get(list::<B>))
        .route("/stats", get(stats::<B>))
        .route("/{lamp_id}", get(lamp::<B>))
        .route("/{lamp_id}/power", post(power::<B>))
        .route("/{lamp_id}/brightness", post(brightness::<B>))
        .route("/{lamp_id}/temperature", post(temperature::<B>))
        .route("/{lamp_id}/rename", post(rename::<B>))
}

async fn list<B: LampBackend>(State(state): State<AppState>) -> Json<ListResponse<B::View>> {
    let backend = B::of(&state);
    let lamps = backend.list().await;
    let stats = backend.stats().await;
    Json(ListResponse {
        success: true,
        lamps,
        total: stats.total,
        connected: stats.connected,
        reachable: stats.reachable,
        message: format!("{}s list retrieved", B::NAME),
    })
}

async fn stats<B: LampBackend>(State(state): State<AppState>) -> Json<StatsResponse> {
    Json(StatsResponse { success: true, stats: B::of(&state).stats().await })
}

async fn lamp<B: LampBackend>(State(state): State<AppState>, Path(lamp_id): Path<String>) -> Json<LampResponse<B::View>> {
    let lamp = B::of(&state).get(&lamp_id).await;
    let message = format!("{} {}", B::NAME, if lamp.is_some() { "retrieved" } else { "not found" });
    Json(LampResponse { success: lamp.is_some(), lamp, message })
}

async fn power<B: LampBackend>(
    State(state): State<AppState>,
    Path(lamp_id): Path<String>,
    Json(body): Json<PowerBody>,
) -> Result<Json<ActionResponse>, AppError> {
    let lamp_state = B::of(&state).set_power(&lamp_id, body.enabled).await?;
    Ok(ActionResponse::ok(lamp_state, format!("{} power updated", B::NAME)))
}

async fn brightness<B: LampBackend>(
    State(state): State<AppState>,
    Path(lamp_id): Path<String>,
    Json(body): Json<BrightnessBody>,
) -> Result<Json<ActionResponse>, AppError> {
    let lamp_state = B::of(&state).set_brightness(&lamp_id, body.brightness).await?;
    Ok(ActionResponse::ok(lamp_state, format!("{} brightness updated", B::NAME)))
}

async fn temperature<B: LampBackend>(
    State(state): State<AppState>,
    Path(lamp_id): Path<String>,
    Json(body): Json<TemperatureBody>,
) -> Result<Json<ActionResponse>, AppError> {
    let lamp_state = B::of(&state).set_temperature(&lamp_id, body.temperature).await?;
    Ok(ActionResponse::ok(lamp_state, format!("{} temperature updated", B::NAME)))
}

async fn rename<B: LampBackend>(
    State(state): State<AppState>,
    Path(lamp_id): Path<String>,
    Json(body): Json<RenameBody>,
) -> Result<Json<SimpleResponse>, AppError> {
    B::of(&state).rename(&lamp_id, &body.name).await?;
    Ok(SimpleResponse::ok(format!("{} renamed", B::NAME)))
}
