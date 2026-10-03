//! The Meross routes against the in-process app and a fake plug on the loopback, which
//! answers the plug's protocol and keeps what it was sent.

mod common;

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    Json, Router,
    extract::State,
    http::{Method, StatusCode},
    routing::post,
};
use serde_json::{Value, json};

/// A fake plug: records each packet and answers by namespace; `refuse_togglex` makes it
/// answer `ToggleX` with an error, as older plugs do; `broken` answers HTTP 500 to all;
/// `delay` holds every answer that long.
#[derive(Clone, Default)]
struct Plug {
    received: Arc<Mutex<Vec<Value>>>,
    refuse_togglex: bool,
    broken: bool,
    delay: Duration,
}

async fn answer(State(plug): State<Plug>, Json(packet): Json<Value>) -> Result<Json<Value>, StatusCode> {
    plug.received.lock().unwrap().push(packet.clone());
    tokio::time::sleep(plug.delay).await;
    if plug.broken {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    let namespace = packet["header"]["namespace"].as_str().unwrap_or_default();
    let method = if namespace == "Appliance.Control.ToggleX" && plug.refuse_togglex { "ERROR" } else { "GETACK" };
    let payload = match namespace {
        "Appliance.System.All" => json!({ "all": {
            "system": {
                "hardware": { "type": "mss310", "version": "2.0.0", "chipType": "mt7682", "uuid": "abc", "macAddress": "aa:bb" },
                "firmware": { "version": "6.1.8", "compileTime": "2021", "innerIp": "127.0.0.1" },
            },
            "digest": { "togglex": [{ "channel": 0, "onoff": 1 }] },
        }}),
        "Appliance.Control.Electricity" => json!({ "electricity": { "channel": 0, "current": 1500, "voltage": 2301, "power": 12000 } }),
        "Appliance.System.Runtime" => json!({ "runtime": { "signal": 87 } }),
        "Appliance.Control.ConsumptionX" => json!({ "consumptionx": [
            { "date": "2026-10-01", "time": 1, "value": 1234 },
            { "date": "2026-10-02", "time": 2, "value": 1000 },
        ]}),
        _ => json!({}),
    };
    Ok(Json(json!({ "header": { "method": method }, "payload": payload })))
}

/// Starts `plug` on the loopback: its address, which is also its device id.
async fn serve(plug: Plug) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let router = Router::new().route("/config", post(answer)).with_state(plug);
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    address
}

fn app(plugs: &[&str]) -> Router {
    let config = common::isolated_config("maison-meross-routes");
    let devices = plugs
        .iter()
        .map(|ip| json!({ "name": "Desk plug", "ip": ip, "key": "plug-key" }))
        .collect::<Vec<_>>();
    std::fs::write(&config.meross_devices_path, Value::from(devices).to_string()).unwrap();
    common::app(config)
}

async fn member(app: &Router, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    common::send(app, method, path, Some(&common::member_token()), body).await
}

#[tokio::test]
async fn toggle_sends_a_signed_togglex() {
    let plug = Plug::default();
    let ip = serve(plug.clone()).await;
    let app = app(&[&ip]);

    let (status, body) = member(&app, Method::POST, &format!("/api/meross/{ip}/toggle"), Some(json!({ "on": true }))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "success": true, "device": { "id": ip, "name": "Desk plug" }, "on": true, "message": "Desk plug turned on" }));

    let received = plug.received.lock().unwrap().clone();
    assert_eq!(received.len(), 1);
    let header = &received[0]["header"];
    assert_eq!((header["method"].as_str(), header["namespace"].as_str()), (Some("SET"), Some("Appliance.Control.ToggleX")));
    let signed = format!(
        "{}plug-key{}",
        header["messageId"].as_str().unwrap(),
        header["timestamp"].as_i64().unwrap()
    );
    assert_eq!(header["sign"], format!("{:x}", md5::compute(signed)));
    assert_eq!(received[0]["payload"], json!({ "togglex": { "channel": 0, "onoff": 1 } }));
}

#[tokio::test]
async fn an_older_plug_is_switched_with_toggle() {
    let plug = Plug { refuse_togglex: true, ..Plug::default() };
    let ip = serve(plug.clone()).await;
    let app = app(&[&ip]);

    let (status, body) = member(&app, Method::POST, &format!("/api/meross/{ip}/toggle"), Some(json!({ "on": false }))).await;
    assert_eq!((status, body["message"].as_str()), (StatusCode::OK, Some("Desk plug turned off")));
    let received = plug.received.lock().unwrap().clone();
    let namespaces = received.iter().map(|packet| packet["header"]["namespace"].clone()).collect::<Vec<_>>();
    assert_eq!(namespaces, ["Appliance.Control.ToggleX", "Appliance.Control.Toggle"]);
    assert_eq!(received[1]["payload"], json!({ "channel": 0, "toggle": { "onoff": 0 } }));
}

