use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

use crate::{
    error::AppError,
    routes::Answer,
    tempo::{Calendar, Forecast, Today},
    AppState,
};

#[derive(Deserialize)]
struct SeasonQuery {
    season: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_today))
        .route("/refresh", post(refresh))
        .route("/forecast", get(get_forecast))
        .route("/calendar", get(get_calendar))
}

/// The sources down and nothing on file: the cause goes to the log, the client gets a
/// plain 503.
fn unavailable(error: AppError) -> AppError {
    tracing::warn!(%error, "tempo request failed");
    AppError::service_unavailable("Tempo service temporarily unavailable")
}

async fn get_today(State(state): State<AppState>) -> Result<Json<Answer<Today>>, AppError> {
    Ok(Answer::ok(state.tempo.today(false).await.map_err(unavailable)?))
}

/// Asks every source again (harmless: members may).
async fn refresh(State(state): State<AppState>) -> Result<Json<Answer<Today>>, AppError> {
    let today = state.tempo.today(true).await.map_err(unavailable)?;
    state.tempo.forecast(true).await.map_err(unavailable)?;
    Ok(Answer::ok(today))
}

async fn get_forecast(State(state): State<AppState>) -> Result<Json<Answer<Forecast>>, AppError> {
    Ok(Answer::ok(state.tempo.forecast(false).await.map_err(unavailable)?))
}

async fn get_calendar(
    State(state): State<AppState>,
    Query(query): Query<SeasonQuery>,
) -> Result<Json<Answer<Calendar>>, AppError> {
    // a bad season is the asker's error, not the sources'
    Ok(Answer::ok(state.tempo.calendar(query.season.as_deref()).await?))
}
