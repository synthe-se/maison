//! POST /api/matter/covers/{id}/skip: « not tonight » on one shutter, for every member.

mod common;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

const COVER: &str = "0000000000000001";

/// The app on a fresh root holding one shutter that follows the sun in Paris (no switch
/// behind it: the views are the kept ones).
fn test_app() -> axum::Router {
    let config = common::isolated_config("maison-rust-matter-skip-tests");
    std::fs::create_dir_all(&config.matter_state_dir).expect("matter dir");
    let covers = json!([{
        "nodeId": 1, "name": "Salon", "endpoint": 1, "vendorId": null, "productId": null,
        "schedule": { "openAtSunrise": true, "closeAtSunset": true, "sunriseOffsetMin": 0, "sunsetOffsetMin": 0 }
    }]);
    std::fs::write(config.matter_state_dir.join("covers.json"), covers.to_string()).expect("covers written");
    let place = json!({ "name": "Paris", "latitude": 48.8566, "longitude": 2.3522 });
    std::fs::write(config.matter_state_dir.join("place.json"), place.to_string()).expect("place written");
    common::app(config)
}

async fn skip(app: &axum::Router, id: &str, body: Value) -> (StatusCode, Value) {
    let member = common::member_token();
    common::send(app, Method::POST, &format!("/api/matter/covers/{id}/skip"), Some(&member), Some(body)).await
}

#[tokio::test]
async fn a_member_skips_the_next_closing_and_takes_it_back() {
    let app = test_app();
    let (status, body) = skip(&app, COVER, json!({ "event": "close", "skip": true })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["success"], json!(true));
    assert_eq!(body["cover"]["skipNextClose"], json!(true));
    assert_eq!(body["cover"]["skipNextOpen"], json!(false));
    assert!(body["cover"]["nextClose"].is_string(), "still the next scheduled time: {body}");

    let (_, list) = common::send_authed(&app, Method::GET, "/api/matter/covers", None).await;
    assert_eq!(list["covers"][0]["skipNextClose"], json!(true), "kept: {list}");

    let (status, body) = skip(&app, COVER, json!({ "event": "close", "skip": false })).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["cover"]["skipNextClose"], json!(false));
}

#[tokio::test]
async fn a_wrong_event_or_shutter_is_refused() {
    let app = test_app();
    for body in [json!({ "event": "sunset", "skip": true }), json!({ "event": "Open", "skip": true })] {
        let (status, answer) = skip(&app, COVER, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{answer}");
    }
    let (status, _) = skip(&app, "00000000000000ff", json!({ "event": "open", "skip": true })).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = common::send(&app, Method::POST, &format!("/api/matter/covers/{COVER}/skip"), None, Some(json!({ "event": "open", "skip": true }))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
