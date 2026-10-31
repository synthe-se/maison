//! /api/photos: who reads and who changes them, what is refused, a round trip of the bytes, and
//! that a change is said on the event stream.

mod common;

use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, Response, StatusCode},
    Router,
};
use futures::StreamExt;
use maison_api::photos::{MAX_PHOTOS, MAX_PHOTO_BYTES, UPLOAD_FIELD};
use maison_backend::{
    auth::issue_service_token,
    people::{People, Scope, TokenKind},
};
use serde_json::{json, Value};
use tower::ServiceExt;

const BOUNDARY: &str = "maison-photo-boundary";

/// A JPEG as far as the server looks: its first and its last bytes.
fn jpeg(n: u8) -> Vec<u8> {
    [&[0xff, 0xd8, 0xff, 0xe0, n][..], &[0xff, 0xd9]].concat()
}

/// The multipart body a browser's FormData sends.
fn multipart(field: &str, bytes: &[u8]) -> Vec<u8> {
    let head = format!(
        "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{field}\"; filename=\"p.jpg\"\r\nContent-Type: image/jpeg\r\n\r\n"
    );
    [
        head.as_bytes(),
        bytes,
        format!("\r\n--{BOUNDARY}--\r\n").as_bytes(),
    ]
    .concat()
}

fn upload(token: Option<&str>, field: &str, bytes: &[u8]) -> Request<Body> {
    let mut request = Request::builder()
        .method(Method::POST)
        .uri("/api/photos")
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={BOUNDARY}"),
        );
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    request
        .body(Body::from(multipart(field, bytes)))
        .expect("request should build")
}

async fn add(app: &Router, bytes: &[u8]) -> (StatusCode, Value) {
    let (status, _, body) = common::respond(
        app,
        upload(Some(&common::test_token()), UPLOAD_FIELD, bytes),
    )
    .await;
    (status, body)
}

async fn raw(app: &Router, path: &str, token: Option<&str>) -> Response<Body> {
    app.clone()
        .oneshot(common::request(Method::GET, path, token, None))
        .await
        .expect("request should succeed")
}

