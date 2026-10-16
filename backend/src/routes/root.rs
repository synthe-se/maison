use axum::{extract::State, routing::get, Json, Router};
use serde::Serialize;

use maison_api::sun::SunNow;

use crate::{
    routes::{matter::sun_now, Answer, Reply},
    AppState,
};

#[derive(Debug, Serialize)]
struct RootResponse {
    message: &'static str,
    version: &'static str,
    description: &'static str,
}

#[derive(Debug, Serialize)]
struct Health {
    status: &'static str,
    service: &'static str,
}

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/", get(root_handler))
        .route("/health", get(health_handler))
        .route("/sun", get(sun_handler))
}

pub fn health_router() -> Router<AppState> {
    Router::new().route("/health", get(health_handler))
}

/// At the root when no web app is built: what the API offers (health is merged separately).
pub fn router() -> Router<AppState> {
    Router::new().route("/", get(root_handler))
}

/// Who answers, nothing more: the routes themselves are the list (`lib.rs`), and a list here
/// would drift (it once still offered a password login).
async fn root_handler() -> Json<RootResponse> {
    Json(RootResponse {
        message: "Home API",
        version: env!("CARGO_PKG_VERSION"),
        description: "Maison backend",
    })
}

async fn health_handler() -> Json<Answer<Health>> {
    Answer::ok(Health {
        status: "healthy",
        service: "maison-backend",
    })
}

/// Signed out, a page asks whether it is day or night at the house, for its theme (a wall screen
/// waiting to be paired): the side only, never the place nor the hours (the house's place and its
/// sun times stay with those signed in, `GET /api/matter/place`).
async fn sun_handler(State(state): State<AppState>) -> Json<Answer<SunNow>> {
    Answer::ok(SunNow {
        side: sun_now(&state).await.and_then(|sun| sun.side),
    })
}
