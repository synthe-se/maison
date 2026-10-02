//! Regression tests for the /api/ir machine route: token auth (constant-time
//! bearer compare, fail-closed) and key-event dispatch semantics.

mod common;

use axum::http::{Method, StatusCode};
use maison_backend::config::Config;
use serde_json::{Value, json};

fn test_app(ir_keymap_path: std::path::PathBuf) -> axum::Router {
    let temp_root = common::temp_root("maison-rust-ir-tests");
    common::app(Config {
        device_cache_path: temp_root.join("device-cache.json"),
        broadlink_codes_path: temp_root.join("broadlink-codes.json"),
        climate_state_path: temp_root.join("climate-state.json"),
        refresh_tokens_path: temp_root.join("refresh-tokens.json"),
        nabaztag_config_path: temp_root.join("nabaztag.json"),
        ir_keymap_path,
        tv_config_path: temp_root.join("tv.json"),
        androidtv_config_path: temp_root.join("androidtv.json"),
        adb_key_path: temp_root.join("adb-key"),
        atv_identity_path: temp_root.join("atv-identity"),
        ..common::test_config()
    })
}

async fn post_key(app: &axum::Router, token: Option<&str>, body: Value) -> (StatusCode, Value) {
    common::send(app, Method::POST, "/api/ir/key", token, Some(body)).await
}

/// Path to a keymap file that does not exist: empty keymap, feature loads fine.
fn missing_keymap() -> std::path::PathBuf {
    common::temp_root("maison-rust-ir-tests").join("missing.json")
}

#[tokio::test]
async fn rejects_missing_token() {
    let app = test_app(missing_keymap());
    let (status, body) = post_key(&app, None, json!({"code": 1, "value": 1})).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["success"], json!(false));
}

#[tokio::test]
async fn rejects_wrong_token() {
    let app = test_app(missing_keymap());
    let (status, _) = post_key(&app, Some("wrong-token"), json!({"code": 1, "value": 1})).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn rejects_user_jwt_on_machine_route() {
    // A valid JWT is still not the machine token: the two auth paths are
    // deliberately not chained.
    let app = test_app(missing_keymap());
    let jwt = common::test_token();
    let (status, _) = post_key(&app, Some(&jwt), json!({"code": 1, "value": 1})).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn unmapped_key_is_a_200() {
    let app = test_app(missing_keymap());
    let (status, body) = post_key(&app, Some(common::TEST_IR_TOKEN), json!({"code": 999, "value": 1})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], json!(true));
    assert!(
        body["message"].as_str().unwrap_or_default().contains("not mapped"),
        "unexpected message: {body}"
    );
}

#[tokio::test]
async fn release_and_repeat_are_ignored_without_repeat_flag() {
    let keymap = common::temp_root("maison-rust-ir-tests").join("ir-keymap.json");
    // Nabaztag action with no NABAZTAG_HOST configured: firing it would error,
    // proving values 0 and 2 never reach the action.
    std::fs::write(
        &keymap,
        r#"{ "207": { "actions": [{ "action": "nabaztag", "command": "dance 1" }] } }"#,
    )
    .expect("keymap written");
    let app = test_app(keymap);

    for value in [0, 2] {
        let (status, body) =
            post_key(&app, Some(common::TEST_IR_TOKEN), json!({"code": 207, "value": value})).await;
        assert_eq!(status, StatusCode::OK, "value {value} should be ignored");
        assert!(
            body["message"].as_str().unwrap_or_default().contains("ignored"),
            "value {value}: unexpected message {body}"
        );
    }
}
