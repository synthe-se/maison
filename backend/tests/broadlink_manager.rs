mod common;

use axum::http::{Method, StatusCode};
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
    common::app(common::isolated_config("maison-rust-broadlink-tests"))
}

/// Setting the blasters up is for admins; the climate tile (cached list, codes, sending)
/// is everyday use.
#[tokio::test]
async fn setting_up_blasters_is_admin_only() {
    let app = test_app();
    let member = common::member_token();
    for (method, path, body) in [
        (Method::GET, "/api/broadlink/discover?forceRefresh=true", None),
        (Method::GET, "/api/broadlink/discover?localIp=192.168.1.10", None),
        (Method::POST, "/api/broadlink/provision", Some(json!({"ssid": "x", "securityMode": "wpa2"}))),
        (Method::POST, "/api/broadlink/learn/ir", Some(json!({"host": "192.168.1.73"}))),
        (Method::POST, "/api/broadlink/send", Some(json!({"host": "192.168.1.73", "packetBase64": "AQ=="}))),
        (Method::POST, "/api/broadlink/codes", Some(json!({"name": "n", "command": "c", "packetBase64": "AQ=="}))),
    ] {
        let (status, answer) = common::send(&app, method.clone(), path, Some(&member), body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}: {answer}");
        assert_eq!(answer["code"], json!("forbidden"));
    }
    let (status, _) = common::send(&app, Method::GET, "/api/broadlink/codes", Some(&member), None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = common::send(&app, Method::GET, "/api/broadlink/mitsubishi/state", Some(&member), None).await;
    assert_eq!(status, StatusCode::OK);
}

/// A blaster address is a LAN device or nothing: no request leaves for anything else.
#[tokio::test]
async fn blaster_addresses_outside_the_lan_are_refused() {
    let app = test_app();
    for host in ["127.0.0.1", "8.8.8.8", "169.254.169.254", "blaster.local", "192.168.1.73:80"] {
        let (status, body) = common::send_authed(
            &app,
            Method::POST,
            "/api/broadlink/send",
            Some(json!({"host": host, "packetBase64": "JgAAAA=="})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{host}: {body}");
    }
}
