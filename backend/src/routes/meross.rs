use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use maison_api::{
    meross::{Consumption, ElectricityAnswer, PlugList, ToggleRequest},
    OfDevice,
};
use serde::{Deserialize, Serialize};

use crate::{
    error::AppError,
    meross::MerossStatus,
    routes::{Answer, DeviceRef, Reply},
    AppState,
};

/// What every plug answer says (`{success, device, message, ..}`), its own fields after.
type Answered<T> = Result<Json<Answer<OfDevice<T>>>, AppError>;

fn answer<T: Serialize>(device: DeviceRef, message: impl Into<String>, body: T) -> Answered<T> {
    Ok(Answer::ok(OfDevice {
        device,
        message: message.into(),
        body,
    }))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    status: MerossStatus,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Switched {
    on: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Dnd {
    dnd_mode: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    Answer::ok(PlugList {
        total: devices.len(),
        devices,
        message: "Meross devices list retrieved".into(),
    })
}

async fn status(State(state): State<AppState>, Path(device_id): Path<String>) -> Answered<Status> {
    let (device, status) = state.meross.get_status(&device_id).await?;
    answer(device, "Status retrieved", Status { status })
}

async fn electricity(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
) -> Result<Json<Answer<ElectricityAnswer>>, AppError> {
    let (device, electricity) = state.meross.get_electricity(&device_id).await?;
    Ok(Answer::ok(ElectricityAnswer {
        device,
        message: "Electricity data retrieved".into(),
        electricity,
    }))
}

async fn consumption(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
) -> Answered<Consumption> {
    let (device, consumption, summary) = state.meross.get_consumption(&device_id).await?;
    answer(
        device,
        "Consumption history retrieved",
        Consumption {
            consumption,
            summary,
        },
    )
}

async fn toggle(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    Json(body): Json<ToggleRequest>,
) -> Answered<Switched> {
    let device = state.meross.toggle(&device_id, body.on).await?;
    let message = format!(
        "{} turned {}",
        device.name,
        if body.on { "on" } else { "off" }
    );
    answer(device, message, Switched { on: body.on })
}

async fn set_dnd(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    Json(body): Json<DndBody>,
) -> Answered<Dnd> {
    let device = state.meross.set_dnd(&device_id, body.enabled).await?;
    let (dnd, led) = if body.enabled {
        ("enabled", "off")
    } else {
        ("disabled", "on")
    };
    answer(
        device,
        format!("DND mode {dnd} (LED {led})"),
        Dnd {
            dnd_mode: body.enabled,
        },
    )
}
