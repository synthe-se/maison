use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    auth::AdminUser,
    error::AppError,
    lamps::{LampState, LampStats},
    routes::{
        SimpleResponse,
        lamps::{self, ActionResponse, LampBackend},
    },
    zigbee::{self, ZigbeeManager},
};

impl LampBackend for ZigbeeManager {
    const NAME: &'static str = "Zigbee lamp";
    type View = zigbee::ZigbeeLampView;

    fn of(state: &AppState) -> &Self {
        &state.zigbee
    }
    async fn list(&self) -> Vec<zigbee::ZigbeeLampView> {
        self.list_lamps().await
    }
    async fn get(&self, id: &str) -> Option<zigbee::ZigbeeLampView> {
        self.get_lamp(id).await
    }
    async fn stats(&self) -> LampStats {
        ZigbeeManager::stats(self).await
    }
    async fn listing(&self) -> (Vec<zigbee::ZigbeeLampView>, LampStats) {
        ZigbeeManager::listing(self).await
    }
    async fn set_power(&self, id: &str, on: bool) -> Result<LampState, AppError> {
        ZigbeeManager::set_power(self, id, on).await
    }
    async fn set_brightness(&self, id: &str, brightness: u8) -> Result<LampState, AppError> {
        ZigbeeManager::set_brightness(self, id, brightness).await
    }
    async fn set_temperature(&self, id: &str, temperature: u8) -> Result<LampState, AppError> {
        ZigbeeManager::set_temperature(self, id, temperature).await
    }
    async fn rename(&self, id: &str, name: &str) -> Result<(), AppError> {
        self.rename_lamp(id, name).await
    }
}

#[derive(Debug, Serialize)]
struct PairingResponse {
    success: bool,
    pairing: zigbee::ZigbeePairingStatus,
    message: String,
}

#[derive(Debug, Deserialize)]
struct ColorBody {
    x: f32,
    y: f32,
}

/// The effect by its name (`candle`, `blink`…); `ZigbeeEffect` says which exist.
#[derive(Debug, Deserialize)]
struct EffectBody {
    effect: String,
}

/// The shared lamp routes under `/lamps`, plus Zigbee's own: pairing (admin to open or
/// close the network), colour and effects.
pub fn router() -> Router<AppState> {
    Router::new()
        .nest("/lamps", lamps::router::<ZigbeeManager>())
        .route("/lamps/pairing/start", post(start_pairing))
        .route("/lamps/pairing/stop", post(stop_pairing))
        .route("/lamps/pairing/status", get(pairing_status))
        .route("/lamps/pairing/touchlink", post(touchlink_scan))
        .route("/lamps/{lamp_id}/color", post(set_color))
        .route("/lamps/{lamp_id}/effect", post(set_effect))
}

fn pairing_response(pairing: zigbee::ZigbeePairingStatus, message: &str) -> Json<PairingResponse> {
    Json(PairingResponse { success: true, pairing, message: message.to_string() })
}

async fn pairing_status(State(state): State<AppState>) -> Json<PairingResponse> {
    let pairing = state.zigbee.pairing_status().await;
    let message = if pairing.active { "Zigbee pairing is active" } else { "Zigbee pairing is inactive" };
    pairing_response(pairing, message)
}

async fn start_pairing(State(state): State<AppState>, _admin: AdminUser) -> Result<Json<PairingResponse>, AppError> {
    Ok(pairing_response(state.zigbee.start_pairing().await?, "Zigbee pairing started"))
}

async fn stop_pairing(State(state): State<AppState>, _admin: AdminUser) -> Result<Json<PairingResponse>, AppError> {
    Ok(pairing_response(state.zigbee.stop_pairing().await?, "Zigbee pairing stopped"))
}

async fn touchlink_scan(State(state): State<AppState>, _admin: AdminUser) -> Result<Json<SimpleResponse>, AppError> {
    state.zigbee.touchlink_scan().await?;
    Ok(SimpleResponse::ok("Touchlink scan initiated"))
}

async fn set_color(
    State(state): State<AppState>,
    Path(lamp_id): Path<String>,
    Json(body): Json<ColorBody>,
) -> Result<Json<ActionResponse>, AppError> {
    let lamp_state = state.zigbee.set_color(&lamp_id, body.x, body.y).await?;
    Ok(ActionResponse::ok(lamp_state, "Zigbee lamp color updated".to_string()))
}

async fn set_effect(
    State(state): State<AppState>,
    Path(lamp_id): Path<String>,
    Json(body): Json<EffectBody>,
) -> Result<Json<ActionResponse>, AppError> {
    let lamp_state = state.zigbee.set_effect(&lamp_id, &body.effect).await?;
    Ok(ActionResponse::ok(lamp_state, "Zigbee lamp effect applied".to_string()))
}
