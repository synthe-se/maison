//! Wall screens: a browser asks for a pairing signed out, an admin pairs it by its code, and
//! the browser gets an HttpOnly cookie that drives the devices as a service token does, never
//! people nor settings; revoked, it is signed out at once.

mod common;

use std::{net::SocketAddr, sync::Arc};

use axum::{
    extract::connect_info::MockConnectInfo,
    http::{header, HeaderMap, Method, StatusCode},
    Router,
};
use maison_backend::{
    auth::issue_service_token,
    build_app_parts_from_config,
    people::{People, Scope, TokenKind},
};
use serde_json::{json, Value};

const COOKIE: &str = "maison_screen";

/// A request carrying the screen's cookie (and no bearer header).
async fn with_cookie(
    app: &Router,
    method: Method,
    path: &str,
    cookie: &str,
) -> (StatusCode, HeaderMap, Value) {
    let mut request = common::request(method, path, None, None);
    request.headers_mut().insert(
        header::COOKIE,
        format!("{COOKIE}={cookie}").parse().unwrap(),
    );
    common::respond(app, request).await
}

/// The screen's token, as its claim's `Set-Cookie` says, with the cookie's attributes.
fn screen_cookie(headers: &HeaderMap) -> (String, String) {
    let set = headers
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .find(|c| c.starts_with(COOKIE))
        .expect("the screen's cookie");
    let token = set
        .split(';')
        .next()
        .unwrap()
        .trim_start_matches(&format!("{COOKIE}="))
        .to_string();
    (token, set)
}

