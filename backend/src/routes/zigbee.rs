use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use maison_api::{
    lamps::{ColorRequest, EffectRequest, ZigbeeLamp},
    Message,
};
use serde::Serialize;

use crate::{
    auth::AdminUser,
    error::AppError,
    lamps::{LampState, LampStats},
    routes::{
        lamps::{self, changed, Changed, LampBackend},
        Answer, Reply,
    },
    zigbee::{ZigbeeManager, ZigbeePairingStatus},
    AppState,
};

impl LampBackend for ZigbeeManager {
    const NAME: &'static str = "Zigbee lamp";
    type View = ZigbeeLamp;

    fn of(state: &AppState) -> &Self {
        &state.zigbee
    }
    async fn list(&self) -> Vec<ZigbeeLamp> {
        self.list_lamps().await
    }
    async fn get(&self, id: &str) -> Option<ZigbeeLamp> {
        self.get_lamp(id).await
    }
    async fn stats(&self) -> LampStats {
        ZigbeeManager::stats(self).await
    }
    async fn listing(&self) -> (Vec<ZigbeeLamp>, LampStats) {
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

/// The pairing window as it is (`/lamps/pairing/*`).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Pairing {
    pairing: ZigbeePairingStatus,
    message: String,
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

fn pairing_answer(pairing: ZigbeePairingStatus, message: &str) -> Json<Answer<Pairing>> {
    Answer::ok(Pairing {
        pairing,
        message: message.to_string(),
    })
}

async fn pairing_status(State(state): State<AppState>) -> Json<Answer<Pairing>> {
    let pairing = state.zigbee.pairing_status().await;
    let message = if pairing.active {
        "Zigbee pairing is active"
    } else {
        "Zigbee pairing is inactive"
    };
    pairing_answer(pairing, message)
}

async fn start_pairing(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<Json<Answer<Pairing>>, AppError> {
    Ok(pairing_answer(
        state.zigbee.start_pairing().await?,
        "Zigbee pairing started",
    ))
}

async fn stop_pairing(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<Json<Answer<Pairing>>, AppError> {
    Ok(pairing_answer(
        state.zigbee.stop_pairing().await?,
        "Zigbee pairing stopped",
    ))
}

async fn touchlink_scan(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<Json<Answer<Message>>, AppError> {
    state.zigbee.touchlink_scan().await?;
    Ok(Json(Answer::said("Touchlink scan initiated")))
}

async fn set_color(
    State(state): State<AppState>,
    Path(lamp_id): Path<String>,
    Json(body): Json<ColorRequest>,
) -> Result<Json<Answer<Changed>>, AppError> {
    let lamp_state = state.zigbee.set_color(&lamp_id, body.x, body.y).await?;
    Ok(changed(lamp_state, "Zigbee lamp color updated"))
}

async fn set_effect(
    State(state): State<AppState>,
    Path(lamp_id): Path<String>,
    Json(body): Json<EffectRequest>,
) -> Result<Json<Answer<Changed>>, AppError> {
    let lamp_state = state.zigbee.set_effect(&lamp_id, &body.effect).await?;
    Ok(changed(lamp_state, "Zigbee lamp effect applied"))
}
