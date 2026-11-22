//! The event stream (`GET /api/events`): someone signed in only; each change said by its name.

mod common;

use axum::http::{Method, StatusCode};
use serde_json::json;

#[tokio::test]
async fn signed_out_there_is_no_stream() {
    let app = common::app(common::isolated_config("maison-events-out"));
    let (status, body) = common::send(&app, Method::GET, "/api/events", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], json!("not_signed_in"));
}
