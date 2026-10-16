use axum::{
    extract::State,
    routing::{get, post, put},
    Json, Router,
};
use maison_api::Message;
use serde::{Deserialize, Serialize};

use crate::{
    auth::AdminUser,
    error::AppError,
    ir::{Resolve, SwitchState},
    routes::{Answer, Reply},
    tv::{self, TvAmbilight, TvConfig, TvKey, TvPower, TvStatus, TvVolume},
    AppState,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    config: TvConfig,
    status: TvStatus,
}

/// The set powered as `IrAction::TvPower` powers it, with the same defaults.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PowerRequest {
    #[serde(default)]
    state: SwitchState,
    /// Also route the set to the box's HDMI input, which is almost always what
    /// is wanted: the TV otherwise comes back on whatever source it was left on.
    #[serde(default = "maison_api::remote::default_true")]
    switch_to_box: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeyRequest {
    key: TvKey,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VolumeRequest {
    level: Option<u8>,
    muted: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AmbilightRequest {
    #[serde(default)]
    state: SwitchState,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Powered {
    power: TvPower,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Volume {
    volume: TvVolume,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Ambilight {
    ambilight: TvAmbilight,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(status))
        .route("/config", put(set_config))
        .route("/power", post(power))
        .route("/key", post(send_key))
        .route("/volume", put(set_volume))
        .route("/ambilight", post(set_ambilight))
        .route("/source/box", post(switch_to_box))
}

async fn status(State(state): State<AppState>) -> Json<Answer<Status>> {
    Answer::ok(Status {
        config: state.tv.config().await,
        status: state.tv.status().await,
    })
}

async fn set_config(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<TvConfig>,
) -> Result<Json<Answer<Message>>, AppError> {
    state.tv.set_config(body).await?;
    Ok(Json(Answer::said("TV configuration saved")))
}

async fn power(
    State(state): State<AppState>,
    Json(body): Json<PowerRequest>,
) -> Result<Json<Answer<Powered>>, AppError> {
    let power = tv::tv_power(&state, body.state, body.switch_to_box).await?;
    Ok(Answer::ok(Powered { power }))
}

async fn send_key(
    State(state): State<AppState>,
    Json(body): Json<KeyRequest>,
) -> Result<Json<Answer<Message>>, AppError> {
    state.tv.send_key(body.key).await?;
    Ok(Json(Answer::said("Key sent")))
}

async fn set_volume(
    State(state): State<AppState>,
    Json(body): Json<VolumeRequest>,
) -> Result<Json<Answer<Volume>>, AppError> {
    let volume = state.tv.set_volume(body.level, body.muted).await?;
    Ok(Answer::ok(Volume { volume }))
}

async fn set_ambilight(
    State(state): State<AppState>,
    Json(body): Json<AmbilightRequest>,
) -> Result<Json<Answer<Ambilight>>, AppError> {
    let on = body
        .state
        .resolve(|| async { Ok(state.tv.ambilight().await?.power) })
        .await?;
    state.tv.set_ambilight_power(on).await?;
    Ok(Answer::ok(Ambilight {
        ambilight: state.tv.ambilight().await?,
    }))
}

async fn switch_to_box(State(state): State<AppState>) -> Result<Json<Answer<Message>>, AppError> {
    // CEC first: the DIAL fallback inside `route_to_box` wakes the box by
    // launching an app, which would interrupt whatever is playing.
    tv::route_to_box(&state).await?;
    Ok(Json(Answer::said("TV routed to the box's HDMI input")))
}
