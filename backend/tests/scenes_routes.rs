//! /api/scenes: who may make and run them, what a scene may hold, and that a scene runs on
//! the remote keys' engine (from the dashboard, the configurator's test and a key).

mod common;

use std::sync::{Arc, Mutex, OnceLock};

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

/// The rabbit has no host in a fresh config: its action fails, saying so — which shows
/// it was run.
const NO_RABBIT: &str = "failed: No Nabaztag host configured";

fn ping() -> Value {
    json!([{ "action": "nabaztag", "command": "ping" }])
}

fn scene(name: &str, actions: Value) -> Value {
    json!({ "name": name, "icon": "film", "actions": actions })
}

/// The app on a fresh root, with a `scenes.json` and an `ir-keymap.json` when given.
fn test_app(scenes: Option<Value>, keymap: Option<Value>) -> axum::Router {
    let config = common::isolated_config("maison-rust-scenes-tests");
    if let Some(scenes) = scenes {
        std::fs::write(&config.scenes_path, scenes.to_string()).expect("scenes written");
    }
    if let Some(keymap) = keymap {
        std::fs::write(&config.ir_keymap_path, keymap.to_string()).expect("keymap written");
    }
    common::app(config)
}

async fn put(app: &axum::Router, id: &str, body: Value) -> (StatusCode, Value) {
    common::send_authed(app, Method::PUT, &format!("/api/scenes/{id}"), Some(body)).await
}

