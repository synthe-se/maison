mod common;

use axum::http::{Method, StatusCode};
use serde_json::{json, Value};

#[tokio::test]
async fn auth_verify_accepts_valid_token() {
    let rust = request_rust(Method::POST, "/api/auth/verify", None, Some(common::test_token())).await;

    assert_eq!(rust.0, StatusCode::OK);
    assert_eq!(
        normalize_verify_response(rust.1),
        json!({
            "success": true,
            "user": {
                "id": "1",
                "name": "Léonard",
                "role": "admin"
            }
        })
    );
}

#[tokio::test]
async fn auth_logout_returns_success_message() {
    let rust = request_rust(Method::POST, "/api/auth/logout", None, None).await;

    assert_eq!(rust.0, StatusCode::OK);
    assert_eq!(
        rust.1,
        json!({
            "success": true,
            "message": "Logged out successfully"
        })
    );
}

async fn request_rust(
    method: Method,
    path: &str,
    body: Option<Value>,
    token: Option<String>,
) -> (StatusCode, Value) {
    common::send(&common::app(common::test_config()), method, path, token.as_deref(), body).await
}

#[tokio::test]
async fn auth_refresh_without_cookie_returns_unauthorized() {
    let rust = request_rust(Method::POST, "/api/auth/refresh", None, None).await;

    assert_eq!(rust.0, StatusCode::UNAUTHORIZED);
    assert_eq!(
        rust.1,
        json!({
            "success": false,
            "error": "No refresh token"
        })
    );
}

#[tokio::test]
async fn auth_verify_without_token_returns_unauthorized() {
    let rust = request_rust(Method::POST, "/api/auth/verify", None, None).await;

    assert_eq!(rust.0, StatusCode::UNAUTHORIZED);
}

fn normalize_verify_response(value: Value) -> Value {
    json!({
        "success": value.get("success").cloned().unwrap_or(Value::Null),
        "user": value.get("user").cloned().unwrap_or(Value::Null),
    })
}

#[tokio::test]
async fn auth_password_login_is_gone() {
    let body = json!({ "username": "leonard", "password": "anything" });
    let rust = request_rust(Method::POST, "/api/auth/login", Some(body), None).await;

    assert_ne!(rust.0, StatusCode::OK, "passkeys only");
}
