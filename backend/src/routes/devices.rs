use std::collections::BTreeMap;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::{
    AppState,
    auth::AdminUser,
    error::AppError,
    routes::SimpleResponse,
    tuya::{self, DeviceRef, TuyaDeviceType, dps},
};

const FEEDER_MAX_PORTIONS: u64 = 10;
const FEEDER_WARN_PORTIONS: u64 = 12;
const LITTER_MAX_CLEAN_DELAY_SECONDS: u64 = 1800;
const FOUNTAIN_MAX_UV_RUNTIME_HOURS: u64 = 24;

#[derive(Debug, Serialize)]
struct DevicesListResponse {
    success: bool,
    devices: Vec<tuya::TuyaDeviceListEntry>,
    total: usize,
    message: &'static str,
}

#[derive(Debug, Serialize)]
struct StatsResponse {
    success: bool,
    total: usize,
    connected: usize,
    disconnected: usize,
    devices: Vec<DeviceConnectionStatsEntry>,
}

#[derive(Debug, Serialize)]
struct DeviceConnectionStatsEntry {
    id: String,
    name: String,
    #[serde(rename = "type")]
    device_type: String,
    #[serde(rename = "isConnected")]
    is_connected: bool,
    connecting: bool,
    #[serde(rename = "reconnectAttempts")]
    reconnect_attempts: i32,
}

#[derive(Debug, Serialize)]
struct DpsScanResponse {
    success: bool,
    scan_range: String,
    scanned_count: u64,
    found_count: usize,
    available_dps: BTreeMap<String, DpsValueSummary>,
    errors_count: usize,
    errors: Option<BTreeMap<String, String>>,
    message: String,
}

#[derive(Debug, Serialize)]
struct DpsValueSummary {
    value: Value,
    #[serde(rename = "type")]
    value_type: &'static str,
    length: Option<usize>,
}

#[derive(Debug, Serialize)]
struct DeviceStatusResponse {
    success: bool,
    device: DeviceRef,
    parsed_status: Value,
    raw_status: Map<String, Value>,
    message: &'static str,
}

#[derive(Debug, Serialize)]
struct TypedStatusResponse {
    success: bool,
    device: DeviceRef,
    parsed_status: Value,
    raw_dps: Map<String, Value>,
    message: &'static str,
}

#[derive(Debug, Serialize)]
struct ActionResponse {
    success: bool,
    message: String,
    device: DeviceRef,
}

#[derive(Debug, Serialize)]
struct MealPlanResponse {
    success: bool,
    device: DeviceRef,
    decoded: Option<Vec<MealPlanEntry>>,
    meal_plan: Option<String>,
    message: String,
}

#[derive(Debug, Serialize)]
struct MealPlanUpdateResponse {
    success: bool,
    message: String,
    device: DeviceRef,
    encoded_base64: String,
    formatted_meal_plan: String,
}

#[derive(Debug, Serialize)]
struct LitterBoxSettingsResponse {
    success: bool,
    message: String,
    device: DeviceRef,
    updated_settings: usize,
}

#[derive(Debug, Serialize)]
struct FountainUvResponse {
    success: bool,
    message: String,
    device: DeviceRef,
    applied_settings: FountainUvAppliedSettings,
}

#[derive(Debug, Serialize)]
struct FountainUvAppliedSettings {
    enabled: Option<bool>,
    runtime: Option<u64>,
}

#[derive(Debug, Serialize)]
struct FountainEcoModeResponse {
    success: bool,
    message: String,
    device: DeviceRef,
    eco_mode: u64,
}

#[derive(Debug, Serialize)]
struct FountainPowerResponse {
    success: bool,
    message: String,
    device: DeviceRef,
    power: bool,
}

#[derive(Debug, Deserialize)]
struct FeedRequest {
    #[serde(default = "default_feeder_portion")]
    portion: u64,
}

