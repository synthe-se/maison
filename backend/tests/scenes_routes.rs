//! /api/scenes: who may make and run them, what a scene may hold, and that a run answers
//! each of its actions.

mod common;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

/// The rabbit has no host in a fresh config: its action fails — which shows it was run.
fn rabbit_failed(result: &Value) -> bool {
    result["action"] == ping()[0] && result["ok"] == json!(false) && result["code"].is_string()
}

fn ping() -> Value {
    json!([{ "action": "nabaztag", "command": "ping" }])
}

fn scene(name: &str, actions: Value) -> Value {
    json!({ "name": name, "icon": "film", "actions": actions })
}

/// The app on a fresh root, with a `scenes.json` when given.
fn test_app(scenes: Option<Value>) -> axum::Router {
    let config = common::isolated_config("maison-rust-scenes-tests");
    if let Some(scenes) = scenes {
        std::fs::write(&config.scenes_path, scenes.to_string()).expect("scenes written");
    }
    common::app(config)
}

async fn put(app: &axum::Router, id: &str, body: Value) -> (StatusCode, Value) {
    common::send_authed(app, Method::PUT, &format!("/api/scenes/{id}"), Some(body)).await
}

#[tokio::test]
async fn made_listed_in_order_replaced_and_removed() {
    let app = test_app(None);
    let member = common::member_token();
    let (status, body) = common::send(&app, Method::GET, "/api/scenes", Some(&member), None).await;
    assert_eq!(
        (status, body),
        (StatusCode::OK, json!({ "success": true, "scenes": [] })),
        "no seeded scenes"
    );

    let (status, body) = put(&app, "cinema", scene(" Cinéma ", ping())).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!({ "success": true, "scene": { "id": "cinema", "name": "Cinéma", "icon": "film", "actions": ping() } })
    );
    put(&app, "nuit", scene("Nuit", ping())).await;
    let (status, _) = put(&app, "cinema", scene("Ciné", ping())).await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = common::send(&app, Method::GET, "/api/scenes", Some(&member), None).await;
    let names: Vec<&str> = body["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Ciné", "Nuit"], "replaced in its place");

    let (status, body) =
        common::send_authed(&app, Method::DELETE, "/api/scenes/cinema", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["success"], json!(true));
    let (status, _) = common::send_authed(&app, Method::DELETE, "/api/scenes/cinema", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn what_a_scene_may_hold() {
    let app = test_app(None);
    let too_many = Value::Array(vec![ping()[0].clone(); 21]);
    let twenty = Value::Array(vec![ping()[0].clone(); 20]);
    for (id, body, code) in [
        ("Cinema", scene("Cinéma", ping()), "invalid"),
        (&"a".repeat(33) as &str, scene("Cinéma", ping()), "invalid"),
        ("cinema", scene("  ", ping()), "bad_name"),
        (
            "cinema",
            json!({ "name": "Cinéma", "icon": "Film!", "actions": ping() }),
            "invalid",
        ),
        ("cinema", scene("Cinéma", json!([])), "invalid"),
        ("cinema", scene("Cinéma", too_many), "too_many_actions"),
        (
            "cinema",
            scene(
                "Cinéma",
                json!([{ "action": "androidtv_app", "package": "not a package" }]),
            ),
            "invalid",
        ),
    ] {
        let (status, answer) = put(&app, id, body.clone()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{id} {body}: {answer}");
        assert_eq!(answer["success"], json!(false));
        assert_eq!(answer["code"], json!(code), "{body}");
    }
    let nested = scene("Cinéma", json!([{ "action": "scene", "scene": "nuit" }]));
    let (status, answer) = put(&app, "cinema", nested).await;
    assert!(status.is_client_error(), "a scene runs no scene: {answer}");
    assert_eq!(answer["code"], json!("invalid"));
    let (status, _) = put(&app, "cinema", scene("Cinéma", twenty)).await;
    assert_eq!(status, StatusCode::OK, "twenty is the most");
    let (_, body) = common::send_authed(&app, Method::GET, "/api/scenes", None).await;
    assert_eq!(
        body["scenes"].as_array().unwrap().len(),
        1,
        "nothing refused was kept"
    );
}

/// Members see and run the scenes; making them is an admin's.
#[tokio::test]
async fn members_run_admins_make() {
    let app = test_app(Some(
        json!([{ "id": "cinema", "name": "Cinéma", "icon": "film", "actions": ping() }]),
    ));
    let member = common::member_token();
    for (method, path, body) in [
        (Method::PUT, "/api/scenes/nuit", Some(scene("Nuit", ping()))),
        (Method::DELETE, "/api/scenes/cinema", None),
    ] {
        let (status, answer) = common::send(&app, method.clone(), path, Some(&member), body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}: {answer}");
        assert_eq!(answer["code"], json!("forbidden"));
    }
    let (status, _) = common::send(&app, Method::GET, "/api/scenes", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = common::send(&app, Method::POST, "/api/scenes/cinema/run", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, body) = common::send(
        &app,
        Method::POST,
        "/api/scenes/cinema/run",
        Some(&member),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["success"],
        json!(false),
        "the rabbit has no host: {body}"
    );
    assert_eq!(body["message"], json!("0/1 actions ok"));
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert!(rabbit_failed(&results[0]), "{body}");

    let (status, _) = common::send(
        &app,
        Method::POST,
        "/api/scenes/nope/run",
        Some(&member),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// « Je pars » as the web's template makes it (lamps, Hue lamps, plugs, the AC off, the
/// shutters closed) is a scene like any other: saved, and run from the dashboard.
#[tokio::test]
async fn leaving_turns_off_the_ac_and_closes_the_shutters() {
    let leave = json!([
        { "action": "zigbee_power", "lamp": "zb-1", "state": "off" },
        { "action": "hue_power", "lamp": "aabbccddeeff", "state": "off" },
        { "action": "meross_power", "device": "p1", "state": "off" },
        { "action": "climate_off", "host": "8.8.8.8" },
        { "action": "cover", "cover": "00000000000000aa", "command": "close" }
    ]);
    let app = test_app(None);
    let (status, body) = put(
        &app,
        "je-pars",
        json!({ "name": "Je pars", "icon": "log-out", "actions": leave }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["scene"]["actions"], leave, "kept as sent");

    let (status, body) =
        common::send_authed(&app, Method::POST, "/api/scenes/je-pars/run", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 5, "every action ran: {body}");
    assert_eq!(
        results[4],
        json!({ "action": leave[4], "ok": false, "code": "not_found" }),
        "no such shutter"
    );
}