async fn listed(app: &Router) -> Vec<String> {
    let (_, body) = common::send_authed(app, Method::GET, "/api/photos", None).await;
    body["photos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn a_photo_goes_in_comes_back_whole_and_goes() {
    let app = common::app(common::isolated_config("maison-photos-trip"));
    assert!(listed(&app).await.is_empty());
    let bytes = jpeg(1);
    let (status, body) = add(&app, &bytes).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let id = body["photo"]["id"].as_str().unwrap().to_string();
    assert_eq!(id.len(), 64, "a SHA-256");
    assert_eq!(body["photo"]["bytes"], json!(bytes.len()));

    // the same bytes are the same photo
    let (status, again) = add(&app, &bytes).await;
    assert_eq!((status, &again), (StatusCode::OK, &body));
    assert_eq!(listed(&app).await, std::slice::from_ref(&id));

    let path = format!("/api/photos/{id}");
    let got = raw(&app, &path, Some(&common::member_token())).await;
    assert_eq!(got.status(), StatusCode::OK);
    assert_eq!(got.headers()[header::CONTENT_TYPE], "image/jpeg");
    assert_eq!(
        got.headers()[header::CACHE_CONTROL],
        "private, max-age=31536000, immutable",
        "named by its content"
    );
    let body = to_bytes(got.into_body(), usize::MAX).await.unwrap();
    assert_eq!(body.as_ref(), bytes);

    let (status, said) = common::send_authed(&app, Method::DELETE, &path, None).await;
    assert_eq!(status, StatusCode::OK, "{said}");
    assert!(listed(&app).await.is_empty());
    let gone = raw(&app, &path, Some(&common::test_token())).await;
    assert_eq!(gone.status(), StatusCode::NOT_FOUND);
    assert_eq!(gone.headers()[header::CACHE_CONTROL], "no-store");
    let (status, body) = common::send_authed(&app, Method::DELETE, &path, None).await;
    assert_eq!(
        (status, &body["code"]),
        (StatusCode::NOT_FOUND, &json!("not_found"))
    );
}

#[tokio::test]
async fn reading_wants_someone_in_changing_wants_an_admin() {
    let app = common::app(common::isolated_config("maison-photos-who"));
    let (_, body) = add(&app, &jpeg(2)).await;
    let path = format!("/api/photos/{}", body["photo"]["id"].as_str().unwrap());

    let (status, body) = common::send(&app, Method::GET, "/api/photos", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(body["code"], json!("not_signed_in"));
    assert_eq!(
        raw(&app, &path, None).await.status(),
        StatusCode::UNAUTHORIZED
    );
    let member = common::member_token();
    let (status, _) = common::send(&app, Method::GET, "/api/photos", Some(&member), None).await;
    assert_eq!(status, StatusCode::OK, "a member sees them");

    let (status, _, body) =
        common::respond(&app, upload(Some(&member), UPLOAD_FIELD, &jpeg(3))).await;
    assert_eq!(
        (status, &body["code"]),
        (StatusCode::FORBIDDEN, &json!("forbidden"))
    );
    let (status, body) = common::send(&app, Method::DELETE, &path, Some(&member), None).await;
    assert_eq!(
        (status, &body["code"]),
        (StatusCode::FORBIDDEN, &json!("forbidden"))
    );
    let (status, _, _) = common::respond(&app, upload(None, UPLOAD_FIELD, &jpeg(3))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(listed(&app).await.len(), 1, "nothing changed");
}

#[tokio::test]
async fn a_wall_screen_and_an_app_read_but_never_change() {
    let config = common::isolated_config("maison-photos-screen");
    let token = issue_service_token(
        &People::new(&config.auth_path),
        "photos-app",
        Scope::Devices,
        TokenKind::App,
    )
    .await
    .unwrap()
    .expect("a new name");
    let app = common::app(config);
    let (_, body) = add(&app, &jpeg(4)).await;
    let path = format!("/api/photos/{}", body["photo"]["id"].as_str().unwrap());

    // a screen, paired as an admin does it
    let (_, start) = common::send(&app, Method::POST, "/api/screens/pairing", None, None).await;
    common::send_authed(
        &app,
        Method::POST,
        "/api/screens",
        Some(json!({ "code": start["code"], "name": "Salon" })),
    )
    .await;
    let (_, headers, _) = common::respond(
        &app,
        common::request(
            Method::POST,
            "/api/screens/pairing/claim",
            None,
            Some(json!({ "secret": start["secret"] })),
        ),
    )
    .await;
    let cookie = headers[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let as_screen = |method: Method, path: &str| {
        Request::builder()
            .method(method)
            .uri(path)
            .header(header::COOKIE, &cookie)
            .body(Body::empty())
            .unwrap()
    };
    let (status, _, list) = common::respond(&app, as_screen(Method::GET, "/api/photos")).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert_eq!(list["photos"].as_array().unwrap().len(), 1);
    let got = app
        .clone()
        .oneshot(as_screen(Method::GET, &path))
        .await
        .unwrap();
    assert_eq!(got.status(), StatusCode::OK, "a screen fetches the bytes");

    let (status, _, body) = common::respond(&app, as_screen(Method::DELETE, &path)).await;
    assert_eq!(
        (status, &body["code"]),
        (StatusCode::UNAUTHORIZED, &json!("not_signed_in")),
        "a screen is nobody's admin"
    );

    // an app's service token: the same
    let (status, _) = common::send(&app, Method::GET, "/api/photos", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = common::send(&app, Method::DELETE, &path, Some(&token), None).await;
    assert!(status.is_client_error(), "{status}");
    assert_eq!(listed(&app).await.len(), 1);
}

#[tokio::test]
async fn what_is_no_whole_jpeg_or_too_heavy_is_refused() {
    let app = common::app(common::isolated_config("maison-photos-refused"));
    let (status, body) = add(&app, b"\x89PNG\r\n\x1a\n").await;
    assert_eq!(
        (status, &body["code"]),
        (StatusCode::UNSUPPORTED_MEDIA_TYPE, &json!("not_a_photo"))
    );
    let torn = &jpeg(5)[..4];
    let (status, body) = add(&app, torn).await;
    assert_eq!((status, &body["code"]), (status, &json!("not_a_photo")));

    // over the limit, inside the body's own: refused by the photo's check
    let mut heavy = jpeg(6);
    heavy.splice(4..4, vec![0; MAX_PHOTO_BYTES]);
    let (status, body) = add(&app, &heavy).await;
    assert_eq!(
        (status, &body["code"]),
        (StatusCode::PAYLOAD_TOO_LARGE, &json!("too_large"))
    );
    // over the body's limit: refused while it comes
    heavy.splice(4..4, vec![0; 1024 * 1024]);
    let (status, body) = add(&app, &heavy).await;
    assert_eq!(
        (status, &body["code"]),
        (StatusCode::PAYLOAD_TOO_LARGE, &json!("too_large"))
    );

    let (status, _, body) =
        common::respond(&app, upload(Some(&common::test_token()), "other", &jpeg(7))).await;
    assert_eq!(
        (status, &body["code"]),
        (StatusCode::BAD_REQUEST, &json!("invalid"))
    );
    assert!(listed(&app).await.is_empty(), "nothing kept");
}

#[tokio::test]
async fn the_house_keeps_a_bounded_number_of_photos() {
    let config = common::isolated_config("maison-photos-full");
    std::fs::create_dir_all(&config.photos_dir).unwrap();
    for n in 0..MAX_PHOTOS {
        let name = format!("{n:064x}.jpg");
        std::fs::write(config.photos_dir.join(name), jpeg(0)).unwrap();
    }
    let first = format!("{:064x}", 0);
    let app = common::app(config);
    let (status, body) = add(&app, &jpeg(8)).await;
    assert_eq!(
        (status, &body["code"]),
        (StatusCode::CONFLICT, &json!("too_many_photos"))
    );
    // one goes, another comes
    let path = format!("/api/photos/{first}");
    common::send_authed(&app, Method::DELETE, &path, None).await;
    assert_eq!(add(&app, &jpeg(8)).await.0, StatusCode::OK);
}

#[tokio::test]
async fn a_name_that_is_no_id_finds_nothing() {
    let app = common::app(common::isolated_config("maison-photos-names"));
    for path in ["/api/photos/secret", "/api/photos/..%2F..%2Fauth"] {
        let got = raw(&app, path, Some(&common::test_token())).await;
        assert_eq!(got.status(), StatusCode::NOT_FOUND, "{path}");
    }
}

#[tokio::test]
async fn a_change_is_said_on_the_stream() {
    let app = common::app(common::isolated_config("maison-photos-events"));
    let response = raw(&app, "/api/events", Some(&common::test_token())).await;
    assert_eq!(response.status(), StatusCode::OK);
    let mut body = Body::into_data_stream(response.into_body());
    add(&app, &jpeg(9)).await;
    let mut said = String::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while !said.contains("data: photos") {
        let chunk = tokio::time::timeout_at(deadline, body.next())
            .await
            .expect("said within the wait")
            .expect("the stream stays open")
            .unwrap();
        said.push_str(std::str::from_utf8(&chunk).unwrap());
    }
}
