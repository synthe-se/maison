use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

use crate::{AppState, error::AppError, meross, tuya::DeviceRef};

#[derive(Debug, Serialize)]
struct MerossListResponse {
    success: bool,
    devices: Vec<meross::MerossDeviceListEntry>,
    total: usize,
    message: &'static str,
}

#[derive(Debug, Serialize)]
struct MerossStatusResponse {
    success: bool,
    device: DeviceRef,
    status: meross::MerossStatus,
    message: &'static str,
}

#[derive(Debug, Serialize)]
struct MerossElectricityResponse {
    success: bool,
    device: DeviceRef,
    electricity: meross::MerossElectricityFormatted,
    message: &'static str,
}

#[derive(Debug, Serialize)]
struct MerossConsumptionResponse {
    success: bool,
    device: DeviceRef,
    consumption: Vec<meross::MerossConsumptionEntry>,
    summary: meross::MerossConsumptionSummary,
    message: &'static str,
}

#[derive(Debug, Serialize)]
struct MerossToggleResponse {
    success: bool,
    device: DeviceRef,
    on: bool,
    message: String,
}

#[derive(Debug, Serialize)]
struct MerossDndResponse {
    success: bool,
    device: DeviceRef,
    #[serde(rename = "dndMode")]
    dnd_mode: bool,
    message: String,
}

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
        .route("/stats", get(stats))
        .route("/{device_id}/status", get(status))
        .route("/{device_id}/electricity", get(electricity))
        .route("/{device_id}/consumption", get(consumption))
        .route("/{device_id}/toggle", post(toggle))
        .route("/{device_id}/dnd", post(set_dnd))
}

async fn list_devices(State(state): State<AppState>) -> Json<MerossListResponse> {
    let devices = state.meross.list_devices().await;
    Json(MerossListResponse {
        success: true,
        total: devices.len(),
        devices,
        message: "Meross devices list retrieved",
    })
}

async fn stats(State(state): State<AppState>) -> Json<meross::MerossStats> {
    Json(state.meross.get_stats().await)
}

async fn status(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
) -> Result<Json<MerossStatusResponse>, AppError> {
    let (device, status) = state.meross.get_status(&device_id).await?;
    Ok(Json(MerossStatusResponse {
        success: true,
        device,
        status,
        message: "Status retrieved",
    }))
}

async fn electricity(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
) -> Result<Json<MerossElectricityResponse>, AppError> {
    let (device, electricity) = state.meross.get_electricity(&device_id).await?;
    Ok(Json(MerossElectricityResponse {
        success: true,
        device,
        electricity,
        message: "Electricity data retrieved",
    }))
}

async fn consumption(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
) -> Result<Json<MerossConsumptionResponse>, AppError> {
    let (device, consumption, summary) = state.meross.get_consumption(&device_id).await?;
    Ok(Json(MerossConsumptionResponse {
        success: true,
        device,
        consumption,
        summary,
        message: "Consumption history retrieved",
    }))
}

async fn toggle(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    Json(body): Json<ToggleBody>,
) -> Result<Json<MerossToggleResponse>, AppError> {
    let device = state.meross.toggle(&device_id, body.on).await?;
    Ok(Json(MerossToggleResponse {
        success: true,
        message: format!("{} turned {}", device.name, if body.on { "on" } else { "off" }),
        device,
        on: body.on,
    }))
}

async fn set_dnd(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    Json(body): Json<DndBody>,
) -> Result<Json<MerossDndResponse>, AppError> {
    let device = state.meross.set_dnd(&device_id, body.enabled).await?;
    let (dnd, led) = if body.enabled { ("enabled", "off") } else { ("disabled", "on") };
    Ok(Json(MerossDndResponse {
        success: true,
        device,
        dnd_mode: body.enabled,
        message: format!("DND mode {dnd} (LED {led})"),
    }))
}
