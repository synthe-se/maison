//! What the devices say, read: their payloads merged into data points (DPS), and the data
//! points of each device type named for the web.

use chrono::{DateTime, Utc};
use crate::util::HOUSE_TZ;
use rust_async_tuyapi::{Payload, mesparse::Message};
use serde_json::{Map, Value, json};

use super::{TuyaDeviceType, dps};
use crate::error::AppError;

/// `"HH:MM"` (00:00 to 23:59) as hours and minutes.
pub fn parse_hhmm(value: &str) -> Result<(u8, u8), AppError> {
    let invalid = || AppError::bad_request("Invalid time format. Use HH:MM");
    let (hours, minutes) = value.split_once(':').ok_or_else(invalid)?;
    let hours = hours.parse::<u8>().map_err(|_| invalid())?;
    let minutes = minutes.parse::<u8>().map_err(|_| invalid())?;
    if hours > 23 || minutes > 59 {
        return Err(invalid());
    }
    Ok((hours, minutes))
}

/// `messages`' data points over `current`.
pub(super) fn merge_messages_into_dps(mut current: Map<String, Value>, messages: Vec<Message>) -> Map<String, Value> {
    for message in messages {
        merge_payload_dps(&mut current, message.payload);
    }
    current
}

/// A payload's data points over `merged`, whatever shape the protocol version gave it.
pub(super) fn merge_payload_dps(merged: &mut Map<String, Value>, payload: Payload) {
    let value = match payload {
        Payload::Struct(payload) => payload.dps.map(|dps| json!({ "dps": dps })),
        Payload::String(raw) => serde_json::from_str::<Value>(&raw).ok(),
        Payload::ControlNewStruct(payload) => serde_json::to_value(payload)
            .ok()
            .and_then(|value| value.get("data").cloned()),
        Payload::Raw(_) => None,
    };
    if let Some(object) = value.as_ref().and_then(|value| value.get("dps")).and_then(Value::as_object) {
        for (key, value) in object {
            merged.insert(key.clone(), value.clone());
        }
    }
}

pub(super) fn parse_device_data(device_type: TuyaDeviceType, dps: &Map<String, Value>) -> Value {
    match device_type {
        TuyaDeviceType::Feeder => parse_feeder_status(dps),
        TuyaDeviceType::LitterBox => parse_litter_status(dps),
        TuyaDeviceType::Fountain => parse_fountain_status(dps),
        TuyaDeviceType::Unknown => Value::Object(Map::new()),
    }
}

