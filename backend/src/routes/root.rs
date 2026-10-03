use axum::{routing::get, Json, Router};
use serde::Serialize;

use crate::{routes::Answer, AppState};

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
    Json(RootResponse { message: "Home API", version: env!("CARGO_PKG_VERSION"), description: "Maison backend" })
}

async fn health_handler() -> Json<Answer<Health>> {
    Answer::ok(Health { status: "healthy", service: "maison-backend" })
}
