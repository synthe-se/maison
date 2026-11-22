//! The HTTP API speaks one casing: camelCase English keys. `common::respond` holds every
//! answer of every integration test to it (`util::casing_offences`); this binary walks the
//! answers no other test reads, and checks that a file written before the rename (the
//! scenes, snake_case) still loads and comes back camelCase.

mod common;

use axum::http::{Method, StatusCode};
use serde_json::json;

/// What a fresh house answers without reaching any device (no host configured anywhere).
const ANSWERS: &[(&str, &str)] = &[
    ("POST", "/api/auth/verify"),
    ("GET", "/api/passkeys"),
    ("GET", "/api/people"),
    ("GET", "/api/invites"),
    ("GET", "/api/devices"),
    ("GET", "/api/meross"),
    ("GET", "/api/hue-lamps"),
    ("GET", "/api/hue-lamps/stats"),
    ("GET", "/api/zigbee/lamps"),
    ("GET", "/api/zigbee/lamps/stats"),
    ("GET", "/api/zigbee/lamps/pairing/status"),
    ("GET", "/api/matter/covers"),
    ("GET", "/api/matter/place"),
    ("GET", "/api/tv"),
    ("GET", "/api/androidtv"),
    ("GET", "/api/nabaztag"),
    ("GET", "/api/scenes"),
    ("GET", "/api/broadlink/codes"),
    ("GET", "/api/broadlink/mitsubishi/codes"),
    ("GET", "/api/broadlink/mitsubishi/state"),
    ("GET", "/api"),
];

#[tokio::test]
async fn every_answer_is_camel_case() {
    let app = common::app(common::isolated_config("maison-api-casing"));
    for (method, path) in ANSWERS {
        let method = Method::from_bytes(method.as_bytes()).unwrap();
        // the casing itself is asserted inside `respond`
        let (status, body) = common::send_authed(&app, method, path, None).await;
        assert!(
            status.is_success() || status == StatusCode::SERVICE_UNAVAILABLE,
            "{path}: {status} {body}"
        );
    }
}

/// `scenes.json` was once written with snake_case fields: it loads, and the API answers it
/// camelCase.
#[tokio::test]
async fn old_snake_case_scenes_still_load() {
    let config = common::isolated_config("maison-api-casing-old-files");
    let actions = json!([
        { "action": "broadlink_code", "host": "192.0.2.1", "code_id": "tv-power" },
        { "action": "climate_toggle", "host": "192.0.2.1", "on_command": "state-cool-16-fan-4-vane-swing" },
        { "action": "tv_power", "state": "on", "switch_to_box": false },
        { "action": "androidtv_app", "package": "org.smarttube.beta", "ensure_tv_on": false }
    ]);
    let scenes = json!([{ "id": "cinema", "name": "Cinéma", "icon": "film", "actions": actions }]);
    std::fs::write(&config.scenes_path, scenes.to_string()).unwrap();
    let app = common::app(config);

    let (status, body) = common::send_authed(&app, Method::GET, "/api/scenes", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let answered = &body["scenes"][0]["actions"];
    assert_eq!(answered[0]["codeId"], "tv-power");
    assert_eq!(answered[1]["onCommand"], "state-cool-16-fan-4-vane-swing");
    assert_eq!(answered[2]["switchToBox"], false);
    assert_eq!(answered[3]["ensureTvOn"], false);
}
