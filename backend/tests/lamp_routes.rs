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
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({"success": false, "lamp": null, "message": "Zigbee lamp not found"}));

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
