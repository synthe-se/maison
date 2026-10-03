//! Regression tests for the /api/ir machine route: token auth (constant-time
//! bearer compare, fail-closed) and key-event dispatch semantics.

mod common;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

const DANCE: &str = r#"{ "207": { "actions": [{ "action": "nabaztag", "command": "dance 1" }] } }"#;

/// The app on a fresh root of its own, with `keymap` (JSON text) installed when given.
fn test_app(keymap: Option<&str>) -> axum::Router {
    let config = common::isolated_config("maison-rust-ir-tests");
    if let Some(keymap) = keymap {
        std::fs::write(&config.ir_keymap_path, keymap).expect("keymap written");
    }
    common::app(config)
}

async fn post_key(app: &axum::Router, token: Option<&str>, body: Value) -> (StatusCode, Value) {
    common::send(app, Method::POST, "/api/ir/key", token, Some(body)).await
}

#[tokio::test]
async fn rejects_missing_token() {
    let app = test_app(None);
    let (status, body) = post_key(&app, None, json!({"code": 1, "value": 1})).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["success"], json!(false));
}

#[tokio::test]
async fn rejects_wrong_token() {
    let app = test_app(None);
    let (status, _) = post_key(&app, Some("wrong-token"), json!({"code": 1, "value": 1})).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn rejects_user_jwt_on_machine_route() {
    // A valid JWT is still not the machine token: the two auth paths are
    // deliberately not chained.
    let app = test_app(None);
    let jwt = common::test_token();
    let (status, _) = post_key(&app, Some(&jwt), json!({"code": 1, "value": 1})).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn unmapped_key_is_a_200() {
    let app = test_app(None);
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
    // Nabaztag action with no NABAZTAG_HOST configured: firing it would error,
    // proving values 0 and 2 never reach the action.
    let app = test_app(Some(DANCE));

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

/// A mapped press is accepted at once and run afterwards: kird hanging up must not stop
/// a binding halfway (a TV power-on takes ~16 s).
#[tokio::test]
async fn a_mapped_press_is_accepted_before_its_actions_run() {
    let app = test_app(Some(DANCE));
    let (status, body) = post_key(&app, Some(common::TEST_IR_TOKEN), json!({"code": 207, "value": 1})).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(body["success"], json!(true));
    assert_eq!(body["results"], json!([]));

    // the phantom double right behind it is still filtered, synchronously
    let (status, body) = post_key(&app, Some(common::TEST_IR_TOKEN), json!({"code": 207, "value": 1})).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["message"].as_str().unwrap_or_default().contains("debounced"), "{body}");
}

/// The configurator is for admins; reading the keymap is not.
#[tokio::test]
async fn the_configurator_is_admin_only() {
    let app = test_app(Some(DANCE));
    let member = common::member_token();
    let binding = json!({ "actions": [{ "action": "nabaztag", "command": "ping" }] });
    let test = json!({ "actions": [{ "action": "nabaztag", "command": "ping" }] });
    for (method, path, body) in [
        (Method::PUT, "/api/ir/keymap/5", Some(binding.clone())),
        (Method::DELETE, "/api/ir/keymap/207", None),
        (Method::GET, "/api/ir/recent", None),
        (Method::POST, "/api/ir/test", Some(test.clone())),
    ] {
        let (status, answer) = common::send(&app, method.clone(), path, Some(&member), body.clone()).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}: {answer}");
        assert_eq!(answer["code"], json!("forbidden"));
        let (status, _) = common::send(&app, method.clone(), path, None, body).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path} signed out");
    }
    let (status, body) = common::send(&app, Method::GET, "/api/ir/keymap", Some(&member), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["keymap"]["207"].is_object(), "{body}");

    let (status, _) = common::send_authed(&app, Method::PUT, "/api/ir/keymap/5", Some(binding)).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = common::send_authed(&app, Method::GET, "/api/ir/recent", None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = common::send_authed(&app, Method::DELETE, "/api/ir/keymap/5", None).await;
    assert_eq!(status, StatusCode::OK);
}
