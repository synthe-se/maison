mod common;

use axum::http::{Method, StatusCode};
use maison_backend::config::Config;
use serde_json::{Value, json};

#[tokio::test]
async fn broadlink_codes_can_be_saved_and_listed() {
    let app = test_app();

    let save = common::send_authed(
        &app,
        Method::POST,
        "/api/broadlink/codes",
        Some(json!({
            "name": "Salon AC 22C",
            "brand": "Mitsubishi",
            "model": "MSZ-AP",
            "command": "cool_22_auto",
            "packetBase64": "AQIDBA==",
            "tags": ["salon", "clim"]
        })),
    )
    .await;

    assert_eq!(save.0, StatusCode::OK);
    assert_eq!(save.1.get("success").and_then(Value::as_bool), Some(true));
    assert_eq!(save.1.pointer("/code/command").and_then(Value::as_str), Some("cool_22_auto"));

    let list = common::send_authed(&app, Method::GET, "/api/broadlink/codes", None).await;
    assert_eq!(list.0, StatusCode::OK);
    assert_eq!(list.1.get("total").and_then(Value::as_u64), Some(1));
    assert_eq!(list.1.pointer("/codes/0/brand").and_then(Value::as_str), Some("mitsubishi"));
}

#[tokio::test]
async fn broadlink_mitsubishi_filter_returns_only_matching_brand() {
    let app = test_app();

    let _ = common::send_authed(
        &app,
        Method::POST,
        "/api/broadlink/codes",
        Some(json!({
            "name": "Salon AC 22C",
            "brand": "Mitsubishi",
            "model": "MSZ-AP",
            "command": "cool_22_auto",
            "packetBase64": "AQIDBA=="
        })),
    )
    .await;
    let _ = common::send_authed(
        &app,
        Method::POST,
        "/api/broadlink/codes",
        Some(json!({
            "name": "TV Power",
            "brand": "Sony",
            "command": "power",
            "packetBase64": "BQYHCA=="
        })),
    )
    .await;

    let response = common::send_authed(&app, Method::GET, "/api/broadlink/mitsubishi/codes", None).await;
    assert_eq!(response.0, StatusCode::OK);
    assert_eq!(response.1.get("total").and_then(Value::as_u64), Some(1));
    assert_eq!(response.1.pointer("/codes/0/brand").and_then(Value::as_str), Some("mitsubishi"));
}

fn test_app() -> axum::Router {
    let temp_root = common::temp_root("maison-rust-broadlink-tests");
    common::app(Config {
        broadlink_codes_path: temp_root.join("broadlink-codes.json"),
        climate_state_path: temp_root.join("climate-state.json"),
        refresh_tokens_path: temp_root.join("refresh-tokens.json"),
        ..common::test_config()
    })
}