/// A screen asks, an admin pairs it as `name`: the screen's cookie.
async fn paired(app: &Router, name: &str) -> String {
    let (_, start) = common::send(app, Method::POST, "/api/screens/pairing", None, None).await;
    let (s, _) = common::send_authed(
        app,
        Method::POST,
        "/api/screens",
        Some(json!({ "code": start["code"], "name": name })),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (_, headers, _) = common::respond(
        app,
        common::request(
            Method::POST,
            "/api/screens/pairing/claim",
            None,
            Some(json!({ "secret": start["secret"] })),
        ),
    )
    .await;
    screen_cookie(&headers).0
}

#[tokio::test]
async fn an_admin_pairs_a_screen_by_its_code_and_the_screen_alone_gets_its_cookie_once() {
    let app = common::app(common::isolated_config("maison-screens-pair"));
    let (s, start) = common::send(&app, Method::POST, "/api/screens/pairing", None, None).await;
    assert_eq!(s, StatusCode::OK, "{start}");
    let code = start["code"].as_str().unwrap().to_string();
    let secret = start["secret"].as_str().unwrap().to_string();
    assert_eq!(code.len(), 9, "ABCD-EFGH");
    assert_eq!(start["pollMs"], 3_000);
    assert_eq!(start["expiresInMs"], 600_000);
    let claim = || {
        common::request(
            Method::POST,
            "/api/screens/pairing/claim",
            None,
            Some(json!({ "secret": secret })),
        )
    };

    let (s, headers, v) = common::respond(&app, claim()).await;
    assert_eq!((s, &v["paired"]), (StatusCode::OK, &json!(false)));
    assert!(
        headers.get(header::SET_COOKIE).is_none(),
        "no cookie before an admin pairs it"
    );

    // a member may not pair; a wrong code pairs nothing
    let pair = |code: &str, name: &str| Some(json!({ "code": code, "name": name }));
    let (s, v) = common::send(
        &app,
        Method::POST,
        "/api/screens",
        Some(&common::member_token()),
        pair(&code, "Salon"),
    )
    .await;
    assert_eq!(
        (s, v["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("forbidden"))
    );
    let (s, v) = common::send_authed(
        &app,
        Method::POST,
        "/api/screens",
        pair("ZZZZ-ZZZZ", "Salon"),
    )
    .await;
    assert_eq!(
        (s, v["code"].as_str()),
        (StatusCode::NOT_FOUND, Some("pairing_unknown"))
    );

    // typed by hand: lowercase, no dash
    let typed = code.replace('-', "").to_lowercase();
    let (s, v) =
        common::send_authed(&app, Method::POST, "/api/screens", pair(&typed, " Salon ")).await;
    assert_eq!(
        (s, &v["screen"]["name"]),
        (StatusCode::OK, &json!("Salon")),
        "{v}"
    );

    let (s, headers, v) = common::respond(&app, claim()).await;
    assert_eq!(
        (s, &v["paired"], &v["screen"]["name"]),
        (StatusCode::OK, &json!(true), &json!("Salon"))
    );
    let (token, set) = screen_cookie(&headers);
    assert!(token.starts_with("maison_"), "a service token");
    assert!(
        set.contains("HttpOnly") && set.contains("SameSite=Lax") && set.contains("Path=/"),
        "{set}"
    );
    assert!(
        set.contains(&format!("Max-Age={}", 400 * 24 * 3600)),
        "{set}"
    );
    assert!(
        v.to_string().find(&token).is_none(),
        "the token is never in the body: a script never sees it"
    );

    let (s, v) = common::respond(&app, claim())
        .await
        .2
        .get("code")
        .map_or((StatusCode::OK, Value::Null), |c| {
            (StatusCode::NOT_FOUND, c.clone())
        });
    assert_eq!(
        (s, v),
        (StatusCode::NOT_FOUND, json!("pairing_unknown")),
        "handed once"
    );
}

#[tokio::test]
async fn a_screen_drives_the_devices_never_people_and_goes_when_revoked() {
    let config = common::isolated_config("maison-screens-use");
    let people = People::new(&config.auth_path);
    let app = common::app(config);
    let cookie = paired(&app, "Salon").await;

    let (s, headers, v) = with_cookie(&app, Method::GET, "/api/screens/me", &cookie).await;
    assert_eq!((s, &v["screen"]["name"]), (StatusCode::OK, &json!("Salon")));
    assert_eq!(
        screen_cookie(&headers).0,
        cookie,
        "renewed at each start of its page"
    );
    for path in [
        "/api/meross",
        "/api/matter/covers",
        "/api/scenes",
        "/api/matter/place",
    ] {
        assert_eq!(
            with_cookie(&app, Method::GET, path, &cookie).await.0,
            StatusCode::OK,
            "{path}"
        );
    }
    assert!(
        people.tokens().await.unwrap()[0].last_used_ms.is_some(),
        "its use is dated"
    );
    // people and settings: a screen is nobody signed in
    for path in [
        "/api/people",
        "/api/passkeys",
        "/api/screens",
        "/api/invites",
    ] {
        let (s, _, v) = with_cookie(&app, Method::GET, path, &cookie).await;
        assert_eq!(
            (s, v["code"].as_str()),
            (StatusCode::UNAUTHORIZED, Some("not_signed_in")),
            "{path}"
        );
    }

    let (s, v) = common::send_authed(&app, Method::GET, "/api/screens", None).await;
    assert_eq!(s, StatusCode::OK);
    let screens = v["screens"].as_array().unwrap();
    assert_eq!(screens.len(), 1);
    assert_eq!(screens[0]["name"], "Salon");
    let id = screens[0]["id"].as_str().unwrap().to_string();
    assert_eq!(
        common::send(
            &app,
            Method::GET,
            "/api/screens",
            Some(&common::member_token()),
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );

    let (s, _) =
        common::send_authed(&app, Method::DELETE, &format!("/api/screens/{id}"), None).await;
    assert_eq!(s, StatusCode::OK);
    for path in ["/api/meross", "/api/screens/me"] {
        let (s, _, v) = with_cookie(&app, Method::GET, path, &cookie).await;
        assert_eq!(
            (s, v["code"].as_str()),
            (StatusCode::UNAUTHORIZED, Some("not_signed_in")),
            "revoked: {path}"
        );
    }
    let (s, v) =
        common::send_authed(&app, Method::DELETE, &format!("/api/screens/{id}"), None).await;
    assert_eq!(
        (s, v["code"].as_str()),
        (StatusCode::NOT_FOUND, Some("not_found"))
    );
}

#[tokio::test]
async fn a_name_pairs_one_screen_and_an_app_is_no_screen() {
    let config = common::isolated_config("maison-screens-names");
    let people = People::new(&config.auth_path);
    let app = common::app(config);
    paired(&app, "Salon").await;
    let (_, start) = common::send(&app, Method::POST, "/api/screens/pairing", None, None).await;
    let (s, v) = common::send_authed(
        &app,
        Method::POST,
        "/api/screens",
        Some(json!({ "code": start["code"], "name": "Salon" })),
    )
    .await;
    assert_eq!(
        (s, v["code"].as_str()),
        (StatusCode::CONFLICT, Some("screen_exists"))
    );

    // an app's token in a screen's cookie drives nothing more, and is not a screen
    let achi = issue_service_token(&people, "achi", Scope::Devices, TokenKind::App)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        with_cookie(&app, Method::GET, "/api/screens/me", &achi)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let (_, v) = common::send_authed(&app, Method::GET, "/api/screens", None).await;
    assert_eq!(
        v["screens"].as_array().unwrap().len(),
        1,
        "the apps are not listed"
    );
    let (s, _) = common::send_authed(
        &app,
        Method::DELETE,
        &format!(
            "/api/screens/{}",
            people
                .tokens()
                .await
                .unwrap()
                .iter()
                .find(|t| t.name == "achi")
                .unwrap()
                .id
        ),
        None,
    )
    .await;
    assert_eq!(
        s,
        StatusCode::NOT_FOUND,
        "an app's token is not revoked as a screen"
    );
}

#[tokio::test]
async fn a_screens_cookie_comes_through_the_tunnel_only() {
    let config = common::isolated_config("maison-screens-lan");
    let local = common::app(config.clone());
    let cookie = paired(&local, "Salon").await;
    let lan = build_app_parts_from_config(Arc::new(config))
        .unwrap()
        .0
        .layer(MockConnectInfo(SocketAddr::from(([192, 168, 1, 20], 4242))));
    let (s, _, _) = with_cookie(&lan, Method::GET, "/api/meross", &cookie).await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "from the LAN only a bearer token reaches the devices"
    );
    let (s, _) = common::send(&lan, Method::POST, "/api/screens/pairing", None, None).await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "pairing goes through the tunnel too"
    );
}

/// Through the tunnel each request names its client (`CF-Connecting-IP`).
fn from(
    mut request: axum::http::Request<axum::body::Body>,
    ip: &str,
) -> axum::http::Request<axum::body::Body> {
    request
        .headers_mut()
        .insert("cf-connecting-ip", ip.parse().unwrap());
    request
}

#[tokio::test]
async fn only_an_admin_on_the_screens_network_pairs_it() {
    let app = common::app(common::isolated_config("maison-screens-network"));
    let home = "198.51.100.7";
    let (_, _, start) = common::respond(
        &app,
        from(
            common::request(Method::POST, "/api/screens/pairing", None, None),
            home,
        ),
    )
    .await;
    let pair = |ip: &str| {
        from(
            common::request(
                Method::POST,
                "/api/screens",
                Some(&common::test_token()),
                Some(json!({ "code": start["code"], "name": "Salon" })),
            ),
            ip,
        )
    };
    // a code sent to the admin from across the internet: refused, said so
    let (s, _, v) = common::respond(&app, pair("203.0.113.9")).await;
    assert_eq!(
        (s, v["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("pairing_elsewhere"))
    );
    let (s, _, v) = common::respond(&app, pair(home)).await;
    assert_eq!(
        (s, &v["screen"]["name"]),
        (StatusCode::OK, &json!("Salon")),
        "from the house: paired"
    );
}
