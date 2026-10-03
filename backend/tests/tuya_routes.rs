//! The Tuya routes against the in-process app: authentication, roles, validation and the
//! answers for devices that are not reachable (no device on the network is contacted: the
//! test devices have no valid address).

mod common;

use axum::{
    Router,
    http::{Method, StatusCode},
};
use serde_json::{Value, json};

const FEEDER: &str = "feeder-1";
const LITTER: &str = "litter-1";
const FOUNTAIN: &str = "fountain-1";

fn device(id: &str, product_name: &str) -> Value {
    json!({
        "name": product_name,
        "id": id,
        "key": "0123456789abcdef",
        "category": "",
        "product_name": product_name,
        "ip": "no-address",
        "version": "3.4",
    })
}

/// The app over three test devices, the feeder's meal plan in the cache.
fn app() -> Router {
    let config = common::isolated_config("maison-tuya-routes");
    let devices = json!([device(FEEDER, "Smart Feeder"), device(LITTER, "Litter Box"), device(FOUNTAIN, "Pet Fountain")]);
    std::fs::write(&config.devices_path, devices.to_string()).unwrap();
    std::fs::write(&config.device_cache_path, json!({ FEEDER: { "1": "BQgeAgE=" } }).to_string()).unwrap();
    common::app(config)
}

async fn admin(app: &Router, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    common::send_authed(app, method, path, body).await
}

async fn member(app: &Router, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    common::send(app, method, path, Some(&common::member_token()), body).await
}