#[derive(Debug, Deserialize)]
struct MealPlanRequest {
    meal_plan: Vec<MealPlanEntry>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct MealPlanEntry {
    days_of_week: Vec<String>,
    time: String,
    portion: u8,
    status: String,
}

#[derive(Debug, Deserialize)]
struct LitterBoxSettingsRequest {
    clean_delay: Option<u64>,
    sleep_mode: Option<LitterSleepModeSettings>,
    preferences: Option<LitterPreferences>,
    actions: Option<LitterActions>,
}

#[derive(Debug, Deserialize)]
struct LitterSleepModeSettings {
    enabled: Option<bool>,
    start_time: Option<String>,
    end_time: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LitterPreferences {
    child_lock: Option<bool>,
    kitten_mode: Option<bool>,
    lighting: Option<bool>,
    prompt_sound: Option<bool>,
    automatic_homing: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct LitterActions {
    reset_sand_level: Option<bool>,
    reset_factory_settings: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct FountainUvSettingsRequest {
    enabled: Option<bool>,
    runtime: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct FountainEcoModeRequest {
    mode: u64,
}

#[derive(Debug, Deserialize)]
struct FountainPowerRequest {
    enabled: bool,
}

#[derive(Debug, Deserialize)]
struct ScanDpsQuery {
    start: Option<String>,
    end: Option<String>,
}

type DeviceId = Path<String>;

pub fn router() -> Router<AppState> {
    use TuyaDeviceType::{Feeder, Fountain, LitterBox};
    Router::new()
        .route("/", get(list_devices))
        .route("/stats", get(stats))
        .route("/reconnect", post(reconnect))
        .route("/connect", post(connect_all))
        .route("/disconnect", post(disconnect_all))
        .route("/{device_id}/connect", post(connect_device))
        .route("/{device_id}/disconnect", post(disconnect_device))
        .route("/{device_id}/status", get(status))
        .route("/{device_id}/scan-dps", get(scan_dps))
        .route("/{device_id}/feeder/feed", post(feeder_feed))
        .route(
            "/{device_id}/feeder/status",
            get(|s: State<AppState>, id: DeviceId| typed_status(s, id, Feeder, "Feeder status retrieved successfully")),
        )
        .route("/{device_id}/feeder/meal-plan", get(feeder_meal_plan).post(update_feeder_meal_plan))
        .route(
            "/{device_id}/litter-box/clean",
            post(|s: State<AppState>, id: DeviceId| {
                action(s, id, LitterBox, dps::litter::CLEAN, json!(true), "Manual cleaning cycle initiated")
            }),
        )
        .route("/{device_id}/litter-box/settings", post(update_litter_box_settings))
        .route(
            "/{device_id}/litter-box/status",
            get(|s: State<AppState>, id: DeviceId| {
                typed_status(s, id, LitterBox, "Litter box status retrieved successfully")
            }),
        )
        .route(
            "/{device_id}/fountain/reset/water",
            post(|s: State<AppState>, id: DeviceId| {
                action(s, id, Fountain, dps::fountain::WATER_RESET, json!(0), "Water time counter reset")
            }),
        )
        .route(
            "/{device_id}/fountain/reset/filter",
            post(|s: State<AppState>, id: DeviceId| {
                action(s, id, Fountain, dps::fountain::FILTER_RESET, json!(true), "Filter life counter reset")
            }),
        )
        .route(
            "/{device_id}/fountain/reset/pump",
            post(|s: State<AppState>, id: DeviceId| {
                action(s, id, Fountain, dps::fountain::PUMP_RESET, json!(true), "Pump time counter reset")
            }),
        )
        .route("/{device_id}/fountain/uv", post(update_fountain_uv))
        .route("/{device_id}/fountain/eco-mode", post(update_fountain_eco_mode))
        .route("/{device_id}/fountain/power", post(update_fountain_power))
        .route(
            "/{device_id}/fountain/status",
            get(|s: State<AppState>, id: DeviceId| {
                typed_status(s, id, Fountain, "Fountain status retrieved successfully")
            }),
        )
}

async fn list_devices(State(state): State<AppState>) -> Json<DevicesListResponse> {
    let devices = state.tuya.list_devices().await;
    Json(DevicesListResponse {
        success: true,
        total: devices.len(),
        devices,
        message: "Devices list retrieved successfully",
    })
}

async fn status(State(state): State<AppState>, Path(device_id): DeviceId) -> Result<Json<DeviceStatusResponse>, AppError> {
    let (device, raw_status, parsed_status) = state.tuya.get_status(&device_id).await?;
    Ok(Json(DeviceStatusResponse {
        success: true,
        device,
        parsed_status,
        raw_status,
        message: "Device status retrieved successfully",
    }))
}

async fn stats(State(state): State<AppState>) -> Json<StatsResponse> {
    let stats = state.tuya.connection_stats().await;
    Json(StatsResponse {
        success: true,
        total: stats.total,
        connected: stats.connected,
        disconnected: stats.disconnected,
        devices: stats
            .devices
            .into_iter()
            .map(|device| DeviceConnectionStatsEntry {
                id: device.id,
                name: device.name,
                device_type: device.device_type,
                is_connected: device.connected,
                connecting: device.connecting,
                reconnect_attempts: device.reconnect_attempts,
            })
            .collect(),
    })
}

async fn reconnect(State(state): State<AppState>) -> Json<SimpleResponse> {
    state.tuya.reconnect_disconnected().await;
    SimpleResponse::ok("Reconnection initiated for disconnected devices")
}

async fn connect_all(State(state): State<AppState>) -> Json<SimpleResponse> {
    state.tuya.connect_all_devices().await;
    SimpleResponse::ok("All devices connection initiated")
}

async fn disconnect_all(State(state): State<AppState>) -> Json<SimpleResponse> {
    state.tuya.disconnect_all_devices().await;
    SimpleResponse::ok("All devices disconnected")
}

async fn connect_device(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
) -> Result<Json<SimpleResponse>, AppError> {
    let device = state.tuya.get_device_ref(&device_id)?;
    state.tuya.connect_device(&device_id).await?;
    Ok(SimpleResponse::ok(format!("Device {} connection initiated", device.id)))
}

async fn disconnect_device(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
) -> Result<Json<SimpleResponse>, AppError> {
    let device = state.tuya.get_device_ref(&device_id)?;
    state.tuya.disconnect_device(&device_id).await?;
    Ok(SimpleResponse::ok(format!("Device {} disconnected", device.id)))
}

/// The data points a device reports within `start..=end` (a debugging aid: admin only).
async fn scan_dps(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
    Query(query): Query<ScanDpsQuery>,
    _admin: AdminUser,
) -> Result<Json<DpsScanResponse>, AppError> {
    state.tuya.get_device_ref(&device_id)?;
    let (start, end) = scan_range(query.start.as_deref(), query.end.as_deref())?;
    let (_, raw_status, _) = state.tuya.get_status(&device_id).await?;
    let available_dps = dps_in_range(&raw_status, start, end);
    let scanned_count = u64::from(end) - u64::from(start) + 1;
    let found_count = available_dps.len();

    Ok(Json(DpsScanResponse {
        success: true,
        scan_range: format!("{start}-{end}"),
        scanned_count,
        found_count,
        available_dps,
        errors_count: 0,
        errors: None,
        message: format!("DPS scan completed: {found_count} active DPS found out of {scanned_count} scanned"),
    }))
}

fn scan_range(start: Option<&str>, end: Option<&str>) -> Result<(u32, u32), AppError> {
    let start = start
        .unwrap_or("1")
        .parse::<u32>()
        .map_err(|_| AppError::bad_request("Invalid start DPS"))?;
    let end = end
        .unwrap_or("255")
        .parse::<u32>()
        .map_err(|_| AppError::bad_request("Invalid end DPS"))?;
    if start == 0 || end < start {
        return Err(AppError::bad_request("Invalid DPS scan range"));
    }
    Ok((start, end))
}

/// The reported points whose number falls in `start..=end`: the device's own keys are
/// walked, never the range (a range up to 4 294 967 295 would stall the Pi).
fn dps_in_range(raw_status: &Map<String, Value>, start: u32, end: u32) -> BTreeMap<String, DpsValueSummary> {
    raw_status
        .iter()
        .filter(|(key, _)| key.parse::<u32>().is_ok_and(|id| (start..=end).contains(&id)))
        .map(|(key, value)| {
            let summary = DpsValueSummary {
                value: value.clone(),
                value_type: dps_value_type(value),
                length: value.as_str().map(str::len),
            };
            (key.clone(), summary)
        })
        .collect()
}

/// One device type's status, parsed: the feeder, litter box and fountain routes.
async fn typed_status(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
    kind: TuyaDeviceType,
    message: &'static str,
) -> Result<Json<TypedStatusResponse>, AppError> {
    let (device, raw_dps, parsed_status) = state.tuya.get_typed_status(&device_id, kind).await?;
    Ok(Json(TypedStatusResponse {
        success: true,
        device,
        parsed_status,
        raw_dps,
        message,
    }))
}

/// One fixed command to one data point (a cleaning cycle, a counter reset): `done` says
/// what happened, the device's name follows.
async fn action(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
    kind: TuyaDeviceType,
    dps: &'static str,
    value: Value,
    done: &'static str,
) -> Result<Json<ActionResponse>, AppError> {
    let device = state.tuya.send_typed_command(&device_id, kind, dps, value).await?;
    Ok(Json(ActionResponse {
        success: true,
        message: format!("{done} for {}", device.name),
        device,
    }))
}

async fn feeder_feed(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
    Json(body): Json<FeedRequest>,
) -> Result<Json<ActionResponse>, AppError> {
    if !(1..=FEEDER_WARN_PORTIONS).contains(&body.portion) {
        return Err(AppError::bad_request(format!("portion must be between 1 and {FEEDER_WARN_PORTIONS}")));
    }

    let device = state
        .tuya
        .send_typed_command(&device_id, TuyaDeviceType::Feeder, dps::feeder::MANUAL_FEED, json!(body.portion))
        .await?;

    Ok(Json(ActionResponse {
        success: true,
        message: format!("Manual feed command sent to {} with portions: {}", device.name, body.portion),
        device,
    }))
}

async fn feeder_meal_plan(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
) -> Result<Json<MealPlanResponse>, AppError> {
    let (device, meal_plan) = state.tuya.feeder_meal_plan(&device_id).await?;
    let decoded = meal_plan.as_deref().map(decode_meal_plan).transpose()?;
    let message = if meal_plan.is_some() {
        "Current meal plan retrieved"
    } else {
        "Meal plan not available yet."
    };

    Ok(Json(MealPlanResponse {
        success: true,
        device,
        decoded,
        meal_plan,
        message: message.to_string(),
    }))
}

async fn update_feeder_meal_plan(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
    Json(body): Json<MealPlanRequest>,
) -> Result<Json<MealPlanUpdateResponse>, AppError> {
    if body.meal_plan.is_empty() {
        return Err(AppError::bad_request("meal_plan array is required"));
    }
    if body.meal_plan.len() > 10 {
        return Err(AppError::bad_request("meal_plan supports at most 10 entries"));
    }
    for (index, entry) in body.meal_plan.iter().enumerate() {
        validate_meal_plan_entry(entry, index)?;
    }

    let encoded = encode_meal_plan(&body.meal_plan)?;
    let device = state
        .tuya
        .send_typed_command(&device_id, TuyaDeviceType::Feeder, dps::feeder::MEAL_PLAN, json!(encoded))
        .await?;

    Ok(Json(MealPlanUpdateResponse {
        success: true,
        message: format!("Meal plan updated for {}", device.name),
        device,
        encoded_base64: encoded,
        formatted_meal_plan: format_meal_plan(&body.meal_plan),
    }))
}

async fn update_litter_box_settings(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
    Json(body): Json<LitterBoxSettingsRequest>,
) -> Result<Json<LitterBoxSettingsResponse>, AppError> {
    let updates = litter_box_updates(body)?;
    let updated_settings = updates.len();
    let device = state
        .tuya
        .send_typed_commands(&device_id, TuyaDeviceType::LitterBox, updates)
        .await?;

    Ok(Json(LitterBoxSettingsResponse {
        success: true,
        message: format!("Settings updated for {}", device.name),
        device,
        updated_settings,
    }))
}

/// The data points a litter box settings request writes, validated.
fn litter_box_updates(body: LitterBoxSettingsRequest) -> Result<Vec<(String, Value)>, AppError> {
    use dps::litter::*;
    let mut updates = Vec::new();
    let mut push = |dps: &str, value: Option<Value>| {
        if let Some(value) = value {
            updates.push((dps.to_string(), value));
        }
    };

    if let Some(clean_delay) = body.clean_delay {
        if clean_delay > LITTER_MAX_CLEAN_DELAY_SECONDS {
            return Err(AppError::bad_request("clean_delay must be between 0 and 1800 seconds"));
        }
        push(CLEAN_DELAY, Some(json!(clean_delay)));
    }

    if let Some(sleep_mode) = body.sleep_mode {
        push(SLEEP_ENABLED, sleep_mode.enabled.map(Value::Bool));
        let start = sleep_mode.start_time.as_deref().map(parse_hhmm_to_minutes).transpose()?;
        push(SLEEP_START, start.map(|minutes| json!(minutes)));
        let end = sleep_mode.end_time.as_deref().map(parse_hhmm_to_minutes).transpose()?;
        push(SLEEP_END, end.map(|minutes| json!(minutes)));
    }

    if let Some(preferences) = body.preferences {
        push(CHILD_LOCK, preferences.child_lock.map(Value::Bool));
        push(KITTEN_MODE, preferences.kitten_mode.map(Value::Bool));
        push(LIGHTING, preferences.lighting.map(Value::Bool));
        push(PROMPT_SOUND, preferences.prompt_sound.map(Value::Bool));
        push(AUTOMATIC_HOMING, preferences.automatic_homing.map(Value::Bool));
    }

    if let Some(actions) = body.actions {
        push(RESET_SAND_LEVEL, (actions.reset_sand_level == Some(true)).then_some(json!(true)));
        push(FACTORY_RESET, (actions.reset_factory_settings == Some(true)).then_some(json!(true)));
    }

    if updates.is_empty() {
        return Err(AppError::bad_request("No valid settings provided"));
    }
    Ok(updates)
}

async fn update_fountain_uv(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
    Json(body): Json<FountainUvSettingsRequest>,
) -> Result<Json<FountainUvResponse>, AppError> {
    let mut updates = Vec::new();
    if let Some(enabled) = body.enabled {
        updates.push((dps::fountain::UV.to_string(), Value::Bool(enabled)));
    }
    if let Some(runtime) = body.runtime {
        if runtime > FOUNTAIN_MAX_UV_RUNTIME_HOURS {
            return Err(AppError::bad_request("UV runtime must be between 0 and 24 hours"));
        }
        updates.push((dps::fountain::UV_RUNTIME.to_string(), json!(runtime)));
    }
    if updates.is_empty() {
        return Err(AppError::bad_request("No valid settings provided"));
    }

    let applied_settings = FountainUvAppliedSettings {
        enabled: body.enabled,
        runtime: body.runtime,
    };
    let summary = describe_fountain_uv_updates(&applied_settings);
    let device = state
        .tuya
        .send_typed_commands(&device_id, TuyaDeviceType::Fountain, updates)
        .await?;

    Ok(Json(FountainUvResponse {
        success: true,
        message: format!("UV settings updated for {}: {summary}", device.name),
        device,
        applied_settings,
    }))
}

async fn update_fountain_eco_mode(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
    Json(body): Json<FountainEcoModeRequest>,
) -> Result<Json<FountainEcoModeResponse>, AppError> {
    if !(1..=2).contains(&body.mode) {
        return Err(AppError::bad_request("Eco mode must be 1 or 2"));
    }

    let device = state
        .tuya
        .send_typed_command(&device_id, TuyaDeviceType::Fountain, dps::fountain::ECO_MODE, json!(body.mode))
        .await?;

    Ok(Json(FountainEcoModeResponse {
        success: true,
        message: format!("Eco mode set to {} for {}", body.mode, device.name),
        device,
        eco_mode: body.mode,
    }))
}

async fn update_fountain_power(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
    Json(body): Json<FountainPowerRequest>,
) -> Result<Json<FountainPowerResponse>, AppError> {
    let device = state
        .tuya
        .send_typed_command(&device_id, TuyaDeviceType::Fountain, dps::fountain::POWER, Value::Bool(body.enabled))
        .await?;

    Ok(Json(FountainPowerResponse {
        success: true,
        message: format!(
            "Light {} for {}",
            if body.enabled { "turned on" } else { "turned off" },
            device.name
        ),
        device,
        power: body.enabled,
    }))
}

fn default_feeder_portion() -> u64 {
    1
}

fn parse_hhmm_to_minutes(value: &str) -> Result<u64, AppError> {
    let invalid = || AppError::bad_request("Invalid time format. Use HH:MM");
    let (hours, minutes) = value.split_once(':').ok_or_else(invalid)?;
    let hours = hours.parse::<u64>().map_err(|_| invalid())?;
    let minutes = minutes.parse::<u64>().map_err(|_| invalid())?;
    if hours > 23 || minutes > 59 {
        return Err(invalid());
    }
    Ok((hours * 60) + minutes)
}

fn validate_meal_plan_entry(entry: &MealPlanEntry, index: usize) -> Result<(), AppError> {
    let valid = !entry.days_of_week.is_empty()
        && entry.days_of_week.iter().all(|day| day_index(day).is_some())
        && parse_hhmm_to_minutes(&entry.time).is_ok()
        && (1..=FEEDER_MAX_PORTIONS).contains(&u64::from(entry.portion))
        && (entry.status == "Enabled" || entry.status == "Disabled");
    if valid {
        Ok(())
    } else {
        Err(AppError::bad_request(format!("Invalid meal plan entry at index {index}")))
    }
}

fn encode_meal_plan(entries: &[MealPlanEntry]) -> Result<String, AppError> {
    let mut encoded = Vec::with_capacity(entries.len() * 5);
    for entry in entries {
        let days_bits = entry.days_of_week.iter().try_fold(0_u8, |acc, day| {
            day_index(day)
                .map(|index| acc | (1 << index))
                .ok_or_else(|| AppError::bad_request("Invalid day of week"))
        })?;
        // at most 23:59, so both fit a byte
        let total_minutes = parse_hhmm_to_minutes(&entry.time)?;
        let status = u8::from(entry.status == "Enabled");
        let (hours, minutes) = ((total_minutes / 60) as u8, (total_minutes % 60) as u8);
        encoded.extend_from_slice(&[days_bits, hours, minutes, entry.portion, status]);
    }
    Ok(STANDARD.encode(encoded))
}

fn decode_meal_plan(encoded: &str) -> Result<Vec<MealPlanEntry>, AppError> {
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|error| AppError::service_unavailable(error.to_string()))?;
    let mut entries = Vec::new();

    for chunk in bytes.chunks(5) {
        if chunk.len() < 5 {
            break;
        }

        let days_of_week = (0..7)
            .filter(|index| chunk[0] & (1 << index) != 0)
            .map(day_name)
            .collect::<Vec<_>>();
        let time = format!("{:02}:{:02}", chunk[1], chunk[2]);
        let status = if chunk[4] == 1 { "Enabled" } else { "Disabled" };

        entries.push(MealPlanEntry {
            days_of_week,
            time,
            portion: chunk[3],
            status: status.to_string(),
        });
    }

    Ok(entries)
}

fn format_meal_plan(entries: &[MealPlanEntry]) -> String {
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            format!(
                "{}. {} a {} - {} serving(s) - {}",
                index + 1,
                entry.days_of_week.join(", "),
                entry.time,
                entry.portion,
                entry.status,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn day_index(day: &str) -> Option<u8> {
    match day {
        "Monday" => Some(0),
        "Tuesday" => Some(1),
        "Wednesday" => Some(2),
        "Thursday" => Some(3),
        "Friday" => Some(4),
        "Saturday" => Some(5),
        "Sunday" => Some(6),
        _ => None,
    }
}

fn day_name(index: u8) -> String {
    match index {
        0 => "Monday",
        1 => "Tuesday",
        2 => "Wednesday",
        3 => "Thursday",
        4 => "Friday",
        5 => "Saturday",
        6 => "Sunday",
        _ => "Unknown",
    }
    .to_string()
}

fn describe_fountain_uv_updates(settings: &FountainUvAppliedSettings) -> String {
    let mut parts = Vec::new();
    if let Some(enabled) = settings.enabled {
        parts.push(if enabled {
            "UV light enabled".to_string()
        } else {
            "UV light disabled".to_string()
        });
    }
    if let Some(runtime) = settings.runtime {
        parts.push(format!("UV runtime set to {runtime} hours"));
    }
    parts.join(", ")
}

fn dps_value_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hhmm_to_minutes_accepts_valid_times() {
        assert_eq!(parse_hhmm_to_minutes("00:00").unwrap(), 0);
        assert_eq!(parse_hhmm_to_minutes("08:30").unwrap(), 510);
        assert_eq!(parse_hhmm_to_minutes("23:59").unwrap(), 1_439);
    }

    #[test]
    fn parse_hhmm_to_minutes_rejects_invalid_times() {
        let error = parse_hhmm_to_minutes("24:00").unwrap_err();
        assert_eq!(error.to_string(), "Invalid time format. Use HH:MM");

        let error = parse_hhmm_to_minutes("bad").unwrap_err();
        assert_eq!(error.to_string(), "Invalid time format. Use HH:MM");
    }

    #[test]
    fn validate_meal_plan_entry_accepts_valid_entry() {
        let entry = MealPlanEntry {
            days_of_week: vec!["Monday".to_string(), "Wednesday".to_string()],
            time: "08:30".to_string(),
            portion: 2,
            status: "Enabled".to_string(),
        };

        validate_meal_plan_entry(&entry, 0).unwrap();
    }

    #[test]
    fn validate_meal_plan_entry_rejects_invalid_day() {
        let entry = MealPlanEntry {
            days_of_week: vec!["Funday".to_string()],
            time: "08:30".to_string(),
            portion: 2,
            status: "Enabled".to_string(),
        };

        let error = validate_meal_plan_entry(&entry, 0).unwrap_err();
        assert_eq!(error.to_string(), "Invalid meal plan entry at index 0");
    }

    #[test]
    fn validate_meal_plan_entry_rejects_invalid_portion() {
        let entry = MealPlanEntry {
            days_of_week: vec!["Monday".to_string()],
            time: "08:30".to_string(),
            portion: 11,
            status: "Enabled".to_string(),
        };

        let error = validate_meal_plan_entry(&entry, 3).unwrap_err();
        assert_eq!(error.to_string(), "Invalid meal plan entry at index 3");
    }

    #[test]
    fn encode_meal_plan_matches_legacy_format() {
        let entries = vec![MealPlanEntry {
            days_of_week: vec!["Monday".to_string(), "Wednesday".to_string()],
            time: "08:30".to_string(),
            portion: 2,
            status: "Enabled".to_string(),
        }];

        let encoded = encode_meal_plan(&entries).unwrap();
        assert_eq!(encoded, "BQgeAgE=");
    }

    #[test]
    fn decode_meal_plan_matches_legacy_payload() {
        let decoded = decode_meal_plan("BQgeAgE=").unwrap();

        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].days_of_week, vec!["Monday", "Wednesday"]);
        assert_eq!(decoded[0].time, "08:30");
        assert_eq!(decoded[0].portion, 2);
        assert_eq!(decoded[0].status, "Enabled");
    }

    #[test]
    fn encode_then_decode_meal_plan_roundtrips() {
        let entries = vec![
            MealPlanEntry {
                days_of_week: vec!["Monday".to_string(), "Friday".to_string()],
                time: "07:15".to_string(),
                portion: 1,
                status: "Enabled".to_string(),
            },
            MealPlanEntry {
                days_of_week: vec!["Sunday".to_string()],
                time: "18:45".to_string(),
                portion: 3,
                status: "Disabled".to_string(),
            },
        ];

        let encoded = encode_meal_plan(&entries).unwrap();
        let decoded = decode_meal_plan(&encoded).unwrap();

        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].days_of_week, entries[0].days_of_week);
        assert_eq!(decoded[0].time, entries[0].time);
        assert_eq!(decoded[0].portion, entries[0].portion);
        assert_eq!(decoded[0].status, entries[0].status);
        assert_eq!(decoded[1].days_of_week, entries[1].days_of_week);
        assert_eq!(decoded[1].time, entries[1].time);
        assert_eq!(decoded[1].portion, entries[1].portion);
        assert_eq!(decoded[1].status, entries[1].status);
    }

