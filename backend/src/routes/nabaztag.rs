use axum::{
    extract::State,
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    auth::AdminUser,
    error::AppError,
    nabaztag::{NabaztagConfig, TempoPushResult},
    routes::{Answer, Reply, SimpleResponse},
    AppState,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NabaztagStatus {
    config: NabaztagConfig,
    reachable: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandRequest {
    command: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct TempoPushRequest {
    #[serde(default)]
    force_refresh: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TempoPushed {
    message: String,
    result: TempoPushResult,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(status))
        .route("/config", put(set_config))
        .route("/ctl", post(send_command))
        .route("/tempo/push", post(push_tempo))
}

async fn status(State(state): State<AppState>) -> Json<Answer<NabaztagStatus>> {
    Answer::ok(NabaztagStatus {
        config: state.nabaztag.config().await,
        reachable: state.nabaztag.reachable().await,
    })
}

async fn set_config(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<NabaztagConfig>,
) -> Result<Json<SimpleResponse>, AppError> {
    state.nabaztag.set_config(body).await?;
    Ok(SimpleResponse::ok("Nabaztag configuration saved"))
}

async fn send_command(
    State(state): State<AppState>,
    client: crate::auth::DeviceClient,
    Json(body): Json<CommandRequest>,
) -> Result<Json<SimpleResponse>, AppError> {
    // rebooting the rabbit is setting it up, not for a wall screen
    if body.command.split_whitespace().next() == Some("reboot") {
        client.not_a_screen()?;
    }
    state.nabaztag.send_command(&body.command).await?;
    Ok(SimpleResponse::ok(format!(
        "Command sent: {}",
        body.command.trim()
    )))
}

async fn push_tempo(
    State(state): State<AppState>,
    body: Option<Json<TempoPushRequest>>,
) -> Result<Json<Answer<TempoPushed>>, AppError> {
    let force_refresh = body.map(|b| b.force_refresh).unwrap_or(false);
    let result = state
        .nabaztag
        .push_tempo_from(&state.tempo, force_refresh)
        .await?;
    Ok(Answer::ok(TempoPushed {
        message: format!(
            "Tempo pushed: today={}, tomorrow={}",
            result.today_color.as_str(),
            result.tomorrow_color.map_or("unknown", |c| c.as_str())
        ),
        result,
    }))
}
