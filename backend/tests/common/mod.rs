//! What every integration test binary shares: the test configuration, the
//! token, the in-process app and the request round trip. Each binary uses a
//! subset, hence the `dead_code` allowance.
#![allow(dead_code)]

use std::{net::SocketAddr, path::PathBuf, sync::Arc, sync::OnceLock};

use axum::{
    Router,
    body::{Body, to_bytes},
    extract::connect_info::MockConnectInfo,
    http::{HeaderMap, Method, Request, StatusCode},
};
use jsonwebtoken::{EncodingKey, Header, encode};
use maison_backend::{
    auth::Claims,
    build_app_from_config,
    config::{self, Config},
};
use serde_json::{Value, json};
use tower::ServiceExt;

/// The machine token the test config accepts on `/api/ir`.
pub const TEST_IR_TOKEN: &str = "test-ir-token";

/// The repository root: the parent of the backend crate.
pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend has parent")
        .to_path_buf()
}

/// A fresh, created directory `<tmp>/<name>/<uuid>`.
pub fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir()
        .join(name)
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&root).expect("temp test dir should be created");
    root
}

/// `name` from the environment, else `default` under the repository root.
pub fn env_path(name: &str, default: &str) -> PathBuf {
    config::env_text(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root().join(default))
}

/// The secret the test config signs with and the test token is signed with.
pub fn jwt_secret() -> String {
    config::env_text("JWT_SECRET").unwrap_or_else(|| config::DEFAULT_JWT_SECRET.to_string())
}

/// Defaults under the repository root, local-only and with Bluetooth off.
pub fn test_config() -> Config {
    Config {
        matter_state_dir: std::env::temp_dir().join("maison-matter-tests-unused"),
        ..for_tests(Config::defaults(workspace_root()))
    }
}

/// What every test config changes: local-only, Bluetooth off, the test secrets, no public
/// address (passkeys off unless a test sets one).
fn for_tests(defaults: Config) -> Config {
    Config {
        host: "127.0.0.1".into(),
        port: 0,
        jwt_secret: jwt_secret(),
        auth_cookie_secure: false,
        disable_bluetooth: true,
        ir_api_token: Some(TEST_IR_TOKEN.into()),
        public_url: None,
        auth_path: test_people(),
        ..defaults
    }
}

/// A fresh people file holding `TEST_PEOPLE`.
fn test_people() -> PathBuf {
    let path = temp_root("maison-test-people").join("auth.json");
    std::fs::write(&path, TEST_PEOPLE).expect("people file should be written");
    path
}

/// The test config on a fresh, empty root of its own (`<tmp>/<name>/<uuid>`): every state
/// file starts missing, nothing in the repository is read or rewritten.
pub fn isolated_config(name: &str) -> Config {
    let root = temp_root(name);
    Config { matter_state_dir: root.join("matter"), ..for_tests(Config::defaults(root)) }
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
    build_app_from_config(Arc::new(config))
        .expect("failed to build test app")
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
pub async fn respond(app: &Router, request: Request<Body>) -> (StatusCode, HeaderMap, Value) {
    let response = app.clone().oneshot(request).await.expect("request should succeed");
    let status = response.status();
    let headers = response.headers().clone();
    let body = to_bytes(response.into_body(), usize::MAX).await.expect("body should be readable");
    (status, headers, serde_json::from_slice(&body).unwrap_or(Value::Null))
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
