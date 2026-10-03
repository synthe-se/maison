use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    AppState,
    error::AppError,
    meross,
    routes::{Answer, DeviceRef},
};

#[derive(Debug, Serialize)]
struct PlugList {
    devices: Vec<meross::MerossDeviceListEntry>,
    total: usize,
    message: &'static str,
}

/// `{success, device, message, ..}`: what every plug answer says, its own fields after.
#[derive(Debug, Serialize)]
struct PlugAnswer {
    device: DeviceRef,
    message: String,
    #[serde(flatten)]
    body: Value,
}

fn answer(device: DeviceRef, message: impl Into<String>, body: Value) -> Json<Answer<PlugAnswer>> {
    Answer::ok(PlugAnswer { device, message: message.into(), body })
}

type Answered = Result<Json<Answer<PlugAnswer>>, AppError>;

#[derive(Debug, Deserialize)]
struct ToggleBody {
    on: bool,
}

#[derive(Debug, Deserialize)]
struct DndBody {
    enabled: bool,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_devices))
        .route("/{device_id}/status", get(status))
        .route("/{device_id}/electricity", get(electricity))
        .route("/{device_id}/consumption", get(consumption))
        .route("/{device_id}/toggle", post(toggle))
        .route("/{device_id}/dnd", post(set_dnd))
}

async fn list_devices(State(state): State<AppState>) -> Json<Answer<PlugList>> {
    let devices = state.meross.list_devices().await;
    Answer::ok(PlugList { total: devices.len(), devices, message: "Meross devices list retrieved" })
}

async fn status(State(state): State<AppState>, Path(device_id): Path<String>) -> Answered {
    let (device, status) = state.meross.get_status(&device_id).await?;
    Ok(answer(device, "Status retrieved", json!({ "status": status })))
}

async fn electricity(State(state): State<AppState>, Path(device_id): Path<String>) -> Answered {
    let (device, electricity) = state.meross.get_electricity(&device_id).await?;
    Ok(answer(device, "Electricity data retrieved", json!({ "electricity": electricity })))
}

async fn consumption(State(state): State<AppState>, Path(device_id): Path<String>) -> Answered {
    let (device, consumption, summary) = state.meross.get_consumption(&device_id).await?;
    Ok(answer(device, "Consumption history retrieved", json!({ "consumption": consumption, "summary": summary })))
}

async fn toggle(State(state): State<AppState>, Path(device_id): Path<String>, Json(body): Json<ToggleBody>) -> Answered {
    let device = state.meross.toggle(&device_id, body.on).await?;
    let message = format!("{} turned {}", device.name, if body.on { "on" } else { "off" });
    Ok(answer(device, message, json!({ "on": body.on })))
}

async fn set_dnd(State(state): State<AppState>, Path(device_id): Path<String>, Json(body): Json<DndBody>) -> Answered {
    let device = state.meross.set_dnd(&device_id, body.enabled).await?;
    let (dnd, led) = if body.enabled { ("enabled", "off") } else { ("disabled", "on") };
    Ok(answer(device, format!("DND mode {dnd} (LED {led})"), json!({ "dndMode": body.enabled })))
}
