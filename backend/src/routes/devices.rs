use std::collections::BTreeMap;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::{
    AppState,
    auth::AdminUser,
    error::AppError,
    routes::{Answer, DeviceRef, SimpleResponse},
    tuya::{
        self, TuyaDeviceType, dps,
        feeder::{self, MealPlanEntry},
        parse_hhmm,
    },
};

const FEEDER_WARN_PORTIONS: u64 = 12;
const LITTER_MAX_CLEAN_DELAY_SECONDS: u64 = 1800;
const FOUNTAIN_MAX_UV_RUNTIME_HOURS: u64 = 24;

/// `{success, device, message, ..}`: what every device answer says, its own fields after.
#[derive(Debug, Serialize)]
struct DeviceAnswer<T: Serialize> {
    device: DeviceRef,
    message: String,
    #[serde(flatten)]
    body: T,
}

fn answer<T: Serialize>(device: DeviceRef, message: impl Into<String>, body: T) -> Json<Answer<DeviceAnswer<T>>> {
    Answer::ok(DeviceAnswer { device, message: message.into(), body })
}

#[derive(Debug, Serialize)]
struct DevicesList {
    devices: Vec<tuya::TuyaDeviceListEntry>,
    total: usize,
    message: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DpsScan {
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
#[serde(rename_all = "camelCase")]
struct TypedStatus {
    parsed_status: Value,
    raw_dps: Map<String, Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MealPlan {
    decoded: Option<Vec<MealPlanEntry>>,
    meal_plan: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MealPlanUpdate {
    encoded_base64: String,
    formatted_meal_plan: String,
}

#[derive(Debug, Serialize)]
struct FountainUvAppliedSettings {
    enabled: Option<bool>,
    runtime: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct FeedRequest {
    #[serde(default = "default_feeder_portion")]
    portion: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MealPlanRequest {
    meal_plan: Vec<MealPlanEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LitterBoxSettingsRequest {
    clean_delay: Option<u64>,
    sleep_mode: Option<LitterSleepModeSettings>,
    preferences: Option<LitterPreferences>,
    actions: Option<LitterActions>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LitterSleepModeSettings {
    enabled: Option<bool>,
    start_time: Option<String>,
    end_time: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LitterPreferences {
    child_lock: Option<bool>,
    kitten_mode: Option<bool>,
    lighting: Option<bool>,
    prompt_sound: Option<bool>,
    automatic_homing: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
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
        .route("/connect", post(connect_all))
        .route("/disconnect", post(disconnect_all))
        .route("/{device_id}/connect", post(connect_device))
        .route("/{device_id}/disconnect", post(disconnect_device))
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

async fn list_devices(State(state): State<AppState>) -> Json<Answer<DevicesList>> {
    let devices = state.tuya.list_devices().await;
    Answer::ok(DevicesList { total: devices.len(), devices, message: "Devices list retrieved successfully" })
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
) -> Result<Json<Answer<DpsScan>>, AppError> {
    state.tuya.get_device_ref(&device_id)?;
    let (start, end) = scan_range(query.start.as_deref(), query.end.as_deref())?;
    let (_, raw_status, _) = state.tuya.get_status(&device_id).await?;
    let available_dps = dps_in_range(&raw_status, start, end);
    let scanned_count = u64::from(end) - u64::from(start) + 1;
    let found_count = available_dps.len();

    Ok(Answer::ok(DpsScan {
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
) -> Result<Json<Answer<DeviceAnswer<TypedStatus>>>, AppError> {
    let (device, raw_dps, parsed_status) = state.tuya.get_typed_status(&device_id, kind).await?;
    Ok(answer(device, message, TypedStatus { parsed_status, raw_dps }))
}

/// The answer of a command with nothing more to say.
type Done = Json<Answer<DeviceAnswer<serde_json::Map<String, Value>>>>;

fn done(device: DeviceRef, message: String) -> Done {
    answer(device, message, Map::new())
}

/// One fixed command to one data point (a cleaning cycle, a counter reset): `done` says
/// what happened, the device's name follows.
async fn action(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
    kind: TuyaDeviceType,
    dps: &'static str,
    value: Value,
    what: &'static str,
) -> Result<Done, AppError> {
    let device = state.tuya.send_typed_command(&device_id, kind, dps, value).await?;
    let message = format!("{what} for {}", device.name);
    Ok(done(device, message))
}

async fn feeder_feed(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
    Json(body): Json<FeedRequest>,
) -> Result<Done, AppError> {
    if !(1..=FEEDER_WARN_PORTIONS).contains(&body.portion) {
        return Err(AppError::bad_request(format!("portion must be between 1 and {FEEDER_WARN_PORTIONS}")));
    }

    let device = state
        .tuya
        .send_typed_command(&device_id, TuyaDeviceType::Feeder, dps::feeder::MANUAL_FEED, json!(body.portion))
        .await?;

    let message = format!("Manual feed command sent to {} with portions: {}", device.name, body.portion);
    Ok(done(device, message))
}

async fn feeder_meal_plan(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
) -> Result<Json<Answer<DeviceAnswer<MealPlan>>>, AppError> {
    let (device, meal_plan) = state.tuya.feeder_meal_plan(&device_id).await?;
    let decoded = meal_plan.as_deref().map(feeder::decode).transpose()?;
    let message = if meal_plan.is_some() { "Current meal plan retrieved" } else { "Meal plan not available yet." };
    Ok(answer(device, message, MealPlan { decoded, meal_plan }))
}

async fn update_feeder_meal_plan(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
    Json(body): Json<MealPlanRequest>,
) -> Result<Json<Answer<DeviceAnswer<MealPlanUpdate>>>, AppError> {
    let encoded = feeder::encode(&body.meal_plan)?;
    let device = state
        .tuya
        .send_typed_command(&device_id, TuyaDeviceType::Feeder, dps::feeder::MEAL_PLAN, json!(encoded))
        .await?;

    let message = format!("Meal plan updated for {}", device.name);
    Ok(answer(device, message, MealPlanUpdate { encoded_base64: encoded, formatted_meal_plan: feeder::describe(&body.meal_plan) }))
}

async fn update_litter_box_settings(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
    Json(body): Json<LitterBoxSettingsRequest>,
) -> Result<Json<Answer<DeviceAnswer<Value>>>, AppError> {
    let updates = litter_box_updates(body)?;
    let updated_settings = updates.len();
    let device = state
        .tuya
        .send_typed_commands(&device_id, TuyaDeviceType::LitterBox, updates)
        .await?;

    let message = format!("Settings updated for {}", device.name);
    Ok(answer(device, message, json!({ "updatedSettings": updated_settings })))
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
            return Err(AppError::bad_request("cleanDelay must be between 0 and 1800 seconds"));
        }
        push(CLEAN_DELAY, Some(json!(clean_delay)));
    }

    if let Some(sleep_mode) = body.sleep_mode {
        push(SLEEP_ENABLED, sleep_mode.enabled.map(Value::Bool));
        let start = sleep_mode.start_time.as_deref().map(minutes_of_day).transpose()?;
        push(SLEEP_START, start.map(|minutes| json!(minutes)));
        let end = sleep_mode.end_time.as_deref().map(minutes_of_day).transpose()?;
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
) -> Result<Json<Answer<DeviceAnswer<Value>>>, AppError> {
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

    let message = format!("UV settings updated for {}: {summary}", device.name);
    Ok(answer(device, message, json!({ "appliedSettings": applied_settings })))
}

async fn update_fountain_eco_mode(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
    Json(body): Json<FountainEcoModeRequest>,
) -> Result<Json<Answer<DeviceAnswer<Value>>>, AppError> {
    if !(1..=2).contains(&body.mode) {
        return Err(AppError::bad_request("Eco mode must be 1 or 2"));
    }

    let device = state
        .tuya
        .send_typed_command(&device_id, TuyaDeviceType::Fountain, dps::fountain::ECO_MODE, json!(body.mode))
        .await?;

    let message = format!("Eco mode set to {} for {}", body.mode, device.name);
    Ok(answer(device, message, json!({ "ecoMode": body.mode })))
}

async fn update_fountain_power(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
    Json(body): Json<FountainPowerRequest>,
) -> Result<Json<Answer<DeviceAnswer<Value>>>, AppError> {
    let device = state
        .tuya
        .send_typed_command(&device_id, TuyaDeviceType::Fountain, dps::fountain::POWER, Value::Bool(body.enabled))
        .await?;

    let message = format!("Light {} for {}", if body.enabled { "turned on" } else { "turned off" }, device.name);
    Ok(answer(device, message, json!({ "power": body.enabled })))
}

fn default_feeder_portion() -> u64 {
    1
}

/// `"HH:MM"` as minutes since midnight (the litter box's sleep hours).
fn minutes_of_day(value: &str) -> Result<u16, AppError> {
    let (hours, minutes) = parse_hhmm(value)?;
    Ok(u16::from(hours) * 60 + u16::from(minutes))
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
    fn sleep_hours_are_minutes_of_the_day() {
        assert_eq!(minutes_of_day("00:00").unwrap(), 0);
        assert_eq!(minutes_of_day("21:30").unwrap(), 1290);
        assert_eq!(minutes_of_day("23:59").unwrap(), 1_439);
        assert!(minutes_of_day("24:00").is_err());
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
            "cleanDelay": 120,
            "sleepMode": { "enabled": true, "startTime": "21:30", "endTime": "07:00" },
            "preferences": { "childLock": true, "lighting": false },
            "actions": { "resetSandLevel": true, "resetFactorySettings": false },
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
            litter(json!({ "actions": { "resetFactorySettings": false } })).unwrap_err().to_string(),
            "No valid settings provided"
        );
        assert!(litter(json!({ "cleanDelay": 1801 })).is_err());
        assert!(litter(json!({ "sleepMode": { "startTime": "25:00" } })).is_err());
    }
}
