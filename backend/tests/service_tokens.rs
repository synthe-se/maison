//! Service tokens: a machine (a companion app) drives the devices with a bearer token made
//! from the CLI, from the LAN too, and never reaches people, passkeys, invitations nor
//! settings.

mod common;

use std::{net::SocketAddr, sync::Arc};

use axum::{
    extract::connect_info::MockConnectInfo,
    http::{Method, StatusCode},
};
use maison_backend::{
    auth::issue_service_token,
    config::Config,
    people::{People, Scope},
};
use serde_json::json;

/// A test config and a fresh `devices` token for « achi » in its people file.
async fn config_with_token() -> (Config, String) {
    let config = Config { public_url: Some("https://maison.example.com".into()), ..common::isolated_config("maison-tokens") };
    let people = People::new(&config.auth_path);
    let token = issue_service_token(&people, "achi", Scope::Devices).await.unwrap().expect("a new name");
    (config, token)
}

#[tokio::test]
async fn a_service_token_drives_the_devices() {
    let (config, token) = config_with_token().await;
    let app = common::app(config);
    let reads = ["/api/meross", "/api/zigbee/lamps", "/api/hue-lamps", "/api/matter/covers", "/api/scenes", "/api/broadlink/mitsubishi/state"];
    for path in reads {
        let (s, v) = common::send(&app, Method::GET, path, Some(&token), None).await;
        assert_eq!(s, StatusCode::OK, "{path}: {v}");
    }
    // a command reaches its handler (here an unknown lamp: the manager's 404, not a refusal)
    let (s, _) = common::send(&app, Method::POST, "/api/zigbee/lamps/nope/power", Some(&token), Some(json!({ "enabled": true }))).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_service_token_never_administers() {
    let (config, token) = config_with_token().await;
    let app = common::app(config);
    for (method, path, body) in [
        (Method::GET, "/api/people", None),
        (Method::DELETE, "/api/people/alex", None),
        (Method::GET, "/api/invites", None),
        (Method::POST, "/api/invites", Some(json!({ "name": "Eve" }))),
        (Method::GET, "/api/passkeys", None),
        (Method::POST, "/api/auth/verify", None),
        (Method::POST, "/api/auth/logout-everywhere", None),
        (Method::GET, "/api/ir/keymap", None),
        (Method::POST, "/api/zigbee/lamps/pairing/start", None),
        (Method::PUT, "/api/scenes/x", Some(json!({ "name": "X", "icon": "x", "actions": [] }))),
        (Method::PUT, "/api/tv/config", Some(json!({}))),
    ] {
        let (s, v) = common::send(&app, method, path, Some(&token), body).await;
        assert_eq!((s, v["code"].as_str()), (StatusCode::FORBIDDEN, Some("forbidden")), "{path}");
    }
    let (s, v) = common::send(&app, Method::GET, "/api/broadlink/discover?forceRefresh=true", Some(&token), None).await;
    assert_eq!((s, v["code"].as_str()), (StatusCode::FORBIDDEN, Some("forbidden")), "a network scan is setting up");
}

#[tokio::test]
async fn an_unknown_or_revoked_token_is_not_signed_in_and_its_use_is_dated() {
    let (config, token) = config_with_token().await;
    let people = People::new(&config.auth_path);
    let app = common::app(config);
    let forged = format!("maison_{}", "A".repeat(43));
    let (s, v) = common::send(&app, Method::GET, "/api/meross", Some(&forged), None).await;
    assert_eq!((s, v["code"].as_str()), (StatusCode::UNAUTHORIZED, Some("not_signed_in")));

    assert_eq!(people.tokens().await.unwrap()[0].last_used_ms, None);
    assert_eq!(common::send(&app, Method::GET, "/api/meross", Some(&token), None).await.0, StatusCode::OK);
    assert!(people.tokens().await.unwrap()[0].last_used_ms.is_some(), "the use is recorded");

    assert!(people.revoke_token("achi").await.unwrap());
    let (s, v) = common::send(&app, Method::GET, "/api/meross", Some(&token), None).await;
    assert_eq!((s, v["code"].as_str()), (StatusCode::UNAUTHORIZED, Some("not_signed_in")), "revoked at once");
}

#[tokio::test]
async fn from_the_lan_a_service_token_reaches_the_devices_and_a_session_does_not() {
    let (config, token) = config_with_token().await;
    let app = maison_backend::build_app_parts_from_config(Arc::new(config))
        .unwrap()
        .0
        .layer(MockConnectInfo(SocketAddr::from(([100, 64, 0, 7], 4000))));
    let (s, v) = common::send(&app, Method::GET, "/api/meross", Some(&token), None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (s, _) = common::send(&app, Method::GET, "/api/meross", Some(&common::test_token()), None).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "a person's session: the tunnel only");
    let (s, v) = common::send(&app, Method::GET, "/api/people", Some(&token), None).await;
    assert_eq!((s, v["error"].as_str()), (StatusCode::FORBIDDEN, Some("Use the public address")));
}

#[tokio::test]
async fn people_still_sign_in_as_before() {
    let (config, _) = config_with_token().await;
    let app = common::app(config);
    assert_eq!(common::send(&app, Method::GET, "/api/meross", Some(&common::member_token()), None).await.0, StatusCode::OK);
    let (s, v) = common::send(&app, Method::GET, "/api/meross", None, None).await;
    assert_eq!((s, v["code"].as_str()), (StatusCode::UNAUTHORIZED, Some("not_signed_in")));
}
