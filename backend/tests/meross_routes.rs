//! The Meross routes against the in-process app. Nothing on the network is dialled: a plug's
//! address is checked when the list loads, and the test plugs' are not on the house's network
//! (the manager's own tests drive a fake plug on the loopback, `src/meross.rs`).

mod common;

use axum::{
    http::{Method, StatusCode},
    Router,
};
use serde_json::{json, Value};

/// Port 1 on the loopback: not a device address, so never dialled.
const LOOPBACK: &str = "127.0.0.1:1";

fn app(plugs: &[&str]) -> Router {
    let config = common::isolated_config("maison-meross-routes");
    let devices = plugs
        .iter()
        .map(|ip| json!({ "name": "Desk plug", "ip": ip, "key": "plug-key" }))
        .collect::<Vec<_>>();
    std::fs::write(
        &config.meross_devices_path,
        Value::from(devices).to_string(),
    )
    .unwrap();
    common::app(config)
}

async fn member(
    app: &Router,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    common::send(app, method, path, Some(&common::member_token()), body).await
}

#[tokio::test]
async fn a_plug_off_the_network_reads_offline_and_refuses_commands() {
    let app = app(&[LOOPBACK]);

    let (status, body) = member(&app, Method::GET, "/api/meross", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({ "success": true, "devices": [{ "id": LOOPBACK, "name": "Desk plug", "ip": LOOPBACK, "isOnline": false, "isOn": false, "lastPing": 0 }], "total": 1, "message": "Meross devices list retrieved" })
    );

    let (status, body) = member(
        &app,
        Method::GET,
        &format!("/api/meross/{LOOPBACK}/status"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["device"],
        json!({ "id": LOOPBACK, "name": "Desk plug" })
    );
    assert_eq!(body["status"]["online"], json!(false));
    assert_eq!(body["status"]["electricity"], json!(null));

    let (status, body) = member(
        &app,
        Method::POST,
        &format!("/api/meross/{LOOPBACK}/toggle"),
        Some(json!({ "on": true })),
    )
    .await;
    assert_eq!(
        (status, body["success"].as_bool()),
        (StatusCode::SERVICE_UNAVAILABLE, Some(false))
    );
    for path in ["electricity", "consumption"] {
        let (status, _) = member(
            &app,
            Method::GET,
            &format!("/api/meross/{LOOPBACK}/{path}"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{path}");
    }
    let (status, _) = member(
        &app,
        Method::POST,
        &format!("/api/meross/{LOOPBACK}/dnd"),
        Some(json!({ "enabled": true })),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn unknown_plugs_bad_bodies_and_strangers_are_refused() {
    let app = app(&[]);
    let (status, body) = member(
        &app,
        Method::POST,
        "/api/meross/10.0.0.1/toggle",
        Some(json!({ "on": true })),
    )
    .await;
    assert_eq!(
        (status, body["code"].as_str()),
        (StatusCode::NOT_FOUND, Some("not_found"))
    );
    let (status, _) = member(
        &app,
        Method::POST,
        "/api/meross/10.0.0.1/toggle",
        Some(json!({ "on": "yes" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = common::send(&app, Method::GET, "/api/meross", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // the separate on/off routes are gone: toggle says which
    let (status, _) = member(&app, Method::POST, "/api/meross/10.0.0.1/on", None).await;
    assert!(
        status == StatusCode::NOT_FOUND || status == StatusCode::METHOD_NOT_ALLOWED,
        "{status}"
    );
    let (status, _) = member(&app, Method::GET, "/api/meross/stats", None).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "the unused stats route is gone"
    );
}
