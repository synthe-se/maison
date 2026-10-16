use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use maison_api::tempo::{Forecast, Today};
use serde::Deserialize;

use crate::{
    error::AppError,
    routes::{Answer, Reply},
    tempo::Calendar,
    AppState,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
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

async fn get_today(State(state): State<AppState>) -> Result<Json<Answer<Today>>, AppError> {
    Ok(Answer::ok(state.tempo.today(false).await?))
}

/// Asks every source again (harmless: members may), once a minute at most (the coded
/// `too_many_attempts` else: the sources are not ours).
async fn refresh(State(state): State<AppState>) -> Result<Json<Answer<Today>>, AppError> {
    state.tempo.may_force()?;
    let today = state.tempo.today(true).await?;
    state.tempo.forecast(true).await?;
    Ok(Answer::ok(today))
}

async fn get_forecast(State(state): State<AppState>) -> Result<Json<Answer<Forecast>>, AppError> {
    Ok(Answer::ok(state.tempo.forecast(false).await?))
}

/// A season's days; a season that is not one is the coded `invalid`.
async fn get_calendar(
    State(state): State<AppState>,
    Query(query): Query<SeasonQuery>,
) -> Result<Json<Answer<Calendar>>, AppError> {
    Ok(Answer::ok(
        state.tempo.calendar(query.season.as_deref()).await?,
    ))
}
