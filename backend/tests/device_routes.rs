//! The TV, the Android box and the rabbit over HTTP: who may configure them (admins),
//! and which addresses a configuration may hold (LAN devices only — no SSRF). Nothing
//! here reaches a device: every refused request stops before the network, and nothing
//! reads a status once an address is saved.

mod common;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

fn test_app() -> axum::Router {
    common::app(common::isolated_config("maison-rust-device-routes"))
}

#[tokio::test]
async fn configuring_devices_is_admin_only() {
    let app = test_app();
    let member = common::member_token();
    for (method, path, body) in [
        (Method::PUT, "/api/tv/config", Some(json!({"host": "192.168.1.52"}))),
        (Method::PUT, "/api/androidtv/config", Some(json!({"host": "192.168.1.153"}))),
        (Method::POST, "/api/androidtv/pair/start", None),
        (Method::POST, "/api/androidtv/pair/finish", Some(json!({"code": "abcdef"}))),
        (Method::POST, "/api/androidtv/apk", None),
        (Method::PUT, "/api/nabaztag/config", Some(json!({"host": "192.168.1.40"}))),
    ] {
        let (status, answer) = common::send(&app, method.clone(), path, Some(&member), body.clone()).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}: {answer}");
        assert_eq!(answer["code"], json!("forbidden"));
        let (status, _) = common::send(&app, method.clone(), path, None, body).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path} signed out");
    }

    // members keep the everyday views
    for path in ["/api/tv", "/api/androidtv", "/api/nabaztag"] {
        let (status, body) = common::send(&app, Method::GET, path, Some(&member), None).await;
        assert_eq!(status, StatusCode::OK, "{path}: {body}");
    }
    let (_, body) = common::send(&app, Method::GET, "/api/nabaztag", Some(&member), None).await;
    assert_eq!(body["reachable"], json!(false));
    assert!(body.get("status").is_none(), "the rabbit's own page is never passed on: {body}");
}

async fn configure(app: &axum::Router, path: &str, body: Value) -> StatusCode {
    common::send_authed(app, Method::PUT, path, Some(body)).await.0
}

/// The addresses that would turn the backend into a proxy to the rest of the network.
const NOT_DEVICES: [&str; 9] = [
    "127.0.0.1",
    "localhost",
    "8.8.8.8",
    "169.254.169.254",
    "example.com",
    "192.168.1.2/status",
    "192.168.1.2#",
    "user@192.168.1.2",
    "http://192.168.1.2",
];

#[tokio::test]
async fn only_lan_devices_can_be_configured() {
    let app = test_app();
    for host in NOT_DEVICES {
        assert_eq!(configure(&app, "/api/nabaztag/config", json!({"host": host})).await, StatusCode::BAD_REQUEST, "rabbit {host}");
        assert_eq!(configure(&app, "/api/tv/config", json!({"host": host})).await, StatusCode::BAD_REQUEST, "tv {host}");
        assert_eq!(configure(&app, "/api/tv/config", json!({"boxHost": host})).await, StatusCode::BAD_REQUEST, "box {host}");
        assert_eq!(configure(&app, "/api/tv/config", json!({"irBlasterHost": host})).await, StatusCode::BAD_REQUEST, "blaster {host}");
        assert_eq!(configure(&app, "/api/androidtv/config", json!({"host": host})).await, StatusCode::BAD_REQUEST, "androidtv {host}");
    }
    // the DIAL app goes into a URL path
    for app_name in ["../x", "YouTube/../../admin", "a?b", "a b"] {
        let body = json!({"boxHost": "192.168.1.153", "boxWakeApp": app_name});
        assert_eq!(configure(&app, "/api/tv/config", body).await, StatusCode::BAD_REQUEST, "{app_name}");
    }

    let good = json!({"host": "192.168.1.52", "irBlasterHost": "192.168.1.73", "boxHost": "box.local", "boxWakeApp": "YouTube"});
    assert_eq!(configure(&app, "/api/tv/config", good).await, StatusCode::OK);
    assert_eq!(configure(&app, "/api/nabaztag/config", json!({"host": "rabbit.local", "tempoEnabled": false})).await, StatusCode::OK);
    assert_eq!(configure(&app, "/api/androidtv/config", json!({"host": "192.168.1.153", "port": 5555})).await, StatusCode::OK);
}

/// A second APK upload while one runs is refused before its bytes are read; this checks
/// the one an admin sends alone gets past the permit (to the missing box).
#[tokio::test]
async fn an_apk_upload_without_a_box_says_so() {
    use axum::body::Body;
    let app = test_app();
    let boundary = "maisonboundary";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"apk\"; filename=\"a.apk\"\r\nContent-Type: application/octet-stream\r\n\r\nPK\x03\x04rest\r\n--{boundary}--\r\n"
    );
    let request = axum::http::Request::builder()
        .method(Method::POST)
        .uri("/api/androidtv/apk")
        .header("Authorization", format!("Bearer {}", common::test_token()))
        .header("Content-Type", format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(body))
        .expect("request");
    let (status, _, answer) = common::respond(&app, request).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{answer}");
    assert!(answer["error"].as_str().unwrap_or_default().contains("No Android TV box"), "{answer}");
}