#[tokio::test]
async fn made_listed_in_order_replaced_and_removed() {
    let app = test_app(None, None);
    let member = common::member_token();
    let (status, body) = common::send(&app, Method::GET, "/api/scenes", Some(&member), None).await;
    assert_eq!((status, body), (StatusCode::OK, json!({ "success": true, "scenes": [] })), "no seeded scenes");

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
    let names: Vec<&str> = body["scenes"].as_array().unwrap().iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Ciné", "Nuit"], "replaced in its place");

    let (status, body) = common::send_authed(&app, Method::DELETE, "/api/scenes/cinema", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["success"], json!(true));
    let (status, _) = common::send_authed(&app, Method::DELETE, "/api/scenes/cinema", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn what_a_scene_may_hold() {
    let app = test_app(None, None);
    let too_many = Value::Array(vec![ping()[0].clone(); 21]);
    let twenty = Value::Array(vec![ping()[0].clone(); 20]);
    for (id, body, code) in [
        ("Cinema", scene("Cinéma", ping()), None),
        (&"a".repeat(33) as &str, scene("Cinéma", ping()), None),
        ("cinema", scene("  ", ping()), Some("bad_name")),
        ("cinema", json!({ "name": "Cinéma", "icon": "Film!", "actions": ping() }), None),
        ("cinema", scene("Cinéma", json!([])), None),
        ("cinema", scene("Cinéma", too_many), None),
        ("cinema", scene("Cinéma", json!([{ "action": "androidtv_app", "package": "not a package" }])), None),
        ("cinema", scene("Cinéma", json!([{ "action": "scene", "scene": "nuit" }])), Some("nested_scene")),
    ] {
        let (status, answer) = put(&app, id, body.clone()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{id} {body}: {answer}");
        assert_eq!(answer["success"], json!(false));
        if let Some(code) = code {
            assert_eq!(answer["code"], json!(code), "{body}");
        }
    }
    let (status, _) = put(&app, "cinema", scene("Cinéma", twenty)).await;
    assert_eq!(status, StatusCode::OK, "twenty is the most");
    let (_, body) = common::send_authed(&app, Method::GET, "/api/scenes", None).await;
    assert_eq!(body["scenes"].as_array().unwrap().len(), 1, "nothing refused was kept");
}

/// Members see and run the scenes; making them is an admin's.
#[tokio::test]
async fn members_run_admins_make() {
    let app = test_app(Some(json!([{ "id": "cinema", "name": "Cinéma", "icon": "film", "actions": ping() }])), None);
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

    let (status, body) = common::send(&app, Method::POST, "/api/scenes/cinema/run", Some(&member), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["success"], json!(false), "the rabbit has no host: {body}");
    assert_eq!(body["message"], json!("0/1 actions ok"));
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].as_str().unwrap().starts_with(NO_RABBIT), "{body}");

    let (status, _) = common::send(&app, Method::POST, "/api/scenes/nope/run", Some(&member), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// The configurator's test runs a `scene` action like a key would: the scene's actions,
/// in the same answer as a scene's own run; a scene gone is one failed result.
#[tokio::test]
async fn a_scene_action_runs_the_scene() {
    let actions = json!([ping()[0], ping()[0]]);
    let app = test_app(Some(json!([{ "id": "cinema", "name": "Cinéma", "icon": "film", "actions": actions }])), None);
    let test = |scene: &str| json!({ "actions": [{ "action": "scene", "scene": scene }] });
    let (status, body) = common::send_authed(&app, Method::POST, "/api/ir/test", Some(test("cinema"))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, run) = common::send_authed(&app, Method::POST, "/api/scenes/cinema/run", None).await;
    assert_eq!(body, run, "one engine, one answer");
    assert_eq!(body["results"].as_array().unwrap().len(), 2);

    let (status, body) = common::send_authed(&app, Method::POST, "/api/ir/test", Some(test("gone"))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["results"], json!(["failed: Unknown scene gone"]));

    let (status, _) = common::send_authed(&app, Method::POST, "/api/ir/test", Some(test("Not An Id"))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let binding = json!({ "actions": [{ "action": "scene", "scene": "cinema" }] });
    let (status, body) = common::send_authed(&app, Method::PUT, "/api/ir/keymap/60", Some(binding)).await;
    assert_eq!(status, StatusCode::OK, "a key may run a scene: {body}");
}

/// What the app logs, kept for the test to read (a key's actions run after its answer: the
/// log is where their results go).
fn captured_log() -> Arc<Mutex<Vec<u8>>> {
    static LOG: OnceLock<Arc<Mutex<Vec<u8>>>> = OnceLock::new();
    LOG.get_or_init(|| {
        let log = Arc::new(Mutex::new(Vec::new()));
        let sink = log.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_max_level(tracing::Level::INFO)
            .with_writer(move || Sink(sink.clone()))
            .finish();
        tracing::subscriber::set_global_default(subscriber).expect("one subscriber per test binary");
        log
    })
    .clone()
}

struct Sink(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn a_remote_key_bound_to_a_scene_runs_it() {
    let log = captured_log();
    let app = test_app(
        Some(json!([{ "id": "cinema", "name": "Cinéma", "icon": "film", "actions": ping() }])),
        Some(json!({ "4242": { "actions": [{ "action": "scene", "scene": "cinema" }] } })),
    );
    let (status, body) = common::send(
        &app,
        Method::POST,
        "/api/ir/key",
        Some(common::TEST_IR_TOKEN),
        Some(json!({ "code": 4242, "value": 1 })),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    let fired = async {
        loop {
            let text = String::from_utf8_lossy(&log.lock().unwrap()).to_string();
            if let Some(line) = text.lines().find(|l| l.contains("IR key fired") && l.contains("code=4242")) {
                return line.to_string();
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    };
    let line = tokio::time::timeout(std::time::Duration::from_secs(10), fired).await.expect("the key's actions ran");
    assert!(line.contains(NO_RABBIT), "the scene's action ran: {line}");
}

/// « Je pars » as the web's template makes it (lamps, Hue lamps, plugs, the AC off, the
/// shutters closed) is a scene like any other: saved, run from the dashboard, and from a key.
#[tokio::test]
async fn leaving_turns_off_the_ac_and_closes_the_shutters() {
    let log = captured_log();
    let leave = json!([
        { "action": "zigbee_power", "lamp": "zb-1", "state": "off" },
        { "action": "hue_power", "lamp": "aabbccddeeff", "state": "off" },
        { "action": "meross_power", "device": "p1", "state": "off" },
        { "action": "climate_off", "host": "8.8.8.8" },
        { "action": "cover", "cover": "00000000000000aa", "command": "close" }
    ]);
    let app = test_app(None, Some(json!({ "4343": { "actions": [{ "action": "scene", "scene": "je-pars" }] } })));
    let (status, body) = put(&app, "je-pars", json!({ "name": "Je pars", "icon": "log-out", "actions": leave })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["scene"]["actions"], leave, "kept as sent");

    let (status, body) = common::send_authed(&app, Method::POST, "/api/scenes/je-pars/run", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 5, "every action ran: {body}");
    assert_eq!(results[4], json!("failed: Unknown shutter"));

    let (status, _) = common::send(
        &app,
        Method::POST,
        "/api/ir/key",
        Some(common::TEST_IR_TOKEN),
        Some(json!({ "code": 4343, "value": 1 })),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let fired = async {
        loop {
            let text = String::from_utf8_lossy(&log.lock().unwrap()).to_string();
            if let Some(line) = text.lines().find(|l| l.contains("IR key fired") && l.contains("code=4343")) {
                return line.to_string();
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    };
    let line = tokio::time::timeout(std::time::Duration::from_secs(10), fired).await.expect("the key's actions ran");
    assert!(line.contains("Unknown shutter") && line.contains("8.8.8.8"), "the scene's actions ran: {line}");
}