    #[test]
    fn decode_meal_plan_rejects_invalid_base64() {
        let error = decode_meal_plan("***").unwrap_err();
        assert!(!error.to_string().is_empty());
    }

    #[test]
    fn scan_range_defaults_and_rejects_bad_bounds() {
        assert_eq!(scan_range(None, None).unwrap(), (1, 255));
        assert_eq!(scan_range(Some("100"), Some("4294967295")).unwrap(), (100, u32::MAX));
        assert_eq!(scan_range(Some("0"), None).unwrap_err().to_string(), "Invalid DPS scan range");
        assert_eq!(scan_range(Some("9"), Some("3")).unwrap_err().to_string(), "Invalid DPS scan range");
        assert_eq!(scan_range(Some("x"), None).unwrap_err().to_string(), "Invalid start DPS");
        assert_eq!(scan_range(None, Some("-1")).unwrap_err().to_string(), "Invalid end DPS");
    }

    #[test]
    fn dps_in_range_walks_the_reported_points_only() {
        let raw = json!({ "1": true, "101": "abc", "300": 4, "x": 1 }).as_object().cloned().unwrap();
        let found = dps_in_range(&raw, 1, u32::MAX);
        assert_eq!(found.keys().collect::<Vec<_>>(), vec!["1", "101", "300"]);
        assert_eq!(found["101"].value_type, "string");
        assert_eq!(found["101"].length, Some(3));
        assert_eq!(found["1"].value_type, "boolean");
        assert_eq!(dps_in_range(&raw, 2, 200).len(), 1);
    }

