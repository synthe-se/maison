//! The event stream (`GET /api/events`): someone signed in only; each change said by its name.

mod common;

use axum::body::Body;
use axum::http::{Method, StatusCode};
use futures::StreamExt;
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn signed_out_there_is_no_stream() {
    let app = common::app(common::isolated_config("maison-events-out"));
    let (status, body) = common::send(&app, Method::GET, "/api/events", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], json!("not_signed_in"));
}

#[tokio::test]
async fn a_key_of_the_remote_is_said_on_the_stream() {
    let config = common::isolated_config("maison-events-ir");
    std::fs::write(
        &config.ir_keymap_path,
        r#"{ "207": { "actions": [{ "action": "nabaztag", "command": "dance 1" }] } }"#,
    )
    .unwrap();
    let app = common::app(config);
    let response = app
        .clone()
        .oneshot(common::request(
            Method::GET,
            "/api/events",
            Some(&common::test_token()),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"].to_str().unwrap(),
        "text/event-stream"
    );
    let mut body = Body::into_data_stream(response.into_body());

    // the remote's bridge posts a key: the stream says the keys changed
    let (status, _) = common::send(
        &app,
        Method::POST,
        "/api/ir/key",
        Some(common::TEST_IR_TOKEN),
        Some(json!({"code": 207, "value": 1})),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "taken, its actions run after the answer"
    );
    let mut said = String::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while !said.contains("data: ir:recent") {
        let chunk = tokio::time::timeout_at(deadline, body.next())
            .await
            .expect("said within five seconds")
            .expect("the stream stays open")
            .unwrap();
        said.push_str(&String::from_utf8_lossy(&chunk));
    }
    assert!(said.contains("event: change"), "{said}");
}
