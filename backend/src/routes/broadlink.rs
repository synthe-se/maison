use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use maison_api::Message;
use serde::{Deserialize, Serialize};

use crate::{
    auth::AdminUser,
    broadlink::{
        BroadlinkCodeEntry, BroadlinkDiscoveredDevice, BroadlinkSecurityMode, CodeDetails,
        LearnResult, SaveCodeRequest, SendResult, StoredClimateState,
    },
    error::{AppError, Refusal},
    mitsubishi_ir::ClimateSettings,
    routes::{Answer, Reply},
    util::refused,
    AppState,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RediscoverQuery {
    local_ip: Option<String>,
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
    save_code: Option<CodeDetails>,
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
#[serde(rename_all = "camelCase")]
struct Devices {
    total: usize,
    devices: Vec<BroadlinkDiscoveredDevice>,
    message: &'static str,
}

impl Devices {
    fn of(devices: Vec<BroadlinkDiscoveredDevice>) -> Json<Answer<Self>> {
        Answer::ok(Self {
            total: devices.len(),
            devices,
            message: "Broadlink discovery completed",
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Codes {
    total: usize,
    codes: Vec<BroadlinkCodeEntry>,
    message: &'static str,
}

impl Codes {
    fn of(codes: Vec<BroadlinkCodeEntry>, message: &'static str) -> Json<Answer<Self>> {
        Answer::ok(Self {
            total: codes.len(),
            codes,
            message,
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    message: String,
    code: BroadlinkCodeEntry,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ClimateState {
    state: Option<StoredClimateState>,
    message: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Done<T> {
    result: T,
    message: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/discover", get(discover).post(rediscover))
        .route("/provision", post(provision))
        .route("/learn/ir", post(learn_ir))
        .route("/send", post(send_packet))
        .route("/codes", get(list_codes).post(save_code))
        .route("/codes/{code_id}/send", post(send_code))
        .route("/mitsubishi/codes", get(list_mitsubishi_codes))
        .route("/mitsubishi/send", post(send_mitsubishi_command))
        .route("/mitsubishi/state", get(get_climate_state))
}

/// The blasters found before, else one search that tries a few broadcasts and answers at
/// the first that finds one (the climate tile asks this; it never scans on its own).
async fn discover(State(state): State<AppState>) -> Result<Json<Answer<Devices>>, AppError> {
    Ok(Devices::of(state.broadlink.discover().await?))
}

/// Scans the network again now, from a chosen interface when given: setting up, an
/// admin's, and never a GET (a link cannot start it).
async fn rediscover(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(query): Query<RediscoverQuery>,
) -> Result<Json<Answer<Devices>>, AppError> {
    Ok(Devices::of(
        state.broadlink.rediscover(query.local_ip).await?,
    ))
}

async fn provision(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<ProvisionRequest>,
) -> Result<Json<Answer<Message>>, AppError> {
    if body.ssid.trim().is_empty() {
        return Err(refused(Refusal::Invalid, "a network needs its SSID"));
    }

    state
        .broadlink
        .provision(
            body.ssid.trim().to_string(),
            body.password,
            body.security_mode,
        )
        .await?;

    Ok(Json(Answer::said(
        "Provisioning packet sent to Broadlink device in AP mode",
    )))
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

async fn list_codes(State(state): State<AppState>) -> Json<Answer<Codes>> {
    Codes::of(
        state.broadlink.list_codes().await,
        "Broadlink codes retrieved",
    )
}

async fn save_code(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<SaveCodeRequest>,
) -> Result<Json<Answer<Saved>>, AppError> {
    let code = state.broadlink.save_code(body).await?;
    Ok(Answer::ok(Saved {
        message: format!("Code '{}' saved", code.name),
        code,
    }))
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
) -> Json<Answer<Codes>> {
    Codes::of(
        state
            .broadlink
            .list_mitsubishi_codes(query.model.as_deref())
            .await,
        "Mitsubishi IR codes retrieved",
    )
}

async fn get_climate_state(State(state): State<AppState>) -> Json<Answer<ClimateState>> {
    Answer::ok(ClimateState {
        state: state.broadlink.climate_state().await,
        message: "Last commanded Mitsubishi state",
    })
}

async fn send_mitsubishi_command(
    State(state): State<AppState>,
    Json(body): Json<MitsubishiCommandRequest>,
) -> Result<Json<Answer<Done<SendResult>>>, AppError> {
    let broadlink = &state.broadlink;
    let result = match (body.command, body.settings) {
        (Some(command), None) => {
            broadlink
                .send_mitsubishi_command(body.host, body.local_ip, command, body.model)
                .await?
        }
        (None, Some(settings)) => {
            broadlink
                .send_mitsubishi_settings(body.host, body.local_ip, &settings, body.model)
                .await?
        }
        _ => {
            return Err(refused(
                Refusal::Invalid,
                "send either a command or settings",
            ))
        }
    };
    Ok(Answer::ok(Done {
        message: format!(
            "Mitsubishi command sent via Broadlink device {}",
            result.host
        ),
        result,
    }))
}