/// The feeder's last-meal report, `R:<left> C:<count> T:<unix seconds>`: every field
/// optional, in any order, separated by spaces.
fn parse_feeder_history(history: &str) -> Value {
    let field = |prefix: &str| history.split_whitespace().find_map(|part| part.strip_prefix(prefix));
    let timestamp = field("T:");
    let readable = timestamp
        .and_then(|t| t.parse::<i64>().ok())
        .and_then(|t| DateTime::<Utc>::from_timestamp(t, 0))
        .map(|t| t.with_timezone(&HOUSE_TZ).format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default();
    json!({
        "raw": history,
        "parsed": {
            "remaining": field("R:").unwrap_or_default(),
            "count": field("C:"),
            "timestamp": timestamp,
            "timestampReadable": readable,
        }
    })
}

fn parse_feeder_status(dps: &Map<String, Value>) -> Value {
    use dps::feeder::*;
    let read = |id: &str, default: Value| dps.get(id).cloned().unwrap_or(default);

    let history = dps.get(HISTORY).and_then(Value::as_str).map(parse_feeder_history);

    let feed_size = dps
        .get(FEED_SIZE)
        .and_then(Value::as_i64)
        .map(|size| format!("{size} portion{}", if size > 1 { "s" } else { "" }))
        .unwrap_or_else(|| "Unknown".to_string());

    let powered_by = match dps.get(POWER_MODE).and_then(Value::as_i64) {
        Some(0) => "AC Power".to_string(),
        Some(1) => "Battery".to_string(),
        Some(mode) => format!("Mode {mode}"),
        None => "Unknown".to_string(),
    };

    json!({
        "feeding": {
            "manualFeedEnabled": read(MANUAL_FEED_ENABLED, json!(true)),
            "lastFeedSize": feed_size,
            "lastFeedReport": read(FEED_REPORT, json!(0)),
            "quickFeedAvailable": read(QUICK_FEED, json!(false)),
        },
        "settings": {
            "soundEnabled": read(SOUND, json!(true)),
            "alexaFeedEnabled": read(ALEXA_FEED, json!(false)),
        },
        "system": {
            "faultStatus": dps.get(FAULT).and_then(Value::as_i64).unwrap_or_default() != 0,
            "poweredBy": powered_by,
            "ipAddress": read(IP_ADDRESS, json!("Unknown")),
        },
        "history": history,
    })
}

fn parse_litter_status(dps: &Map<String, Value>) -> Value {
    use dps::litter::*;
    let read = |id: &str, default: Value| dps.get(id).cloned().unwrap_or(default);
    let number = |id: &str| dps.get(id).and_then(Value::as_i64).unwrap_or_default();
    let clean_delay = number(CLEAN_DELAY);
    let start_minutes = number(SLEEP_START);
    let end_minutes = number(SLEEP_END);

    json!({
        "cleanDelay": {
            "seconds": clean_delay,
            "formatted": format_seconds(clean_delay),
        },
        "sleepMode": {
            "enabled": read(SLEEP_ENABLED, json!(false)),
            "startTimeMinutes": start_minutes,
            "startTimeFormatted": format_minutes(start_minutes),
            "endTimeMinutes": end_minutes,
            "endTimeFormatted": format_minutes(end_minutes),
        },
        "sensors": {
            "defecationDuration": read(DEFECATION_DURATION, json!(0)),
            "defecationFrequency": read(DEFECATION_FREQUENCY, json!(0)),
            "faultAlarm": read(FAULT_ALARM, json!(0)),
            "litterLevel": read(LITTER_LEVEL, json!("unknown")),
        },
        "system": {
            "state": read(STATE, json!("unknown")),
            "cleaningInProgress": read(CLEAN, json!(false)),
            "maintenanceRequired": read(MAINTENANCE, json!(false)),
        },
        "settings": {
            "lighting": read(LIGHTING, json!(false)),
            "childLock": read(CHILD_LOCK, json!(false)),
            "promptSound": read(PROMPT_SOUND, json!(false)),
            "kittenMode": read(KITTEN_MODE, json!(false)),
            "automaticHoming": read(AUTOMATIC_HOMING, json!(false)),
        },
    })
}

fn parse_fountain_status(dps: &Map<String, Value>) -> Value {
    use dps::fountain::*;
    let parsed = [
        (POWER, "power"),
        (WATER_TIME, "waterTime"),
        (FILTER_LIFE, "filterLife"),
        (PUMP_TIME, "pumpTime"),
        (WATER_RESET, "waterReset"),
        (FILTER_RESET, "filterReset"),
        (PUMP_RESET, "pumpReset"),
        (UV, "uv"),
        (UV_RUNTIME, "uvRuntime"),
        (WATER_LEVEL, "waterLevel"),
        (LOW_WATER, "lowWater"),
        (ECO_MODE, "ecoMode"),
        (ECO_WATERING_STATUS, "ecoWateringStatus"),
        (NO_WATER, "noWater"),
        (ASSOCIATED_CAMERA, "associatedCamera"),
        (MAC_ADDRESS, "macAddress"),
    ]
    .into_iter()
    .filter_map(|(id, field)| dps.get(id).map(|value| (field.to_string(), value.clone())))
    .collect();
    Value::Object(parsed)
}

fn format_seconds(seconds: i64) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn format_minutes(minutes: i64) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

#[cfg(test)]
mod tests {
    use rust_async_tuyapi::PayloadStruct;

    use super::*;

    fn map(value: Value) -> Map<String, Value> {
        value.as_object().cloned().unwrap()
    }

    #[test]
    fn times_of_day_are_checked() {
        assert_eq!(parse_hhmm("00:00").unwrap(), (0, 0));
        assert_eq!(parse_hhmm("08:30").unwrap(), (8, 30));
        assert_eq!(parse_hhmm("23:59").unwrap(), (23, 59));
        for bad in ["24:00", "12:60", "bad", "08", "08:30:00", "-1:00", "300:00"] {
            assert_eq!(parse_hhmm(bad).unwrap_err().to_string(), "Invalid time format. Use HH:MM", "{bad}");
        }
    }

    #[test]
    fn feeder_status_maps_its_data_points() {
        let parsed = parse_feeder_status(&map(json!({
            "101": 2, "105": 1, "14": 0, "103": false, "107": "10.0.0.2",
            "104": "R:3  C:5  T:1700000000",
        })));
        assert_eq!(parsed["feeding"]["lastFeedSize"], "2 portions");
        assert_eq!(parsed["feeding"]["manualFeedEnabled"], true);
        assert_eq!(parsed["settings"]["soundEnabled"], false);
        assert_eq!(parsed["system"]["poweredBy"], "Battery");
        assert_eq!(parsed["system"]["faultStatus"], false);
        assert_eq!(parsed["system"]["ipAddress"], "10.0.0.2");
        assert_eq!(parsed["history"]["parsed"]["remaining"], "3");
        assert_eq!(parsed["history"]["parsed"]["count"], "5");
        assert_eq!(parsed["history"]["parsed"]["timestamp"], "1700000000");

        let empty = parse_feeder_status(&Map::new());
        assert_eq!(empty["feeding"]["lastFeedSize"], "Unknown");
        assert_eq!(empty["system"]["poweredBy"], "Unknown");
        assert_eq!(empty["history"], Value::Null);
        assert_eq!(parse_feeder_status(&map(json!({ "101": 1, "105": 7 })))["feeding"]["lastFeedSize"], "1 portion");
        assert_eq!(parse_feeder_status(&map(json!({ "105": 7 })))["system"]["poweredBy"], "Mode 7");
    }

    /// The real feeder separates the fields with one space (the legacy parser wanted two,
    /// and read everything as « remaining »); the time is said in the house's time zone.
    #[test]
    fn the_feeder_history_is_read_field_by_field() {
        let history = parse_feeder_history("R:0 C:3 T:1773270006");
        assert_eq!(
            history["parsed"],
            json!({ "remaining": "0", "count": "3", "timestamp": "1773270006", "timestampReadable": "2026-03-12 00:00:06" })
        );
        let partial = parse_feeder_history("C:1");
        assert_eq!(partial["parsed"], json!({ "remaining": "", "count": "1", "timestamp": null, "timestampReadable": "" }));
        assert_eq!(parse_feeder_history("T:soon")["parsed"]["timestampReadable"], "");
    }

    #[test]
    fn litter_status_maps_its_data_points() {
        let parsed = parse_litter_status(&map(json!({
            "101": 125, "102": true, "103": 1290, "104": 420, "110": true, "112": "half", "109": "standby",
        })));
        assert_eq!(parsed["cleanDelay"], json!({ "seconds": 125, "formatted": "2:05" }));
        assert_eq!(parsed["sleepMode"]["enabled"], true);
        assert_eq!(parsed["sleepMode"]["startTimeFormatted"], "21:30");
        assert_eq!(parsed["sleepMode"]["endTimeFormatted"], "07:00");
        assert_eq!(parsed["settings"]["childLock"], true);
        assert_eq!(parsed["settings"]["kittenMode"], false);
        assert_eq!(parsed["sensors"]["litterLevel"], "half");
        assert_eq!(parsed["system"]["state"], "standby");
        assert_eq!(parse_litter_status(&Map::new())["sensors"]["litterLevel"], "unknown");
    }

    #[test]
    fn fountain_status_names_only_the_reported_points() {
        let parsed = parse_fountain_status(&map(json!({ "1": true, "4": 80, "102": 2, "999": "x" })));
        assert_eq!(parsed, json!({ "power": true, "filterLife": 80, "ecoMode": 2 }));
        assert_eq!(parse_device_data(TuyaDeviceType::Unknown, &map(json!({ "1": true }))), json!({}));
    }

    /// The legacy server's snake_case keys, camelCased as the API says them now.
    fn camel_keys(value: Value) -> Value {
        match value {
            Value::Object(entries) => Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| {
                        let mut words = key.split('_');
                        let first = words.next().unwrap_or_default().to_string();
                        let rest = words.map(|w| w[..1].to_uppercase() + &w[1..]);
                        (first + &rest.collect::<String>(), camel_keys(value))
                    })
                    .collect(),
            ),
            other => other,
        }
    }

    /// The parsers still answer what the legacy server answered for the same data points
    /// (captured from the real devices, `tests/fixtures/tuya`, keys then snake_case), but
    /// for the feeder's history, which the legacy server misread (see above).
    #[test]
    fn parsers_match_the_captured_legacy_answers() {
        let fixture = |text: &str| serde_json::from_str::<Value>(text).unwrap()["body"].clone();
        for (kind, text) in [
            (TuyaDeviceType::LitterBox, include_str!("../../tests/fixtures/tuya/litter-box-status.json")),
            (TuyaDeviceType::Fountain, include_str!("../../tests/fixtures/tuya/fountain-status.json")),
        ] {
            let body = fixture(text);
            let parsed = parse_device_data(kind, &map(body["raw_dps"].clone()));
            assert_eq!(crate::util::casing_offences(&parsed), Vec::<String>::new(), "{kind:?}");
            assert_eq!(parsed, camel_keys(body["parsed_status"].clone()), "{kind:?}");
        }
        let listed = fixture(include_str!("../../tests/fixtures/tuya/devices.json"));
        for device in listed["devices"].as_array().unwrap() {
            let kind: TuyaDeviceType = serde_json::from_value(device["type"].clone()).unwrap();
            let mut parsed = parse_device_data(kind, &map(device["last_data"]["dps"].clone()));
            assert_eq!(crate::util::casing_offences(&parsed), Vec::<String>::new(), "{kind:?}");
            let mut legacy = camel_keys(device["parsed_data"].clone());
            if let (Some(parsed), Some(legacy)) = (parsed.as_object_mut(), legacy.as_object_mut()) {
                parsed.remove("history");
                legacy.remove("history");
            }
            assert_eq!(parsed, legacy, "{kind:?}");
        }
    }

    #[test]
    fn payloads_of_every_shape_merge_their_data_points() {
        let mut merged = map(json!({ "1": "old", "2": 2 }));
        merge_payload_dps(
            &mut merged,
            Payload::Struct(PayloadStruct {
                gw_id: None,
                dev_id: "d".into(),
                uid: None,
                t: None,
                dp_id: None,
                dps: Some(json!({ "1": "new" })),
            }),
        );
        merge_payload_dps(&mut merged, Payload::String(r#"{"dps":{"3":true}}"#.into()));
        merge_payload_dps(&mut merged, Payload::String("not json".into()));
        merge_payload_dps(&mut merged, Payload::Raw(vec![1, 2, 3]));
        assert_eq!(Value::Object(merged), json!({ "1": "new", "2": 2, "3": true }));
    }

    #[test]
    fn format_helpers_pad_as_a_clock() {
        assert_eq!(format_seconds(65), "1:05");
        assert_eq!(format_minutes(65), "01:05");
    }
}
