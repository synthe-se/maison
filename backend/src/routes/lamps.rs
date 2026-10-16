//! The routes every lamp family answers the same way (list, stats, one lamp, power,
//! brightness, temperature, rename), said once over a [`LampBackend`]. The URL paths and
//! JSON shapes are the web app's contract: each family mounts this router where its
//! routes always were.

use std::future::Future;

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use maison_api::{
    lamps::{BrightnessRequest, LampList, PowerRequest, RenameRequest, TemperatureRequest},
    Message,
};
use serde::Serialize;

use crate::{
    error::AppError,
    lamps::{self, Lamp, LampState, LampStats},
    routes::{Answer, Reply},
    AppState,
};

/// A lamp family, as the shared routes drive it.
pub trait LampBackend: Send + Sync + 'static {
    /// « Hue lamp », « Zigbee lamp »: the start of every message.
    const NAME: &'static str;
    type View: Serialize + Lamp + Send;

    fn of(state: &AppState) -> &Self;
    fn list(&self) -> impl Future<Output = Vec<Self::View>> + Send;
    fn get(&self, id: &str) -> impl Future<Output = Option<Self::View>> + Send;
    fn stats(&self) -> impl Future<Output = LampStats> + Send;
    /// The list and its counts, for `GET /`: a family that syncs before answering does it
    /// once for both.
    fn listing(&self) -> impl Future<Output = (Vec<Self::View>, LampStats)> + Send {
        async { (self.list().await, self.stats().await) }
    }
    fn set_power(
        &self,
        id: &str,
        on: bool,
    ) -> impl Future<Output = Result<LampState, AppError>> + Send;
    /// `brightness` as [`lamps::brightness`] takes it (1–100, 0 the dimmest).
    fn set_brightness(
        &self,
        id: &str,
        brightness: u8,
    ) -> impl Future<Output = Result<LampState, AppError>> + Send;
    fn set_temperature(
        &self,
        id: &str,
        temperature: u8,
    ) -> impl Future<Output = Result<LampState, AppError>> + Send;
    fn rename(&self, id: &str, name: &str) -> impl Future<Output = Result<(), AppError>> + Send;
}

/// One lamp (`GET /{id}`).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OneLamp<V> {
    lamp: V,
    message: String,
}

/// The answer of a command that changed a lamp: its new state.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Changed {
    state: LampState,
    message: String,
}

/// `{success, state, message}`.
pub fn changed(state: LampState, message: impl Into<String>) -> Json<Answer<Changed>> {
    Answer::ok(Changed {
        state,
        message: message.into(),
    })
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

async fn list<B: LampBackend>(State(state): State<AppState>) -> Json<Answer<LampList<B::View>>> {
    let (lamps, stats) = B::of(&state).listing().await;
    Answer::ok(LampList {
        lamps,
        total: stats.total,
        connected: stats.connected,
        reachable: stats.reachable,
        message: format!("{}s list retrieved", B::NAME),
    })
}

async fn stats<B: LampBackend>(State(state): State<AppState>) -> Json<Answer<LampStats>> {
    Answer::ok(B::of(&state).stats().await)
}

/// One lamp; an unknown id is the coded `not_found`.
async fn lamp<B: LampBackend>(
    State(state): State<AppState>,
    Path(lamp_id): Path<String>,
) -> Result<Json<Answer<OneLamp<B::View>>>, AppError> {
    let lamp = B::of(&state)
        .get(&lamp_id)
        .await
        .ok_or_else(lamps::not_found)?;
    Ok(Answer::ok(OneLamp {
        lamp,
        message: format!("{} retrieved", B::NAME),
    }))
}

async fn power<B: LampBackend>(
    State(state): State<AppState>,
    Path(lamp_id): Path<String>,
    Json(body): Json<PowerRequest>,
) -> Result<Json<Answer<Changed>>, AppError> {
    let lamp_state = B::of(&state).set_power(&lamp_id, body.enabled).await?;
    Ok(changed(lamp_state, format!("{} power updated", B::NAME)))
}

async fn brightness<B: LampBackend>(
    State(state): State<AppState>,
    Path(lamp_id): Path<String>,
    Json(body): Json<BrightnessRequest>,
) -> Result<Json<Answer<Changed>>, AppError> {
    let lamp_state = B::of(&state)
        .set_brightness(&lamp_id, body.brightness)
        .await?;
    Ok(changed(
        lamp_state,
        format!("{} brightness updated", B::NAME),
    ))
}

async fn temperature<B: LampBackend>(
    State(state): State<AppState>,
    Path(lamp_id): Path<String>,
    Json(body): Json<TemperatureRequest>,
) -> Result<Json<Answer<Changed>>, AppError> {
    let lamp_state = B::of(&state)
        .set_temperature(&lamp_id, body.temperature)
        .await?;
    Ok(changed(
        lamp_state,
        format!("{} temperature updated", B::NAME),
    ))
}

async fn rename<B: LampBackend>(
    State(state): State<AppState>,
    Path(lamp_id): Path<String>,
    Json(body): Json<RenameRequest>,
) -> Result<Json<Answer<Message>>, AppError> {
    B::of(&state).rename(&lamp_id, &body.name).await?;
    Ok(Json(Answer::said(format!("{} renamed", B::NAME))))
}
