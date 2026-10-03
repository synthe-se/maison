use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    auth::{AdminUser, AuthenticatedUser},
    broadlink::{BroadlinkSecurityMode, LearnCodeSaveRequest, LearnResult, SaveCodeRequest, SendResult},
    error::AppError,
    mitsubishi_ir::ClimateSettings,
    passkey::Refusal,
    routes::{Answer, SimpleResponse},
};
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiscoverQuery {
    local_ip: Option<String>,
    force_refresh: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MitsubishiQuery {
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProvisionRequest {
    ssid: String,
    password: Option<String>,
    security_mode: BroadlinkSecurityMode,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LearnIrRequest {
    host: String,
    local_ip: Option<String>,
    timeout_secs: Option<u64>,
    save_code: Option<LearnCodeSaveRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SendPacketRequest {
    host: String,
    local_ip: Option<String>,
    packet_base64: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SendCodeRequest {
    host: String,
    local_ip: Option<String>,
}

/// A structured command as text (`state-cool-21-…`) or as settings (`{mode, temperature,
/// …}`, written into a command by the backend): one of the two.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MitsubishiCommandRequest {
    host: String,
    local_ip: Option<String>,
    command: Option<String>,
    settings: Option<ClimateSettings>,
    model: Option<String>,
}

#[derive(Debug, Serialize)]
struct Listed<T> {
    total: usize,
    #[serde(flatten)]
    items: T,
    message: &'static str,
}

#[derive(Debug, Serialize)]
struct Done<T> {
    result: T,
    message: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/discover", get(discover))
        .route("/provision", post(provision))
        .route("/learn/ir", post(learn_ir))
        .route("/send", post(send_packet))
        .route("/codes", get(list_codes).post(save_code))
        .route("/codes/{code_id}/send", post(send_code))
        .route("/mitsubishi/codes", get(list_mitsubishi_codes))
        .route("/mitsubishi/send", post(send_mitsubishi_command))
        .route("/mitsubishi/state", get(get_climate_state))
}

async fn discover(
    State(state): State<AppState>,
    Query(query): Query<DiscoverQuery>,
    user: AuthenticatedUser,
) -> Result<Json<Answer<Listed<Value>>>, AppError> {
    // The climate tile reads the cached list; scanning the network again, or from a
    // chosen interface, is setting up.
    let force_refresh = query.force_refresh.unwrap_or(false);
    if (force_refresh || query.local_ip.is_some()) && !user.0.is_admin() {
        return Err(Refusal::Forbidden.into());
    }
    let devices = state
        .broadlink
        .discover(query.local_ip, force_refresh)
        .await?;
    Ok(Answer::ok(Listed {
        total: devices.len(),
        items: json!({ "devices": devices }),
        message: "Broadlink discovery completed",
    }))
}

async fn provision(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<ProvisionRequest>,
) -> Result<Json<SimpleResponse>, AppError> {
    if body.ssid.trim().is_empty() {
        return Err(AppError::bad_request("ssid is required"));
    }

    state
        .broadlink
        .provision(body.ssid.trim().to_string(), body.password, body.security_mode)
        .await?;

    Ok(SimpleResponse::ok("Provisioning packet sent to Broadlink device in AP mode"))
}

async fn learn_ir(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<LearnIrRequest>,
) -> Result<Json<Answer<Done<LearnResult>>>, AppError> {
    let result = state
        .broadlink
        .learn_ir(body.host, body.local_ip, body.timeout_secs, body.save_code)
        .await?;
    Ok(Answer::ok(Done {
        message: "IR code learned successfully".to_string(),
        result,
    }))
}

async fn send_packet(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<SendPacketRequest>,
) -> Result<Json<Answer<Done<SendResult>>>, AppError> {
    let result = state
        .broadlink
        .send_packet(body.host, body.local_ip, body.packet_base64, None, None)
        .await?;
    Ok(Answer::ok(Done {
        message: format!("Packet sent to Broadlink device {}", result.host),
        result,
    }))
}

async fn list_codes(State(state): State<AppState>) -> Json<Answer<Listed<Value>>> {
    let codes = state.broadlink.list_codes().await;
    Answer::ok(Listed { total: codes.len(), items: json!({ "codes": codes }), message: "Broadlink codes retrieved" })
}

async fn save_code(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<SaveCodeRequest>,
) -> Result<Json<Answer<Value>>, AppError> {
    let code = state.broadlink.save_code(body).await?;
    Ok(Answer::ok(json!({ "message": format!("Code '{}' saved", code.name), "code": code })))
}

async fn send_code(
    State(state): State<AppState>,
    Path(code_id): Path<String>,
    Json(body): Json<SendCodeRequest>,
) -> Result<Json<Answer<Done<SendResult>>>, AppError> {
    let result = state
        .broadlink
        .send_saved_code(body.host, body.local_ip, code_id)
        .await?;
    Ok(Answer::ok(Done {
        message: format!("Saved code sent to Broadlink device {}", result.host),
        result,
    }))
}

async fn list_mitsubishi_codes(
    State(state): State<AppState>,
    Query(query): Query<MitsubishiQuery>,
) -> Json<Answer<Listed<Value>>> {
    let codes = state.broadlink.list_mitsubishi_codes(query.model.as_deref()).await;
    Answer::ok(Listed { total: codes.len(), items: json!({ "codes": codes }), message: "Mitsubishi IR codes retrieved" })
}

async fn get_climate_state(State(state): State<AppState>) -> Json<Answer<Value>> {
    Answer::ok(json!({
        "state": state.broadlink.climate_state().await,
        "message": "Last commanded Mitsubishi state",
    }))
}

async fn send_mitsubishi_command(
    State(state): State<AppState>,
    Json(body): Json<MitsubishiCommandRequest>,
) -> Result<Json<Answer<Done<SendResult>>>, AppError> {
    let broadlink = &state.broadlink;
    let result = match (body.command, body.settings) {
        (Some(command), None) => broadlink.send_mitsubishi_command(body.host, body.local_ip, command, body.model).await?,
        (None, Some(settings)) => {
            broadlink.send_mitsubishi_settings(body.host, body.local_ip, &settings, body.model).await?
        }
        _ => return Err(AppError::bad_request("Send either a command or settings")),
    };
    Ok(Answer::ok(Done {
        message: format!("Mitsubishi command sent via Broadlink device {}", result.host),
        result,
    }))
}
