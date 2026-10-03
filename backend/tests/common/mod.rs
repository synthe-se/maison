//! What every integration test binary shares: the test configuration, the
//! token, the in-process app and the request round trip. Each binary uses a
//! subset, hence the `dead_code` allowance.
//!
//! A test never touches the repository: its configuration lives in a temp root of its
//! own (`isolated_config`), removed when the test ends.
#![allow(dead_code)]

use std::{cell::RefCell, net::SocketAddr, path::PathBuf, sync::Arc, sync::OnceLock};

use axum::{
    Router,
    body::{Body, to_bytes},
    extract::connect_info::MockConnectInfo,
    http::{HeaderMap, Method, Request, StatusCode},
};
use jsonwebtoken::{EncodingKey, Header, encode};
use maison_backend::{
    auth::Claims,
    build_app_parts_from_config,
    config::{self, Config},
};
use serde_json::{Value, json};
use tower::ServiceExt;

/// The machine token the test config accepts on `/api/ir`.
pub const TEST_IR_TOKEN: &str = "test-ir-token";

/// The repository root: the parent of the backend crate. Read only (the live tests'
/// device files, the vendored Tempo data): a test writes under `temp_root`.
pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend has parent")
        .to_path_buf()
}

thread_local! {
    /// This test's temp roots: each test runs on a thread of its own, and they go with it.
    static ROOTS: RefCell<Vec<tempfile::TempDir>> = const { RefCell::new(Vec::new()) };
}

/// A fresh, created directory `<tmp>/<name>-<random>`, removed when the test ends.
pub fn temp_root(name: &str) -> PathBuf {
    let dir = tempfile::Builder::new()
        .prefix(&format!("{name}-"))
        .tempdir()
        .expect("temp test dir should be created");
    let path = dir.path().to_path_buf();
    ROOTS.with(|roots| roots.borrow_mut().push(dir));
    path
}

/// `name` from the environment, else `default` under the repository root (the live tests,
/// on the real devices).
pub fn env_path(name: &str, default: &str) -> PathBuf {
    config::env_text(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root().join(default))
}

/// The secret the test config signs with and the test token is signed with.
pub fn jwt_secret() -> String {
    config::env_text("JWT_SECRET").unwrap_or_else(|| config::DEFAULT_JWT_SECRET.to_string())
}

/// The one test config: every default on a fresh root of its own (`temp_root`), where every
/// state file starts missing but the people (`TEST_PEOPLE`); local-only, Bluetooth off, the
/// test secrets, no public address (passkeys off unless a test sets one). Nothing in the
/// repository is read or rewritten.
pub fn isolated_config(name: &str) -> Config {
    let defaults = Config::defaults(temp_root(name));
    std::fs::create_dir_all(defaults.auth_path.parent().expect("auth/")).expect("auth dir should be created");
    std::fs::write(&defaults.auth_path, TEST_PEOPLE).expect("people file should be written");
    Config {
        host: "127.0.0.1".into(),
        port: 0,
        jwt_secret: jwt_secret(),
        auth_cookie_secure: false,
        disable_bluetooth: true,
        ir_api_token: Some(TEST_IR_TOKEN.into()),
        public_url: None,
        ..defaults
    }
}

/// A never-expiring token for `person` (one of `TEST_PEOPLE`), as if they had just signed in.
pub fn token_for(person: &str) -> String {
    token_signed_in_ago(person, 0)
}

/// A token for `person` whose last passkey sign-in was `age_ms` ago.
pub fn token_signed_in_ago(person: &str, age_ms: i64) -> String {
    let now = chrono::Utc::now().timestamp_millis();
    let claims = Claims { user_id: person.to_string(), issued_ms: now, auth_ms: now - age_ms, exp: 4_102_444_800 };
    encode(&Header::default(), &claims, &EncodingKey::from_secret(jwt_secret().as_bytes()))
        .expect("test token should encode")
}

/// A never-expiring admin token for `leonard`.
pub fn test_token() -> String {
    token_for("leonard")
}

/// A never-expiring member token for `alex` (no admin rights).
pub fn member_token() -> String {
    token_for("alex")
}

/// Who exists in every test config: an admin and a member.
const TEST_PEOPLE: &str = r#"{"people": [
  {"id": "leonard", "name": "Léonard", "role": "admin", "userHandle": "0b3f2a4e-1c5d-4e6f-8a9b-0c1d2e3f4a5b"},
  {"id": "alex", "name": "Alex", "role": "member", "userHandle": "1c4a3b5f-2d6e-4f70-9bac-1d2e3f4a5b6c"}
]}"#;

/// The full app built from `config`, reachable as if from localhost.
pub fn app(config: Config) -> Router {
    build_app_parts_from_config(Arc::new(config))
        .expect("failed to build test app")
        .0
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
}

/// A request, with a bearer `token` and a JSON `body` when given.
pub fn request(method: Method, path: &str, token: Option<&str>, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(token) = token {
        builder = builder.header("Authorization", format!("Bearer {token}"));
    }
    if body.is_some() {
        builder = builder.header("Content-Type", "application/json");
    }
    builder
        .body(
            body.map(|value| Body::from(serde_json::to_vec(&value).expect("body should encode")))
                .unwrap_or_else(Body::empty),
        )
        .expect("request should build")
}

/// Runs `request` through `app`: status, headers and the JSON body (`null` when empty).
/// Every API answer is held to the API's casing (`util::casing_offences`): camelCase
/// English keys, whatever the test is about.
pub async fn respond(app: &Router, request: Request<Body>) -> (StatusCode, HeaderMap, Value) {
    let path = request.uri().path().to_string();
    let response = app.clone().oneshot(request).await.expect("request should succeed");
    let status = response.status();
    let headers = response.headers().clone();
    let body = to_bytes(response.into_body(), usize::MAX).await.expect("body should be readable");
    let json = serde_json::from_slice(&body).unwrap_or(Value::Null);
    if path.starts_with("/api") {
        let offences = maison_backend::util::casing_offences(&json);
        assert!(offences.is_empty(), "{path} answers keys that are not camelCase: {offences:?}");
    }
    (status, headers, json)
}

/// One round trip: status and JSON body.
pub async fn send(
    app: &Router,
    method: Method,
    path: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let (status, _, json) = respond(app, request(method, path, token, body)).await;
    (status, json)
}

/// One round trip as the test admin.
pub async fn send_authed(app: &Router, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    send(app, method, path, Some(&test_token()), body).await
}

/// Serialises the tests of one binary that share a real device.
pub fn serial() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(Default::default)
}

/// Rounds every number to 6 decimals, so float noise does not fail a comparison.
pub fn normalize_numbers(value: Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.into_iter().map(normalize_numbers).collect()),
        Value::Object(entries) => Value::Object(
            entries
                .into_iter()
                .map(|(key, value)| (key, normalize_numbers(value)))
                .collect(),
        ),
        Value::Number(number) => match number.as_f64() {
            Some(value) => json!((value * 1_000_000.0).round() / 1_000_000.0),
            None => Value::Number(number),
        },
        other => other,
    }
}

/// `assert_eq!` on JSON, printing both sides pretty on failure.
pub fn assert_json_eq(left: &Value, right: &Value) {
    let pretty = |value: &Value| serde_json::to_string_pretty(value).expect("json should pretty print");
    assert_eq!(left, right, "left:\n{}\n\nright:\n{}", pretty(left), pretty(right));
}