#[tokio::test]
async fn status_electricity_and_consumption_are_read_from_the_plug() {
    let ip = serve(Plug::default()).await;
    let app = app(&[&ip]);

    let (status, body) = member(&app, Method::GET, &format!("/api/meross/{ip}/status"), None).await;
    assert_eq!(status, StatusCode::OK);
    let plug = &body["status"];
    assert_eq!((plug["online"].as_bool(), plug["on"].as_bool()), (Some(true), Some(true)));
    assert_eq!(plug["electricity"], json!({ "voltage": 230.1, "current": 1.5, "power": 12.0 }));
    assert_eq!(plug["hardware"]["mac"], "aa:bb");
    assert_eq!(plug["firmware"]["innerIp"], "127.0.0.1");
    assert_eq!(plug["wifi"]["signal"], 87);

    let (_, body) = member(&app, Method::GET, &format!("/api/meross/{ip}/electricity"), None).await;
    assert_eq!(
        (&body["electricity"]["voltage"], &body["electricity"]["current"], &body["electricity"]["power"]),
        (&json!("230.1V"), &json!("1.5A"), &json!("12W"))
    );

    let (_, body) = member(&app, Method::GET, &format!("/api/meross/{ip}/consumption"), None).await;
    assert_eq!(body["summary"], json!({ "days": 2, "totalWh": 2234, "totalKwh": 2.23 }));

    let (_, body) = member(&app, Method::POST, &format!("/api/meross/{ip}/dnd"), Some(json!({ "enabled": true }))).await;
    assert_eq!((body["dndMode"].as_bool(), body["message"].as_str()), (Some(true), Some("DND mode enabled (LED off)")));

    let (_, body) = member(&app, Method::GET, "/api/meross", None).await;
    assert_eq!((body["total"].as_u64(), body["devices"][0]["isOn"].as_bool()), (Some(1), Some(true)));
    let (status, _) = member(&app, Method::GET, "/api/meross/stats", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "the unused stats route is gone");
}

/// A plug that fails (here a 500; a timeout alike) is not asked again with `Toggle`: only
/// a protocol refusal means « try the older order ».
#[tokio::test]
async fn a_failing_plug_is_not_asked_twice() {
    let plug = Plug { broken: true, ..Plug::default() };
    let ip = serve(plug.clone()).await;
    let app = app(&[&ip]);

    let (status, body) = member(&app, Method::POST, &format!("/api/meross/{ip}/toggle"), Some(json!({ "on": true }))).await;
    assert_eq!((status, body["error"].as_str()), (StatusCode::SERVICE_UNAVAILABLE, Some("Plug unreachable")));
    assert_eq!(plug.received.lock().unwrap().len(), 1);
}

/// The list asks every plug at once: three slow plugs cost one delay, not three.
#[tokio::test]
async fn the_list_asks_every_plug_at_once() {
    let delay = Duration::from_millis(400);
    let mut ips = Vec::new();
    for _ in 0..3 {
        ips.push(serve(Plug { delay, ..Plug::default() }).await);
    }
    let app = app(&ips.iter().map(String::as_str).collect::<Vec<_>>());

    let started = Instant::now();
    let (status, body) = member(&app, Method::GET, "/api/meross", None).await;
    assert_eq!((status, body["total"].as_u64()), (StatusCode::OK, Some(3)));
    assert!(body["devices"].as_array().unwrap().iter().all(|plug| plug["isOnline"] == true));
    assert!(started.elapsed() < delay * 2, "asked one by one: {:?}", started.elapsed());
}

#[tokio::test]
async fn an_unreachable_plug_reads_offline_and_refuses_commands() {
    // port 1 on the loopback: refused at once
    let ip = "127.0.0.1:1";
    let app = app(&[ip]);

    let (status, body) = member(&app, Method::GET, &format!("/api/meross/{ip}/status"), None).await;
    assert_eq!((status, body["status"]["online"].as_bool()), (StatusCode::OK, Some(false)));
    let (status, body) = member(&app, Method::POST, &format!("/api/meross/{ip}/toggle"), Some(json!({ "on": true }))).await;
    assert_eq!((status, body["success"].as_bool()), (StatusCode::SERVICE_UNAVAILABLE, Some(false)));
    let (status, _) = member(&app, Method::GET, &format!("/api/meross/{ip}/electricity"), None).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn unknown_plugs_bad_bodies_and_strangers_are_refused() {
    let app = app(&[]);
    let (status, body) = member(&app, Method::POST, "/api/meross/10.0.0.1/toggle", Some(json!({ "on": true }))).await;
    assert_eq!((status, body["error"].as_str()), (StatusCode::NOT_FOUND, Some("Device not found")));
    let (status, _) = member(&app, Method::POST, "/api/meross/10.0.0.1/toggle", Some(json!({ "on": "yes" }))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = common::send(&app, Method::GET, "/api/meross", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // the separate on/off routes are gone: toggle says which
    let (status, _) = member(&app, Method::POST, "/api/meross/10.0.0.1/on", None).await;
    assert!(status == StatusCode::NOT_FOUND || status == StatusCode::METHOD_NOT_ALLOWED, "{status}");
}