#[tokio::test]
async fn the_routes_need_a_signed_in_person() {
    let app = app();
    let (status, _) = common::send(&app, Method::GET, "/api/devices", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_list_shows_every_device_with_its_cached_values() {
    let app = app();
    let (status, body) = member(&app, Method::GET, "/api/devices", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 3);
    let devices = body["devices"].as_array().unwrap();
    let feeder = devices.iter().find(|device| device["id"] == FEEDER).unwrap();
    assert_eq!(feeder["type"], "feeder");
    assert_eq!(feeder["lastData"]["dps"]["1"], "BQgeAgE=");
    let types = devices.iter().map(|device| device["type"].as_str().unwrap()).collect::<Vec<_>>();
    assert!(types.contains(&"litter-box") && types.contains(&"fountain"));

    assert!(devices.iter().all(|device| device["connected"] == false));
}

/// Routes the web never called are gone.
#[tokio::test]
async fn the_unused_routes_are_gone() {
    let app = app();
    for (method, path) in [
        (Method::GET, "/api/devices/stats".to_string()),
        (Method::POST, "/api/devices/reconnect".to_string()),
        (Method::GET, format!("/api/devices/{FEEDER}/status")),
    ] {
        let (status, _) = member(&app, method, &path, None).await;
        assert!(status == StatusCode::NOT_FOUND || status == StatusCode::METHOD_NOT_ALLOWED, "{path}: {status}");
    }
}

#[tokio::test]
async fn the_dps_scan_is_for_admins_and_checks_its_range() {
    let app = app();
    let path = format!("/api/devices/{FEEDER}/scan-dps");
    let (status, body) = member(&app, Method::GET, &path, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "forbidden");

    let (status, body) = admin(&app, Method::GET, &format!("{path}?start=9&end=3"), None).await;
    assert_eq!((status, body["error"].as_str()), (StatusCode::BAD_REQUEST, Some("Invalid DPS scan range")));
    let (status, _) = admin(&app, Method::GET, &format!("{path}?start=abc"), None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = admin(&app, Method::GET, "/api/devices/nope/scan-dps", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // A huge range is fine (the reported points are walked): the offline device answers 503.
    let (status, _) = admin(&app, Method::GET, &format!("{path}?start=1&end=4294967295"), None).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn bad_requests_are_refused_before_reaching_the_device() {
    let app = app();
    let cases = [
        (format!("/api/devices/{FEEDER}/feeder/feed"), json!({ "portion": 0 }), "portion must be between 1 and 12"),
        (format!("/api/devices/{FEEDER}/feeder/meal-plan"), json!({ "mealPlan": [] }), "mealPlan array is required"),
        (
            format!("/api/devices/{FEEDER}/feeder/meal-plan"),
            json!({ "mealPlan": [{ "daysOfWeek": ["Monday"], "time": "25:00", "portion": 1, "status": "Enabled" }] }),
            "Invalid meal plan entry at index 0",
        ),
        (format!("/api/devices/{LITTER}/litter-box/settings"), json!({}), "No valid settings provided"),
        (
            format!("/api/devices/{LITTER}/litter-box/settings"),
            json!({ "cleanDelay": 5000 }),
            "cleanDelay must be between 0 and 1800 seconds",
        ),
        (format!("/api/devices/{FOUNTAIN}/fountain/eco-mode"), json!({ "mode": 3 }), "Eco mode must be 1 or 2"),
        (format!("/api/devices/{FOUNTAIN}/fountain/uv"), json!({ "runtime": 25 }), "UV runtime must be between 0 and 24 hours"),
        (format!("/api/devices/{FOUNTAIN}/fountain/uv"), json!({}), "No valid settings provided"),
    ];
    for (path, body, error) in cases {
        let (status, answer) = member(&app, Method::POST, &path, Some(body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}");
        assert_eq!(answer["error"], error, "{path}");
        assert_eq!(answer["success"], false);
    }
    // a day or status the feeder has no word for does not even read as a meal plan
    for meal in [
        json!({ "daysOfWeek": ["Funday"], "time": "08:00", "portion": 1, "status": "Enabled" }),
        json!({ "daysOfWeek": ["Monday"], "time": "08:00", "portion": 1, "status": "Maybe" }),
    ] {
        let path = format!("/api/devices/{FEEDER}/feeder/meal-plan");
        let (status, _) = member(&app, Method::POST, &path, Some(json!({ "mealPlan": [meal] }))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }
}

#[tokio::test]
async fn a_route_for_another_device_type_is_refused() {
    let app = app();
    let cases = [
        (Method::GET, format!("/api/devices/{LITTER}/feeder/status"), "Device is not a feeder"),
        (Method::GET, format!("/api/devices/{FEEDER}/fountain/status"), "Device is not a fountain"),
        (Method::POST, format!("/api/devices/{FOUNTAIN}/litter-box/clean"), "Device is not a litter box"),
        (Method::POST, format!("/api/devices/{LITTER}/fountain/reset/filter"), "Device is not a fountain"),
    ];
    for (method, path, error) in cases {
        let (status, answer) = member(&app, method, &path, None).await;
        assert_eq!((status, answer["error"].as_str()), (StatusCode::BAD_REQUEST, Some(error)), "{path}");
    }
    let (status, answer) = member(&app, Method::POST, "/api/devices/nope/fountain/reset/pump", None).await;
    assert_eq!((status, answer["error"].as_str()), (StatusCode::NOT_FOUND, Some("Device not found")));
}

#[tokio::test]
async fn commands_to_an_unreachable_device_fail_with_503() {
    let app = app();
    for path in [
        format!("/api/devices/{LITTER}/litter-box/clean"),
        format!("/api/devices/{FOUNTAIN}/fountain/reset/water"),
    ] {
        let (status, answer) = member(&app, Method::POST, &path, None).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{path}");
        assert!(answer["error"].as_str().unwrap().contains("not connected"), "{answer}");
    }
    let (status, _) = member(&app, Method::POST, &format!("/api/devices/{FOUNTAIN}/connect"), None).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn connections_can_be_dropped_and_started_again() {
    let app = app();
    let (status, body) = member(&app, Method::POST, "/api/devices/disconnect", None).await;
    assert_eq!((status, body), (StatusCode::OK, json!({ "success": true, "message": "All devices disconnected" })));
    let (status, body) = member(&app, Method::POST, &format!("/api/devices/{FEEDER}/disconnect"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["message"], format!("Device {FEEDER} disconnected"));
    let (status, _) = member(&app, Method::POST, "/api/devices/nope/disconnect", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
