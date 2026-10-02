#![cfg(feature = "live-runtime-tests")]

mod common;

use axum::http::{Method, StatusCode};
use common::{assert_json_eq, normalize_numbers};
use serde_json::{json, Value};

const LEGACY_BASE_URL: &str = "http://localhost:3033";
const LEGACY_TOKEN: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJ1c2VySWQiOiIxIiwidXNlcm5hbWUiOiJsZW9uYXJkIiwicm9sZSI6ImFkbWluIiwiZXhwIjoxNzczODc0NTE1LCJpYXQiOjE3NzMyNjk3MTV9.iA2VDfv_KLmADqGHI-yXa2fPRom5LqfyKIT2mP3dh6g";
const DEVICE_ID: &str = "192.168.1.113";

#[tokio::test]
async fn meross_list_matches_legacy_contract() {
    let _guard = common::serial().lock().await;
    let rust = request_rust(Method::GET, "/api/meross", None).await;
    let legacy = request_legacy(Method::GET, "/meross", None).await;

    assert_eq!(rust.0, StatusCode::OK);
    assert_eq!(rust.0, legacy.0);
    assert_json_eq(&normalize_list(rust.1), &normalize_list(legacy.1));
}

#[tokio::test]
async fn meross_status_matches_legacy_contract() {
    let _guard = common::serial().lock().await;
    let rust = request_rust(Method::GET, &format!("/api/meross/{DEVICE_ID}/status"), None).await;
    let legacy = request_legacy(Method::GET, &format!("/meross/{DEVICE_ID}/status"), None).await;

    assert_eq!(rust.0, StatusCode::OK);
    assert_eq!(rust.0, legacy.0);
    assert_json_eq(&normalize_status(rust.1), &normalize_status(legacy.1));
}

#[tokio::test]
async fn meross_electricity_matches_legacy_contract() {
    let _guard = common::serial().lock().await;
    let rust = request_rust(Method::GET, &format!("/api/meross/{DEVICE_ID}/electricity"), None).await;
    let legacy = request_legacy(Method::GET, &format!("/meross/{DEVICE_ID}/electricity"), None).await;

    assert_eq!(rust.0, StatusCode::OK);
    assert_eq!(rust.0, legacy.0);
    assert_json_eq(&normalize_electricity(rust.1), &normalize_electricity(legacy.1));
}

#[tokio::test]
async fn meross_consumption_matches_legacy_contract() {
    let _guard = common::serial().lock().await;
    let rust = request_rust(Method::GET, &format!("/api/meross/{DEVICE_ID}/consumption"), None).await;
    let legacy = request_legacy(Method::GET, &format!("/meross/{DEVICE_ID}/consumption"), None).await;

    assert_eq!(rust.0, StatusCode::OK);
    assert_eq!(rust.0, legacy.0);
    assert_json_eq(&normalize_consumption(rust.1), &normalize_consumption(legacy.1));
}

#[tokio::test]
async fn meross_toggle_matches_legacy_contract() {
    let _guard = common::serial().lock().await;
    let body = json!({ "on": false });
    let rust = request_rust(Method::POST, &format!("/api/meross/{DEVICE_ID}/toggle"), Some(body.clone())).await;
    let legacy = request_legacy(Method::POST, &format!("/meross/{DEVICE_ID}/toggle"), Some(body)).await;

    assert_eq!(rust.0, StatusCode::OK);
    assert_eq!(rust.0, legacy.0);
    assert_json_eq(&normalize_toggle(rust.1), &normalize_toggle(legacy.1));
}

#[tokio::test]
async fn meross_turn_on_matches_legacy_contract() {
    let _guard = common::serial().lock().await;
    let rust = request_rust(Method::POST, &format!("/api/meross/{DEVICE_ID}/on"), None).await;
    let legacy = request_legacy(Method::POST, &format!("/meross/{DEVICE_ID}/on"), None).await;

    assert_eq!(rust.0, StatusCode::OK);
    assert_eq!(rust.0, legacy.0);
    assert_json_eq(&normalize_toggle(rust.1), &normalize_toggle(legacy.1));
}

#[tokio::test]
async fn meross_turn_off_matches_legacy_contract() {
    let _guard = common::serial().lock().await;
    let rust = request_rust(Method::POST, &format!("/api/meross/{DEVICE_ID}/off"), None).await;
    let legacy = request_legacy(Method::POST, &format!("/meross/{DEVICE_ID}/off"), None).await;

    assert_eq!(rust.0, StatusCode::OK);
    assert_eq!(rust.0, legacy.0);
    assert_json_eq(&normalize_toggle(rust.1), &normalize_toggle(legacy.1));
}

