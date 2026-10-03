//! The Hue lamp routes the web app does not call are gone; blacklisting an unknown lamp
//! is a 404, not a `success: false`.

mod common;

use axum::http::{Method, StatusCode};
use common::{app, isolated_config, send, test_token};
use serde_json::json;

#[tokio::test]
async fn connecting_by_hand_is_no_route() {
    let app = app(isolated_config("maison-hue-routes"));
    let admin = test_token();
    for path in [
        "/api/hue-lamps/connect",
        "/api/hue-lamps/disconnect",
        "/api/hue-lamps/aabbccddeeff/connect",
        "/api/hue-lamps/aabbccddeeff/disconnect",
        "/api/hue-lamps/aabbccddeeff/state",
    ] {
        let (status, body) = send(&app, Method::POST, path, Some(&admin), Some(json!({"isOn": true}))).await;
        assert!(matches!(status, StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED), "{path}: {status} {body}");
    }
}

#[cfg(feature = "bluetooth")]
#[tokio::test]
async fn blacklisting_an_unknown_lamp_is_not_found() {
    let app = app(isolated_config("maison-hue-routes"));
    let (status, body) = send(&app, Method::POST, "/api/hue-lamps/nope/blacklist", Some(&test_token()), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["success"], false);
}
