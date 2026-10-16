mod common;

use axum::http::{Method, StatusCode};
use serde_json::{json, Value};

#[tokio::test]
async fn auth_verify_accepts_valid_token() {
    let rust = request_rust(
        Method::POST,
        "/api/auth/verify",
        None,
        Some(common::test_token()),
    )
    .await;

    assert_eq!(rust.0, StatusCode::OK);
    assert_eq!(
        normalize_verify_response(rust.1),
        json!({
            "success": true,
            "user": {
                "id": "leonard",
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
    common::send(
        &common::app(common::isolated_config("maison-auth")),
        method,
        path,
        token.as_deref(),
        body,
    )
    .await
}

#[tokio::test]
async fn auth_refresh_without_cookie_returns_unauthorized() {
    let rust = request_rust(Method::POST, "/api/auth/refresh", None, None).await;

    assert_eq!(rust.0, StatusCode::UNAUTHORIZED);
    assert_eq!(
        rust.1,
        json!({
            "success": false,
            "error": "Not signed in",
            "code": "not_signed_in"
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

// ---------- roles, people, sessions, the request guard ----------

use axum::{body::Body, extract::connect_info::MockConnectInfo, http::Request};
use maison_backend::config::Config;
use std::net::SocketAddr;

/// The app with passkeys on (invitations need a public address), as the test people.
fn house() -> axum::Router {
    common::app(Config {
        public_url: Some("https://maison.example.com".into()),
        ..common::isolated_config("maison-auth")
    })
}

#[tokio::test]
async fn a_member_controls_the_house_but_does_not_administer_it() {
    let app = house();
    let member = common::member_token();
    let (s, _) = common::send(&app, Method::GET, "/api/meross", Some(&member), None).await;
    assert_eq!(s, StatusCode::OK, "everyday use");
    for (method, path, body) in [
        (Method::GET, "/api/people", None),
        (Method::GET, "/api/invites", None),
        (Method::POST, "/api/invites", Some(json!({ "name": "Eve" }))),
        (Method::DELETE, "/api/people/leonard", None),
    ] {
        let (s, v) = common::send(&app, method, path, Some(&member), body).await;
        assert_eq!(
            (s, v["code"].as_str()),
            (StatusCode::FORBIDDEN, Some("forbidden")),
            "{path}"
        );
    }
}

#[tokio::test]
async fn nobody_signed_in_gets_nothing_from_the_house() {
    let app = house();
    let (s, v) = common::send(&app, Method::GET, "/api/meross", None, None).await;
    assert_eq!(
        (s, v["code"].as_str()),
        (StatusCode::UNAUTHORIZED, Some("not_signed_in"))
    );
    let stranger = common::token_for("nobody");
    assert_eq!(
        common::send(&app, Method::GET, "/api/meross", Some(&stranger), None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn an_invitation_by_name_never_hands_over_an_existing_account() {
    let app = house();
    let (s, v) = common::send_authed(
        &app,
        Method::POST,
        "/api/invites",
        Some(json!({ "name": "Léonard" })),
    )
    .await;
    assert_eq!(
        (s, v["code"].as_str()),
        (StatusCode::CONFLICT, Some("person_exists"))
    );
    assert_eq!(
        v["detail"]["person"],
        json!({ "id": "leonard", "name": "Léonard" }),
        "who has the name"
    );
    // giving Alex their access back is said explicitly
    let (s, v) = common::send_authed(
        &app,
        Method::POST,
        "/api/invites",
        Some(json!({ "name": "Alex", "person": "alex" })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (s, v) = common::send_authed(
        &app,
        Method::POST,
        "/api/invites",
        Some(json!({ "name": "Eve" })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    // a person's id is a slug, said with a code
    let (s, v) = common::send_authed(
        &app,
        Method::POST,
        "/api/invites",
        Some(json!({ "name": "Eve", "person": "Not An Id" })),
    )
    .await;
    assert_eq!(
        (s, v["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("bad_person"))
    );
}

#[tokio::test]
async fn health_and_logout_answer_in_the_usual_envelope() {
    let app = house();
    let (s, v) = common::send(&app, Method::GET, "/health", None, None).await;
    assert_eq!(
        (s, v),
        (
            StatusCode::OK,
            json!({ "success": true, "status": "healthy", "service": "maison-backend" })
        )
    );
    let (_, v) = common::send(&app, Method::POST, "/api/auth/logout", None, None).await;
    assert_eq!(
        v,
        json!({ "success": true, "message": "Logged out successfully" })
    );
}

#[tokio::test]
async fn admins_see_everyone_and_remove_someone_but_never_themselves() {
    let app = house();
    let (s, people) = common::send_authed(&app, Method::GET, "/api/people", None).await;
    assert_eq!(s, StatusCode::OK);
    let ids: Vec<&str> = people["people"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["leonard", "alex"]);
    let (s, v) = common::send_authed(&app, Method::DELETE, "/api/people/leonard", None).await;
    assert_eq!(
        (s, v["code"].as_str()),
        (StatusCode::CONFLICT, Some("not_yourself"))
    );
    // a session alone, its passkey sign-in long ago, removes nobody
    let stale = common::token_signed_in_ago("leonard", 6 * 60 * 1000);
    let (s, v) = common::send(&app, Method::DELETE, "/api/people/alex", Some(&stale), None).await;
    assert_eq!(
        (s, v["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("reauth_needed"))
    );
    let alex = common::member_token();
    assert_eq!(
        common::send(&app, Method::POST, "/api/auth/verify", Some(&alex), None)
            .await
            .0,
        StatusCode::OK
    );
    let (s, _, _) = common::respond(
        &app,
        common::request(
            Method::DELETE,
            "/api/people/alex",
            Some(&common::test_token()),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(
        common::send(&app, Method::POST, "/api/auth/verify", Some(&alex), None)
            .await
            .0,
        StatusCode::UNAUTHORIZED,
        "gone at once"
    );
}

#[tokio::test]
async fn signing_out_everywhere_ends_the_access_tokens_too() {
    let app = house();
    let me = common::member_token();
    assert_eq!(
        common::send(&app, Method::POST, "/api/auth/verify", Some(&me), None)
            .await
            .0,
        StatusCode::OK
    );
    // a token issued in the same millisecond is still valid: wait one
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    assert_eq!(
        common::send(
            &app,
            Method::POST,
            "/api/auth/logout-everywhere",
            Some(&me),
            None
        )
        .await
        .0,
        StatusCode::OK
    );
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    assert_eq!(
        common::send(&app, Method::POST, "/api/auth/verify", Some(&me), None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        common::send(
            &app,
            Method::POST,
            "/api/auth/verify",
            Some(&common::member_token()),
            None
        )
        .await
        .0,
        StatusCode::OK,
        "a new sign-in works"
    );
}

#[tokio::test]
async fn adding_a_passkey_from_a_session_wants_a_recent_passkey_sign_in() {
    let app = house();
    let stale = common::token_signed_in_ago("leonard", 6 * 60 * 1000);
    let (s, v) = common::send(
        &app,
        Method::POST,
        "/api/passkeys/register/start",
        Some(&stale),
        Some(json!({})),
    )
    .await;
    assert_eq!(
        (s, v["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("reauth_needed"))
    );
    let (s, _) = common::send_authed(
        &app,
        Method::POST,
        "/api/passkeys/register/start",
        Some(json!({})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "just signed in");
}

fn raw(method: Method, path: &str, headers: &[(&str, &str)]) -> Request<Body> {
    let mut r = common::request(method, path, Some(&common::test_token()), None);
    for (k, v) in headers {
        r.headers_mut().insert(
            axum::http::HeaderName::from_bytes(k.as_bytes()).unwrap(),
            v.parse().unwrap(),
        );
    }
    r
}

#[tokio::test]
async fn a_change_from_another_site_is_refused() {
    let app = house();
    let (s, _, _) = common::respond(
        &app,
        raw(
            Method::POST,
            "/api/auth/logout",
            &[("sec-fetch-site", "cross-site")],
        ),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _, _) = common::respond(
        &app,
        raw(
            Method::POST,
            "/api/auth/logout",
            &[("sec-fetch-site", "same-origin")],
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, headers, _) = common::respond(&app, raw(Method::GET, "/api/meross", &[])).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        headers.get("cache-control").and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    assert!(headers
        .get("content-security-policy")
        .is_some_and(|v| v.to_str().unwrap().contains("base-uri 'none'")));
}

#[tokio::test]
async fn the_lan_reaches_only_the_ir_bridge() {
    let app = maison_backend::build_app_parts_from_config(std::sync::Arc::new(
        common::isolated_config("maison-auth"),
    ))
    .unwrap()
    .0
    .layer(MockConnectInfo(SocketAddr::from(([192, 168, 1, 20], 4000))));
    let (s, _, _) = common::respond(&app, raw(Method::GET, "/api/meross", &[])).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "even signed in: the tunnel only");
    let (s, _, _) =
        common::respond(&app, common::request(Method::GET, "/health", None, None)).await;
    assert_eq!(s, StatusCode::OK);
    let mut key = common::request(Method::POST, "/api/ir/key", None, Some(json!({})));
    key.headers_mut().insert(
        "authorization",
        format!("Bearer {}", common::TEST_IR_TOKEN).parse().unwrap(),
    );
    let (s, _, _) = common::respond(&app, key).await;
    assert_ne!(s, StatusCode::FORBIDDEN, "the IR bridge passes the guard");
}

#[tokio::test]
async fn an_admins_or_a_recovery_invitation_wants_a_recent_passkey_sign_in() {
    let app = house();
    let stale = common::token_signed_in_ago("leonard", 6 * 60 * 1000);
    for body in [
        json!({ "name": "Eve", "admin": true }),
        json!({ "name": "Alex", "person": "alex" }),
    ] {
        let (s, v) =
            common::send(&app, Method::POST, "/api/invites", Some(&stale), Some(body)).await;
        assert_eq!(
            (s, v["code"].as_str()),
            (StatusCode::FORBIDDEN, Some("reauth_needed")),
            "a stolen session cookie plants no passkey and makes no admin"
        );
    }
    let (s, v) = common::send(
        &app,
        Method::POST,
        "/api/invites",
        Some(&stale),
        Some(json!({ "name": "Eve" })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "a member's invitation: {v}");
    let (s, v) = common::send_authed(
        &app,
        Method::POST,
        "/api/invites",
        Some(json!({ "name": "Zoé", "admin": true })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let hours =
        (v["expiresMs"].as_i64().unwrap() - chrono::Utc::now().timestamp_millis()) / 3_600_000;
    assert!(
        (71..=72).contains(&hours),
        "an admin's link: three days ({hours} h)"
    );
}

#[tokio::test]
async fn an_admin_ends_someones_sessions_and_their_passkeys_stay() {
    let app = house();
    let alex = common::member_token();
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    let (s, _) =
        common::send_authed(&app, Method::POST, "/api/people/alex/sessions/end", None).await;
    assert_eq!(s, StatusCode::OK);
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    assert_eq!(
        common::send(&app, Method::POST, "/api/auth/verify", Some(&alex), None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let (s, v) =
        common::send_authed(&app, Method::POST, "/api/people/nobody/sessions/end", None).await;
    assert_eq!(
        (s, v["code"].as_str()),
        (StatusCode::NOT_FOUND, Some("not_found"))
    );
    let (s, _) = common::send(
        &app,
        Method::POST,
        "/api/people/leonard/sessions/end",
        Some(&common::member_token()),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "an admin's");
}

#[tokio::test]
async fn signing_out_everywhere_clears_the_browser() {
    let app = house();
    let (s, headers, _) = common::respond(
        &app,
        common::request(
            Method::POST,
            "/api/auth/logout-everywhere",
            Some(&common::member_token()),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        headers.get("clear-site-data").and_then(|v| v.to_str().ok()),
        Some("\"cookies\", \"storage\"")
    );
}

#[tokio::test]
async fn sessions_are_mine_to_list_and_end() {
    let app = house();
    let (s, v) = common::send_authed(&app, Method::GET, "/api/auth/sessions", None).await;
    assert_eq!((s, &v["sessions"]), (StatusCode::OK, &json!([])), "{v}");
    let (s, v) = common::send_authed(
        &app,
        Method::DELETE,
        "/api/auth/sessions/0123456789abcdef",
        None,
    )
    .await;
    assert_eq!(
        (s, v["code"].as_str()),
        (StatusCode::NOT_FOUND, Some("not_found"))
    );
}

#[tokio::test]
async fn what_a_route_cannot_read_is_a_coded_refusal() {
    let app = house();
    let mut bad = common::request(
        Method::POST,
        "/api/invites",
        Some(&common::test_token()),
        None,
    );
    *bad.body_mut() = Body::from("{not json");
    bad.headers_mut()
        .insert("content-type", "application/json".parse().unwrap());
    let (s, headers, v) = common::respond(&app, bad).await;
    assert_eq!(
        (s, v["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("invalid")),
        "{v}"
    );
    assert_eq!(
        headers.get("content-type").and_then(|v| v.to_str().ok()),
        Some("application/json")
    );
    let (s, v) = common::send_authed(
        &app,
        Method::POST,
        "/api/invites",
        Some(json!({ "name": 3 })),
    )
    .await;
    assert_eq!(
        (s, v["code"].as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("invalid"))
    );
    let (s, v) = common::send_authed(&app, Method::POST, "/api/invites", None).await;
    assert_eq!(
        (s, v["code"].as_str()),
        (StatusCode::UNSUPPORTED_MEDIA_TYPE, Some("invalid")),
        "no JSON at all"
    );
}

#[tokio::test]
async fn a_change_whose_origin_is_not_this_site_is_refused_without_a_host_too() {
    let app = house();
    let mut r = raw(
        Method::POST,
        "/api/auth/logout",
        &[("origin", "https://evil.example")],
    );
    r.headers_mut().remove("host");
    let (s, headers, v) = common::respond(&app, r).await;
    assert_eq!(
        (s, v["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("cross_site"))
    );
    assert!(
        headers.get("content-security-policy").is_some(),
        "a guard's refusal carries the security headers too"
    );
    let (s, _, _) = common::respond(
        &app,
        raw(
            Method::POST,
            "/api/auth/logout",
            &[("origin", "https://maison.example.com")],
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "PUBLIC_URL's origin");
}

#[tokio::test]
async fn the_ir_bridge_is_refused_through_the_tunnel() {
    let app = house();
    let mut key = common::request(
        Method::POST,
        "/api/ir/key",
        Some(common::TEST_IR_TOKEN),
        Some(json!({"code": 1, "value": 1})),
    );
    key.headers_mut()
        .insert("cf-connecting-ip", "203.0.113.9".parse().unwrap());
    let (s, _, v) = common::respond(&app, key).await;
    assert_eq!(
        (s, v["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("lan_only"))
    );
}

#[tokio::test]
async fn wrong_machine_tokens_make_the_address_wait() {
    let app = house();
    for _ in 0..10 {
        let (s, _) = common::send(
            &app,
            Method::POST,
            "/api/ir/key",
            Some("wrong"),
            Some(json!({"code": 1, "value": 1})),
        )
        .await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
    }
    let (s, v) = common::send(
        &app,
        Method::POST,
        "/api/ir/key",
        Some(common::TEST_IR_TOKEN),
        Some(json!({"code": 1, "value": 1})),
    )
    .await;
    assert_eq!(
        (s, v["code"].as_str()),
        (StatusCode::TOO_MANY_REQUESTS, Some("too_many_attempts")),
        "even the right token waits"
    );
}

#[tokio::test]
async fn the_api_says_no_version() {
    let (s, v) = common::send(&house(), Method::GET, "/api", None, None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(v.get("version").is_none(), "{v}");
}
