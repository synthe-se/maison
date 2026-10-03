//! The lamp routes (Hue Bluetooth and Zigbee) as the web app calls them: the shared shapes,
//! and which ones only an admin may use.

mod common;

use axum::http::{Method, StatusCode};
use common::{app, isolated_config, member_token, send, test_token};
use serde_json::json;

#[tokio::test]
async fn both_families_answer_with_the_same_shapes() {
    let app = app(isolated_config("maison-lamp-routes"));
    let member = member_token();

    let (status, body) = send(&app, Method::GET, "/api/zigbee/lamps", Some(&member), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!({"success": true, "lamps": [], "total": 0, "connected": 0, "reachable": 0, "message": "Zigbee lamps list retrieved"})
    );
    let (status, body) = send(&app, Method::GET, "/api/hue-lamps", Some(&member), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["message"], "Hue lamps list retrieved");
    assert_eq!(body["total"], 0);

    let (status, body) = send(&app, Method::GET, "/api/hue-lamps/stats", Some(&member), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    for field in ["success", "total", "connected", "reachable", "disabled", "message"] {
        assert!(body.get(field).is_some(), "{field} missing: {body}");
    }
    assert_eq!(body["disabled"], true);

    let (status, body) = send(&app, Method::GET, "/api/zigbee/lamps/nope", Some(&member), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({"success": false, "error": "Zigbee lamp not found"}));
    let (status, body) = send(&app, Method::GET, "/api/hue-lamps/aabbccddeeff", Some(&member), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"], "Hue lamp not found");

    let (status, _) = send(&app, Method::GET, "/api/zigbee/lamps/pairing/status", Some(&member), None).await;
    assert_eq!(status, StatusCode::OK, "a static route is not taken for a lamp id");

    let (status, body) =
        send(&app, Method::POST, "/api/zigbee/lamps/nope/rename", Some(&member), Some(json!({"name": "  "}))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, _) =
        send(&app, Method::POST, "/api/zigbee/lamps/nope/rename", Some(&member), Some(json!({"name": "Salon"}))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn pairing_scanning_and_blacklisting_are_for_admins() {
    let app = app(isolated_config("maison-lamp-routes"));
    let member = member_token();
    let admin = test_token();
    let admin_only = [
        "/api/zigbee/lamps/pairing/start",
        "/api/zigbee/lamps/pairing/stop",
        "/api/zigbee/lamps/pairing/touchlink",
        "/api/hue-lamps/scan",
        "/api/hue-lamps/aabbccddeeff/blacklist",
    ];
    for path in admin_only {
        let (status, body) = send(&app, Method::POST, path, Some(&member), None).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}: {body}");
        let (status, body) = send(&app, Method::POST, path, Some(&admin), None).await;
        assert_ne!(status, StatusCode::FORBIDDEN, "{path}: {body}");
    }
    let (status, _) = send(&app, Method::POST, "/api/hue-lamps/scan", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// An unknown lamp is a 404 that never reaches the radio: a member posting to it again and
/// again must not have the driver count failures (five of them used to rebuild the EZSP
/// pipeline and take every lamp offline).
#[tokio::test]
async fn commands_to_an_unknown_zigbee_lamp_are_404() {
    let app = app(isolated_config("maison-lamp-routes"));
    let member = member_token();
    let commands = [
        ("power", json!({"enabled": true})),
        ("brightness", json!({"brightness": 50})),
        ("temperature", json!({"temperature": 50})),
        ("color", json!({"x": 0.3, "y": 0.3})),
        ("effect", json!({"effect": "candle"})),
    ];
    for _ in 0..3 {
        for (command, body) in &commands {
            let path = format!("/api/zigbee/lamps/nope/{command}");
            let (status, answer) = send(&app, Method::POST, &path, Some(&member), Some(body.clone())).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{path}: {answer}");
            assert_eq!(answer["error"], "Zigbee lamp not found");
        }
    }
    let (status, answer) =
        send(&app, Method::POST, "/api/zigbee/lamps/nope/effect", Some(&member), Some(json!({"effect": "disco"}))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "an unknown effect is refused first: {answer}");
}
