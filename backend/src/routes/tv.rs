use axum::{
    Json, Router,
    extract::State,
    routing::{get, post, put},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    AppState,
    auth::AdminUser,
    error::AppError,
    ir::SwitchState,
    routes::{Answer, SimpleResponse},
    tv::{self, TvConfig, TvKey, TvStatus},
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    config: TvConfig,
    status: TvStatus,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PowerRequest {
    #[serde(default)]
    state: SwitchState,
    /// Also route the set to the box's HDMI input, which is almost always what
    /// is wanted: the TV otherwise comes back on whatever source it was left on.
    #[serde(default = "crate::util::default_true")]
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

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(status))
        .route("/config", put(set_config))
        .route("/power", post(power))
        .route("/key", post(send_key))
        .route("/volume", put(set_volume))
        .route("/ambilight", post(set_ambilight))
        .route("/ambilight/styles", get(ambilight_styles))
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
) -> Result<Json<SimpleResponse>, AppError> {
    state.tv.set_config(body).await?;
    Ok(SimpleResponse::ok("TV configuration saved"))
}

async fn power(
    State(state): State<AppState>,
    Json(body): Json<PowerRequest>,
) -> Result<Json<Answer<Value>>, AppError> {
    let power = tv::tv_power(&state, body.state, body.switch_to_box).await?;
    Ok(Answer::ok(json!({ "power": power })))
}

async fn send_key(
    State(state): State<AppState>,
    Json(body): Json<KeyRequest>,
) -> Result<Json<SimpleResponse>, AppError> {
    state.tv.send_key(body.key).await?;
    Ok(SimpleResponse::ok("Key sent"))
}

async fn set_volume(
    State(state): State<AppState>,
    Json(body): Json<VolumeRequest>,
) -> Result<Json<Answer<Value>>, AppError> {
    let volume = match body.level {
        Some(level) => state.tv.set_volume(level, body.muted).await?,
        // A mute-only request still has to go through the absolute-volume
        // write, so read the current level and keep it.
        None => {
            let current = state.tv.volume().await?;
            match body.muted {
                Some(muted) => state.tv.set_volume(current.current, Some(muted)).await?,
                None => current,
            }
        }
    };
    Ok(Answer::ok(json!({ "volume": volume })))
}

async fn set_ambilight(
    State(state): State<AppState>,
    Json(body): Json<AmbilightRequest>,
) -> Result<Json<Answer<Value>>, AppError> {
    let on = body.state.resolve(|| async { Ok(state.tv.ambilight().await?.power) }).await?;
    state.tv.set_ambilight_power(on).await?;
    Ok(Answer::ok(json!({ "ambilight": state.tv.ambilight().await? })))
}

async fn ambilight_styles(State(state): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
    state.tv.ambilight_styles().await.map(Json)
}

async fn switch_to_box(State(state): State<AppState>) -> Result<Json<SimpleResponse>, AppError> {
    // CEC first: the DIAL fallback inside `route_to_box` wakes the box by
    // launching an app, which would interrupt whatever is playing.
    tv::route_to_box(&state).await?;
    Ok(SimpleResponse::ok("TV routed to the box's HDMI input"))
}