#[tokio::test]
async fn meross_dnd_matches_legacy_contract() {
    let _guard = common::serial().lock().await;
    let body = json!({ "enabled": true });
    let rust = request_rust(Method::POST, &format!("/api/meross/{DEVICE_ID}/dnd"), Some(body.clone())).await;
    let legacy = request_legacy(Method::POST, &format!("/meross/{DEVICE_ID}/dnd"), Some(body)).await;

    assert_eq!(rust.0, StatusCode::OK);
    assert_eq!(rust.0, legacy.0);
    assert_json_eq(&normalize_dnd(rust.1), &normalize_dnd(legacy.1));
}

async fn request_rust(method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    common::send_authed(&common::app(common::test_config()), method, path, body).await
}

async fn request_legacy(method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut request = reqwest::Client::new()
        .request(method, format!("{LEGACY_BASE_URL}{path}"))
        .bearer_auth(LEGACY_TOKEN);
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request
        .send()
        .await
        .expect("legacy request should succeed");
    let status = response.status();
    let json = response
        .json::<Value>()
        .await
        .expect("legacy response should be valid json");
    (status, json)
}

fn normalize_list(value: Value) -> Value {
    let devices = value
        .get("devices")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|device| {
            json!({
                "id": device.get("id").cloned().unwrap_or(Value::Null),
                "name": device.get("name").cloned().unwrap_or(Value::Null),
                "ip": device.get("ip").cloned().unwrap_or(Value::Null),
                "isOnline": device.get("isOnline").cloned().unwrap_or(Value::Null),
                "isOn": device.get("isOn").cloned().unwrap_or(Value::Null),
            })
        })
        .collect::<Vec<_>>();
    let mut devices = devices;
    devices.sort_by(|left, right| {
        left.get("id")
            .and_then(Value::as_str)
            .cmp(&right.get("id").and_then(Value::as_str))
    });

    normalize_numbers(json!({
        "success": value.get("success").cloned().unwrap_or(Value::Null),
        "devices": devices,
        "total": value.get("total").cloned().unwrap_or(Value::Null),
        "message": value.get("message").cloned().unwrap_or(Value::Null),
    }))
}

fn normalize_status(value: Value) -> Value {
    let status = value.get("status").cloned().unwrap_or(Value::Null);
    normalize_numbers(json!({
        "success": value.get("success").cloned().unwrap_or(Value::Null),
        "device": value.get("device").cloned().unwrap_or(Value::Null),
        "status": {
            "online": status.get("online").cloned().unwrap_or(Value::Null),
            "on": status.get("on").cloned().unwrap_or(Value::Null),
            "electricity": status.get("electricity").cloned().unwrap_or(Value::Null),
            "hardware": status.get("hardware").cloned().unwrap_or(Value::Null),
            "firmware": status.get("firmware").cloned().unwrap_or(Value::Null),
            "wifi": {
                "signal": status
                    .get("wifi")
                    .and_then(|wifi| wifi.get("signal"))
                    .map(|signal| if signal.is_null() { Value::Null } else { Value::String("present".to_string()) })
                    .unwrap_or(Value::Null),
            },
        },
        "message": value.get("message").cloned().unwrap_or(Value::Null),
    }))
}

fn normalize_electricity(value: Value) -> Value {
    normalize_numbers(json!({
        "success": value.get("success").cloned().unwrap_or(Value::Null),
        "device": value.get("device").cloned().unwrap_or(Value::Null),
        "electricity": value.get("electricity").cloned().unwrap_or(Value::Null),
        "message": value.get("message").cloned().unwrap_or(Value::Null),
    }))
}

fn normalize_consumption(value: Value) -> Value {
    normalize_numbers(json!({
        "success": value.get("success").cloned().unwrap_or(Value::Null),
        "device": value.get("device").cloned().unwrap_or(Value::Null),
        "consumption": value.get("consumption").cloned().unwrap_or(Value::Null),
        "summary": value.get("summary").cloned().unwrap_or(Value::Null),
        "message": value.get("message").cloned().unwrap_or(Value::Null),
    }))
}

fn normalize_toggle(value: Value) -> Value {
    normalize_numbers(json!({
        "success": value.get("success").cloned().unwrap_or(Value::Null),
        "device": value.get("device").cloned().unwrap_or(Value::Null),
        "on": value.get("on").cloned().unwrap_or(Value::Null),
        "message": value.get("message").cloned().unwrap_or(Value::Null),
    }))
}

fn normalize_dnd(value: Value) -> Value {
    normalize_numbers(json!({
        "success": value.get("success").cloned().unwrap_or(Value::Null),
        "device": value.get("device").cloned().unwrap_or(Value::Null),
        "dndMode": value.get("dndMode").cloned().unwrap_or(Value::Null),
        "message": value.get("message").cloned().unwrap_or(Value::Null),
    }))
}
