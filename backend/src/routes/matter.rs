use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    auth::AuthenticatedUser,
    error::AppError,
    matter::{CoverCommand, CoverView},
    AppState,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CoversResponse {
    success: bool,
    covers: Vec<CoverView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CoverResponse {
    success: bool,
    cover: CoverView,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemovedResponse {
    success: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommissionRequest {
    /// Manual pairing code or `MT:` QR payload.
    code: String,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenameRequest {
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PositionRequest {
    /// 0 = closed, 100 = fully open.
    open_percent: u8,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/covers", get(list))
        .route("/commission", post(commission))
        .route("/covers/{id}", get(get_cover).patch(rename).delete(remove))
        .route("/covers/{id}/open", post(open))
        .route("/covers/{id}/close", post(close))
        .route("/covers/{id}/stop", post(stop))
        .route("/covers/{id}/position", post(position))
}

async fn list(State(state): State<AppState>, _user: AuthenticatedUser) -> Json<CoversResponse> {
    Json(CoversResponse {
        success: true,
        covers: state.matter.list().await,
    })
}

async fn get_cover(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<Json<CoverResponse>, AppError> {
    let cover = state.matter.get(&id).await?;
    Ok(Json(CoverResponse {
        success: true,
        cover,
    }))
}

async fn commission(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Json(body): Json<CommissionRequest>,
) -> Result<Json<CoverResponse>, AppError> {
    let cover = state.matter.commission(&body.code, &body.name).await?;
    Ok(Json(CoverResponse {
        success: true,
        cover,
    }))
}

async fn rename(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Path(id): Path<String>,
    Json(body): Json<RenameRequest>,
) -> Result<Json<CoverResponse>, AppError> {
    let cover = state.matter.rename(&id, &body.name).await?;
    Ok(Json(CoverResponse {
        success: true,
        cover,
    }))
}

async fn remove(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<Json<RemovedResponse>, AppError> {
    state.matter.remove(&id).await?;
    Ok(Json(RemovedResponse { success: true }))
}

async fn open(
    state: State<AppState>,
    user: AuthenticatedUser,
    id: Path<String>,
) -> Result<Json<CoverResponse>, AppError> {
    run(state, user, id, CoverCommand::Open).await
}

async fn close(
    state: State<AppState>,
    user: AuthenticatedUser,
    id: Path<String>,
) -> Result<Json<CoverResponse>, AppError> {
    run(state, user, id, CoverCommand::Close).await
}

async fn stop(
    state: State<AppState>,
    user: AuthenticatedUser,
    id: Path<String>,
) -> Result<Json<CoverResponse>, AppError> {
    run(state, user, id, CoverCommand::Stop).await
}

async fn position(
    state: State<AppState>,
    user: AuthenticatedUser,
    id: Path<String>,
    Json(body): Json<PositionRequest>,
) -> Result<Json<CoverResponse>, AppError> {
    if body.open_percent > 100 {
        return Err(AppError::http(
            axum::http::StatusCode::BAD_REQUEST,
            "openPercent must be between 0 and 100",
        ));
    }
    run(
        state,
        user,
        id,
        CoverCommand::OpenPercent(body.open_percent),
    )
    .await
}

async fn run(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Path(id): Path<String>,
    command: CoverCommand,
) -> Result<Json<CoverResponse>, AppError> {
    let cover = state.matter.command(&id, command).await?;
    Ok(Json(CoverResponse {
        success: true,
        cover,
    }))
}
