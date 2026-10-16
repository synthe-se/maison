use std::collections::BTreeMap;

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use maison_api::{Message, OfDevice};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::{
    auth::AdminUser,
    error::{AppError, Refusal},
    routes::{Answer, DeviceRef, Reply},
    tuya::{
        self, dps,
        feeder::{self, MealPlanEntry},
        TuyaDeviceType,
    },
    util::{minutes_of_day, refused},
    AppState,
};

const LITTER_MAX_CLEAN_DELAY_SECONDS: u64 = 1800;
const FOUNTAIN_MAX_UV_RUNTIME_HOURS: u64 = 24;

/// What every device answer says (`{success, device, message, ..}`), its own fields after.
type Said<T> = Json<Answer<OfDevice<T>>>;

fn answer<T: Serialize>(device: DeviceRef, message: impl Into<String>, body: T) -> Said<T> {
    Answer::ok(OfDevice {
        device,
        message: message.into(),
        body,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
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
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DpsValueSummary {
    value: Value,
    r#type: &'static str,
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
#[serde(rename_all = "camelCase")]
struct LitterSettingsUpdate {
    updated_settings: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FountainUvUpdate {
    applied_settings: FountainUvAppliedSettings,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FountainUvAppliedSettings {
    enabled: Option<bool>,
    runtime: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FountainEcoModeUpdate {
    eco_mode: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FountainPowerUpdate {
    power: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FeedRequest {
    #[serde(default = "default_feeder_portion")]
    portion: u8,
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
    /// An admin's, through `POST …/litter-box/factory-reset`: refused here.
    reset_factory_settings: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FountainUvSettingsRequest {
    enabled: Option<bool>,
    runtime: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FountainEcoModeRequest {
    mode: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FountainPowerRequest {
    enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScanDpsQuery {
    start: Option<String>,
    end: Option<String>,
}

/// A status read: from what the device's session knows, or read from the device again
/// (`?refresh=true`, a radio round trip of two seconds or more).
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatusQuery {
    #[serde(default)]
    refresh: bool,
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
            get(|s: State<AppState>, id: DeviceId, q: Query<StatusQuery>| {
                typed_status(s, id, q, Feeder, "Feeder status retrieved successfully")
            }),
        )
        .route(
            "/{device_id}/feeder/meal-plan",
            get(feeder_meal_plan).post(update_feeder_meal_plan),
        )
        .route(
            "/{device_id}/litter-box/clean",
            post(|s: State<AppState>, id: DeviceId| {
                action(
                    s,
                    id,
                    LitterBox,
                    dps::litter::CLEAN,
                    json!(true),
                    "Manual cleaning cycle initiated",
                )
            }),
        )
        .route(
            "/{device_id}/litter-box/settings",
            post(update_litter_box_settings),
        )
        .route(
            "/{device_id}/litter-box/factory-reset",
            post(|s: State<AppState>, id: DeviceId, _admin: AdminUser| {
                action(
                    s,
                    id,
                    LitterBox,
                    dps::litter::FACTORY_RESET,
                    json!(true),
                    "Factory settings restored",
                )
            }),
        )
        .route(
            "/{device_id}/litter-box/status",
            get(|s: State<AppState>, id: DeviceId, q: Query<StatusQuery>| {
                typed_status(
                    s,
                    id,
                    q,
                    LitterBox,
                    "Litter box status retrieved successfully",
                )
            }),
        )
        .route(
            "/{device_id}/fountain/reset/water",
            post(|s: State<AppState>, id: DeviceId| {
                action(
                    s,
                    id,
                    Fountain,
                    dps::fountain::WATER_RESET,
                    json!(0),
                    "Water time counter reset",
                )
            }),
        )
        .route(
            "/{device_id}/fountain/reset/filter",
            post(|s: State<AppState>, id: DeviceId| {
                action(
                    s,
                    id,
                    Fountain,
                    dps::fountain::FILTER_RESET,
                    json!(true),
                    "Filter life counter reset",
                )
            }),
        )
        .route(
            "/{device_id}/fountain/reset/pump",
            post(|s: State<AppState>, id: DeviceId| {
                action(
                    s,
                    id,
                    Fountain,
                    dps::fountain::PUMP_RESET,
                    json!(true),
                    "Pump time counter reset",
                )
            }),
        )
        .route("/{device_id}/fountain/uv", post(update_fountain_uv))
        .route(
            "/{device_id}/fountain/eco-mode",
            post(update_fountain_eco_mode),
        )
        .route("/{device_id}/fountain/power", post(update_fountain_power))
        .route(
            "/{device_id}/fountain/status",
            get(|s: State<AppState>, id: DeviceId, q: Query<StatusQuery>| {
                typed_status(s, id, q, Fountain, "Fountain status retrieved successfully")
            }),
        )
}

async fn list_devices(State(state): State<AppState>) -> Json<Answer<DevicesList>> {
    let devices = state.tuya.list_devices().await;
    Answer::ok(DevicesList {
        total: devices.len(),
        devices,
        message: "Devices list retrieved successfully",
    })
}

async fn connect_all(State(state): State<AppState>) -> Json<Answer<Message>> {
    state.tuya.connect_all_devices().await;
    Json(Answer::said("All devices connection initiated"))
}

async fn disconnect_all(State(state): State<AppState>) -> Json<Answer<Message>> {
    state.tuya.disconnect_all_devices().await;
    Json(Answer::said("All devices disconnected"))
}

async fn connect_device(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
) -> Result<Json<Answer<Message>>, AppError> {
    let device = state.tuya.get_device_ref(&device_id)?;
    state.tuya.connect_device(&device_id).await?;
    Ok(Json(Answer::said(format!(
        "Device {} connection initiated",
        device.id
    ))))
}

async fn disconnect_device(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
) -> Result<Json<Answer<Message>>, AppError> {
    let device = state.tuya.get_device_ref(&device_id)?;
    state.tuya.disconnect_device(&device_id).await?;
    Ok(Json(Answer::said(format!(
        "Device {} disconnected",
        device.id
    ))))
}

/// The data points a device reports within `start..=end`, read from it now (a debugging
/// aid: admin only).
async fn scan_dps(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
    Query(query): Query<ScanDpsQuery>,
    _admin: AdminUser,
) -> Result<Json<Answer<DpsScan>>, AppError> {
    state.tuya.get_device_ref(&device_id)?;
    let (start, end) = scan_range(query.start.as_deref(), query.end.as_deref())?;
    let (_, raw_status, _) = state.tuya.get_status(&device_id, true).await?;
    let available_dps = dps_in_range(&raw_status, start, end);
    let scanned_count = u64::from(end) - u64::from(start) + 1;
    let found_count = available_dps.len();

    Ok(Answer::ok(DpsScan {
        scan_range: format!("{start}-{end}"),
        scanned_count,
        found_count,
        available_dps,
        message: format!(
            "DPS scan completed: {found_count} active DPS found out of {scanned_count} scanned"
        ),
    }))
}

fn scan_range(start: Option<&str>, end: Option<&str>) -> Result<(u32, u32), AppError> {
    let bound = |value: Option<&str>, default: &str| {
        let value = value.unwrap_or(default);
        value
            .parse::<u32>()
            .map_err(|_| refused(Refusal::Invalid, format!("DPS {value:?}")))
    };
    let (start, end) = (bound(start, "1")?, bound(end, "255")?);
    if start == 0 || end < start {
        return Err(refused(
            Refusal::Invalid,
            format!("DPS scan range {start}-{end}"),
        ));
    }
    Ok((start, end))
}

/// The reported points whose number falls in `start..=end`: the device's own keys are
/// walked, never the range (a range up to 4 294 967 295 would stall the Pi).
fn dps_in_range(
    raw_status: &Map<String, Value>,
    start: u32,
    end: u32,
) -> BTreeMap<String, DpsValueSummary> {
    raw_status
        .iter()
        .filter(|(key, _)| {
            key.parse::<u32>()
                .is_ok_and(|id| (start..=end).contains(&id))
        })
        .map(|(key, value)| {
            let summary = DpsValueSummary {
                value: value.clone(),
                r#type: dps_value_type(value),
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
    Query(query): Query<StatusQuery>,
    kind: TuyaDeviceType,
    message: &'static str,
) -> Result<Said<TypedStatus>, AppError> {
    let (device, raw_dps, parsed_status) = state
        .tuya
        .get_typed_status(&device_id, kind, query.refresh)
        .await?;
    Ok(answer(
        device,
        message,
        TypedStatus {
            parsed_status,
            raw_dps,
        },
    ))
}

/// Nothing more to say than what was done.
#[derive(Debug, Serialize)]
struct Nothing {}

/// One fixed command to one data point (a cleaning cycle, a counter reset): `what` says
/// what happened, the device's name follows.
async fn action(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
    kind: TuyaDeviceType,
    dps: &'static str,
    value: Value,
    what: &'static str,
) -> Result<Said<Nothing>, AppError> {
    let device = state
        .tuya
        .send_typed_command(&device_id, kind, dps, value)
        .await?;
    let message = format!("{what} for {}", device.name);
    Ok(answer(device, message, Nothing {}))
}

/// Feeds on demand, `feeder::MAX_FEED_PORTIONS` at most, within the feeder's limits (one
/// feed per `FEED_COOLDOWN`, `DAILY_PORTIONS` a day: `too_many_attempts` past them).
async fn feeder_feed(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
    Json(body): Json<FeedRequest>,
) -> Result<Said<Nothing>, AppError> {
    if !(1..=feeder::MAX_FEED_PORTIONS).contains(&body.portion) {
        return Err(refused(
            Refusal::Invalid,
            format!("{} portions", body.portion),
        ));
    }
    let device = state.tuya.feed(&device_id, body.portion).await?;
    let message = format!(
        "Manual feed command sent to {} with portions: {}",
        device.name, body.portion
    );
    Ok(answer(device, message, Nothing {}))
}

async fn feeder_meal_plan(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
) -> Result<Said<MealPlan>, AppError> {
    let (device, meal_plan) = state.tuya.feeder_meal_plan(&device_id).await?;
    let decoded = meal_plan.as_deref().map(feeder::decode).transpose()?;
    let message = if meal_plan.is_some() {
        "Current meal plan retrieved"
    } else {
        "Meal plan not available yet."
    };
    Ok(answer(device, message, MealPlan { decoded, meal_plan }))
}

async fn update_feeder_meal_plan(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
    Json(body): Json<MealPlanRequest>,
) -> Result<Said<MealPlanUpdate>, AppError> {
    let encoded = feeder::encode(&body.meal_plan)?;
    let device = state
        .tuya
        .send_typed_command(
            &device_id,
            TuyaDeviceType::Feeder,
            dps::feeder::MEAL_PLAN,
            json!(encoded),
        )
        .await?;

    let message = format!("Meal plan updated for {}", device.name);
    Ok(answer(
        device,
        message,
        MealPlanUpdate {
            encoded_base64: encoded,
            formatted_meal_plan: feeder::describe(&body.meal_plan),
        },
    ))
}

async fn update_litter_box_settings(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
    Json(body): Json<LitterBoxSettingsRequest>,
) -> Result<Said<LitterSettingsUpdate>, AppError> {
    let updates = litter_box_updates(body)?;
    let updated_settings = updates.len();
    let device = state
        .tuya
        .send_typed_commands(&device_id, TuyaDeviceType::LitterBox, updates)
        .await?;

    let message = format!("Settings updated for {}", device.name);
    Ok(answer(
        device,
        message,
        LitterSettingsUpdate { updated_settings },
    ))
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
    let clock = |time: Option<String>| {
        time.map(|time| {
            minutes_of_day(&time)
                .map(|minutes| json!(minutes))
                .ok_or_else(|| refused(Refusal::Invalid, format!("time {time:?}")))
        })
        .transpose()
    };

    if let Some(clean_delay) = body.clean_delay {
        if clean_delay > LITTER_MAX_CLEAN_DELAY_SECONDS {
            return Err(refused(
                Refusal::Invalid,
                format!("clean delay {clean_delay} s"),
            ));
        }
        push(CLEAN_DELAY, Some(json!(clean_delay)));
    }

    if let Some(sleep_mode) = body.sleep_mode {
        push(SLEEP_ENABLED, sleep_mode.enabled.map(Value::Bool));
        push(SLEEP_START, clock(sleep_mode.start_time)?);
        push(SLEEP_END, clock(sleep_mode.end_time)?);
    }

    if let Some(preferences) = body.preferences {
        push(CHILD_LOCK, preferences.child_lock.map(Value::Bool));
        push(KITTEN_MODE, preferences.kitten_mode.map(Value::Bool));
        push(LIGHTING, preferences.lighting.map(Value::Bool));
        push(PROMPT_SOUND, preferences.prompt_sound.map(Value::Bool));
        push(
            AUTOMATIC_HOMING,
            preferences.automatic_homing.map(Value::Bool),
        );
    }

    if let Some(actions) = body.actions {
        if actions.reset_factory_settings == Some(true) {
            return Err(refused(
                Refusal::Forbidden,
                "a factory reset is an admin's (POST …/litter-box/factory-reset)",
            ));
        }
        push(
            RESET_SAND_LEVEL,
            (actions.reset_sand_level == Some(true)).then_some(json!(true)),
        );
    }

    if updates.is_empty() {
        return Err(refused(Refusal::Invalid, "no setting to change"));
    }
    Ok(updates)
}

async fn update_fountain_uv(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
    Json(body): Json<FountainUvSettingsRequest>,
) -> Result<Said<FountainUvUpdate>, AppError> {
    let mut updates = Vec::new();
    if let Some(enabled) = body.enabled {
        updates.push((dps::fountain::UV.to_string(), Value::Bool(enabled)));
    }
    if let Some(runtime) = body.runtime {
        if runtime > FOUNTAIN_MAX_UV_RUNTIME_HOURS {
            return Err(refused(Refusal::Invalid, format!("UV runtime {runtime} h")));
        }
        updates.push((dps::fountain::UV_RUNTIME.to_string(), json!(runtime)));
    }
    if updates.is_empty() {
        return Err(refused(Refusal::Invalid, "no setting to change"));
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
    Ok(answer(
        device,
        message,
        FountainUvUpdate { applied_settings },
    ))
}

async fn update_fountain_eco_mode(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
    Json(body): Json<FountainEcoModeRequest>,
) -> Result<Said<FountainEcoModeUpdate>, AppError> {
    if !(1..=2).contains(&body.mode) {
        return Err(refused(Refusal::Invalid, format!("eco mode {}", body.mode)));
    }

    let device = state
        .tuya
        .send_typed_command(
            &device_id,
            TuyaDeviceType::Fountain,
            dps::fountain::ECO_MODE,
            json!(body.mode),
        )
        .await?;

    let message = format!("Eco mode set to {} for {}", body.mode, device.name);
    Ok(answer(
        device,
        message,
        FountainEcoModeUpdate {
            eco_mode: body.mode,
        },
    ))
}

async fn update_fountain_power(
    State(state): State<AppState>,
    Path(device_id): DeviceId,
    Json(body): Json<FountainPowerRequest>,
) -> Result<Said<FountainPowerUpdate>, AppError> {
    let device = state
        .tuya
        .send_typed_command(
            &device_id,
            TuyaDeviceType::Fountain,
            dps::fountain::POWER,
            Value::Bool(body.enabled),
        )
        .await?;

    let message = format!(
        "Light {} for {}",
        if body.enabled {
            "turned on"
        } else {
            "turned off"
        },
        device.name
    );
    Ok(answer(
        device,
        message,
        FountainPowerUpdate {
            power: body.enabled,
        },
    ))
}

fn default_feeder_portion() -> u8 {
    1
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
    fn scan_range_defaults_and_rejects_bad_bounds() {
        assert_eq!(scan_range(None, None).unwrap(), (1, 255));
        assert_eq!(
            scan_range(Some("100"), Some("4294967295")).unwrap(),
            (100, u32::MAX)
        );
        for (start, end) in [
            (Some("0"), None),
            (Some("9"), Some("3")),
            (Some("x"), None),
            (None, Some("-1")),
        ] {
            assert!(matches!(
                scan_range(start, end),
                Err(AppError::Coded {
                    code: "invalid",
                    ..
                })
            ));
        }
    }

    #[test]
    fn dps_in_range_walks_the_reported_points_only() {
        let raw = json!({ "1": true, "101": "abc", "300": 4, "x": 1 })
            .as_object()
            .cloned()
            .unwrap();
        let found = dps_in_range(&raw, 1, u32::MAX);
        assert_eq!(found.keys().collect::<Vec<_>>(), vec!["1", "101", "300"]);
        assert_eq!(found["101"].r#type, "string");
        assert_eq!(found["101"].length, Some(3));
        assert_eq!(found["1"].r#type, "boolean");
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
        let code = |body| match litter(body) {
            Err(AppError::Coded { code, .. }) => code,
            other => panic!("{other:?}"),
        };
        assert_eq!(code(json!({})), "invalid");
        assert_eq!(
            code(json!({ "actions": { "resetFactorySettings": false } })),
            "invalid"
        );
        assert_eq!(
            code(json!({ "actions": { "resetFactorySettings": true } })),
            "forbidden",
            "a factory reset is an admin's own route"
        );
        assert!(litter(json!({ "cleanDelay": 1801 })).is_err());
        assert!(litter(json!({ "sleepMode": { "startTime": "25:00" } })).is_err());
    }
}
