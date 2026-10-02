mod common;

use axum::http::{Method, StatusCode};
use maison_backend::config::Config;
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
                "username": "leonard",
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

#[tokio::test]
async fn auth_invalid_login_returns_unauthorized() {
    let body = json!({ "username": "nope", "password": "nope" });
    let rust = request_rust(Method::POST, "/api/auth/login", Some(body), None).await;

    assert_eq!(rust.0, StatusCode::UNAUTHORIZED);
    assert_eq!(
        rust.1,
        json!({
            "success": false,
            "error": "Invalid username or password"
        })
    );
}

#[tokio::test]
async fn auth_successful_login_returns_ok_and_sets_cookies() {
    let temp_dir = common::temp_root("maison-auth-tests");

    // Argon2id hash of "testpass123"
    let users_json = serde_json::to_string(&json!([{
        "id": "42",
        "username": "testuser",
        "password_hash": "$argon2id$v=19$m=65536,t=3,p=4$go6DSNTX5Epa+lj9oxCogw$gGKfcn6IJsTTpj9XGQ+prTlKRGUUAMwJ6mhnzaYkvGs",
        "role": "admin"
    }]))
    .unwrap();
    let users_path = temp_dir.join("users.json");
    std::fs::write(&users_path, &users_json).expect("users file should be written");

    let app = common::app(Config { users_path, ..common::test_config() });

    let body = json!({ "username": "testuser", "password": "testpass123" });
    let (status, headers, json) = common::respond(
        &app,
        common::request(Method::POST, "/api/auth/login", None, Some(body)),
    )
    .await;

    let set_cookie_headers: Vec<String> = headers
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .collect();

    assert_eq!(status, StatusCode::OK, "response body: {json}");
    assert_eq!(json.get("success").and_then(Value::as_bool), Some(true));
    assert_eq!(json.pointer("/user/id").and_then(Value::as_str), Some("42"));
    assert_eq!(json.pointer("/user/username").and_then(Value::as_str), Some("testuser"));

    // Both access and refresh cookies must be present.
    assert!(
        set_cookie_headers.iter().any(|c| c.starts_with("maison_session=")),
        "access token cookie missing, got: {set_cookie_headers:?}"
    );
    assert!(
        set_cookie_headers.iter().any(|c| c.starts_with("maison_refresh=")),
        "refresh token cookie missing, got: {set_cookie_headers:?}"
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
