use axum::{
    Json, Router,
    extract::{Path, State},
    routing::post,
};

use crate::{
    AppState,
    auth::AdminUser,
    error::AppError,
    hue::HueManager,
    lamps::{HueLampView, LampState, LampStats},
    routes::{
        SimpleResponse,
        lamps::{self, LampBackend},
    },
};

impl LampBackend for HueManager {
    const NAME: &'static str = "Hue lamp";
    type View = HueLampView;

    fn of(state: &AppState) -> &Self {
        &state.hue
    }
    async fn list(&self) -> Vec<HueLampView> {
        self.list_lamps().await
    }
    async fn get(&self, id: &str) -> Option<HueLampView> {
        self.get_lamp(id).await
    }
    async fn stats(&self) -> LampStats {
        HueManager::stats(self).await
    }
    async fn set_power(&self, id: &str, on: bool) -> Result<LampState, AppError> {
        HueManager::set_power(self, id, on).await
    }
    async fn set_brightness(&self, id: &str, brightness: u8) -> Result<LampState, AppError> {
        HueManager::set_brightness(self, id, brightness).await
    }
    async fn set_temperature(&self, id: &str, temperature: u8) -> Result<LampState, AppError> {
        HueManager::set_temperature(self, id, temperature).await
    }
    async fn rename(&self, id: &str, name: &str) -> Result<(), AppError> {
        self.rename_lamp(id, name).await
    }
}

/// The shared lamp routes, plus Bluetooth's own (admin): scanning and blacklisting.
/// Lamps connect by themselves when a scan sees them: there is no connect route.
pub fn router() -> Router<AppState> {
    lamps::router::<HueManager>()
        .route("/scan", post(scan))
        .route("/{lamp_id}/blacklist", post(blacklist_lamp))
}

async fn scan(State(state): State<AppState>, _admin: AdminUser) -> Result<Json<SimpleResponse>, AppError> {
    state.hue.trigger_scan().await?;
    Ok(SimpleResponse::ok("Hue lamp scan started"))
}

async fn blacklist_lamp(
    State(state): State<AppState>,
    Path(lamp_id): Path<String>,
    _admin: AdminUser,
) -> Result<Json<SimpleResponse>, AppError> {
    if state.hue.blacklist_lamp(&lamp_id).await? {
        Ok(SimpleResponse::ok("Hue lamp blacklisted"))
    } else {
        Err(AppError::not_found("Hue lamp not found"))
    }
}
