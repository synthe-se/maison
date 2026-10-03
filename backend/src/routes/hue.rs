use axum::{
    Json, Router,
    extract::{Path, State},
    routing::post,
};
use serde::Deserialize;

use crate::{
    AppState,
    auth::AdminUser,
    error::AppError,
    hue::HueManager,
    lamps::{HueLampView, LampState, LampStats},
    routes::{
        SimpleResponse,
        lamps::{self, ActionResponse, LampBackend},
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

#[derive(Debug, Deserialize)]
struct StateBody {
    #[serde(rename = "isOn")]
    is_on: bool,
    brightness: Option<u8>,
}

/// The shared lamp routes, plus Bluetooth's own: scanning and blacklisting (admin),
/// connecting, and power with brightness in one write.
pub fn router() -> Router<AppState> {
    lamps::router::<HueManager>()
        .route("/scan", post(scan))
        .route("/connect", post(connect_all))
        .route("/disconnect", post(disconnect_all))
        .route("/{lamp_id}/connect", post(connect_lamp))
        .route("/{lamp_id}/disconnect", post(disconnect_lamp))
        .route("/{lamp_id}/state", post(set_state))
        .route("/{lamp_id}/blacklist", post(blacklist_lamp))
}

async fn scan(State(state): State<AppState>, _admin: AdminUser) -> Result<Json<SimpleResponse>, AppError> {
    state.hue.trigger_scan().await?;
    Ok(SimpleResponse::ok("Hue lamp scan started"))
}

async fn connect_all(State(state): State<AppState>) -> Json<SimpleResponse> {
    state.hue.connect_all().await;
    SimpleResponse::ok("Hue lamp connections started")
}

async fn disconnect_all(State(state): State<AppState>) -> Json<SimpleResponse> {
    state.hue.disconnect_all().await;
    SimpleResponse::ok("Hue lamps disconnected")
}

async fn connect_lamp(
    State(state): State<AppState>,
    Path(lamp_id): Path<String>,
) -> Result<Json<SimpleResponse>, AppError> {
    let connected = state.hue.connect_lamp(&lamp_id).await?;
    Ok(Json(SimpleResponse {
        success: connected,
        message: if connected { "Hue lamp connected" } else { "Hue lamp connection unavailable" }.to_string(),
    }))
}

async fn disconnect_lamp(
    State(state): State<AppState>,
    Path(lamp_id): Path<String>,
) -> Result<Json<SimpleResponse>, AppError> {
    state.hue.disconnect_lamp(&lamp_id).await?;
    Ok(SimpleResponse::ok("Hue lamp disconnected"))
}

async fn set_state(
    State(state): State<AppState>,
    Path(lamp_id): Path<String>,
    Json(body): Json<StateBody>,
) -> Result<Json<ActionResponse>, AppError> {
    let lamp_state = state.hue.set_lamp_state(&lamp_id, body.is_on, body.brightness).await?;
    Ok(ActionResponse::ok(lamp_state, "Hue lamp state updated".to_string()))
}

async fn blacklist_lamp(
    State(state): State<AppState>,
    Path(lamp_id): Path<String>,
    _admin: AdminUser,
) -> Result<Json<SimpleResponse>, AppError> {
    let success = state.hue.blacklist_lamp(&lamp_id).await?;
    Ok(Json(SimpleResponse {
        success,
        message: if success { "Hue lamp blacklisted" } else { "Hue lamp not found" }.to_string(),
    }))
}