    fn litter(body: Value) -> Result<Vec<(String, Value)>, AppError> {
        litter_box_updates(serde_json::from_value(body).unwrap())
    }

    #[test]
    fn litter_settings_become_their_data_points() {
        let updates = litter(json!({
            "clean_delay": 120,
            "sleep_mode": { "enabled": true, "start_time": "21:30", "end_time": "07:00" },
            "preferences": { "child_lock": true, "lighting": false },
            "actions": { "reset_sand_level": true, "reset_factory_settings": false },
        }))
        .unwrap();
        let expected = [
            (dps::litter::CLEAN_DELAY, json!(120)),
            (dps::litter::SLEEP_ENABLED, json!(true)),
            (dps::litter::SLEEP_START, json!(1290)),
            (dps::litter::SLEEP_END, json!(420)),
            (dps::litter::CHILD_LOCK, json!(true)),
            (dps::litter::LIGHTING, json!(false)),
            (dps::litter::RESET_SAND_LEVEL, json!(true)),
        ]
        .map(|(id, value)| (id.to_string(), value));
        assert_eq!(updates, expected);
    }

    #[test]
    fn litter_settings_reject_bad_or_empty_requests() {
        assert_eq!(litter(json!({})).unwrap_err().to_string(), "No valid settings provided");
        assert_eq!(
            litter(json!({ "actions": { "reset_factory_settings": false } })).unwrap_err().to_string(),
            "No valid settings provided"
        );
        assert!(litter(json!({ "clean_delay": 1801 })).is_err());
        assert!(litter(json!({ "sleep_mode": { "start_time": "25:00" } })).is_err());
    }

    #[test]
    fn parse_hhmm_rejects_extra_parts() {
        assert!(parse_hhmm_to_minutes("08:30:00").is_err());
        assert!(parse_hhmm_to_minutes("08").is_err());
    }
}
