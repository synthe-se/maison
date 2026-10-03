use std::collections::HashMap;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post, put},
};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    auth::{AdminUser, AuthenticatedUser, MachineClient},
    error::AppError,
    ir::{IrAction, IrBinding, IrEventLog},
    routes::SimpleResponse,
    tv,
};

/// One evdev key event as forwarded by kird from the STB:
/// value 1 = press, 2 = autorepeat, 0 = release.
#[derive(Debug, Deserialize)]
struct KeyEvent {
    code: u16,
    value: i32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct KeyResponse {
    success: bool,
    message: String,
    /// One entry per executed action ("ok: ..." / "failed: ..."); empty for a key press,
    /// whose actions run after the answer.
    results: Vec<String>,
}

impl KeyResponse {
    fn nothing(message: String) -> Json<Self> {
        Json(Self { success: true, message, results: Vec::new() })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct KeymapResponse {
    success: bool,
    keymap: HashMap<u16, IrBinding>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecentResponse {
    success: bool,
    events: Vec<IrEventLog>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        // Machine route (kird on the STB, IR_API_TOKEN)
        .route("/key", post(key_event))
        // Configurator routes (frontend, user session)
        .route("/keymap", get(keymap))
        .route("/keymap/{code}", put(set_binding).delete(remove_binding))
        .route("/recent", get(recent))
        .route("/test", post(test_actions))
}

async fn key_event(
    State(state): State<AppState>,
    _machine: MachineClient,
    Json(event): Json<KeyEvent>,
) -> Result<(StatusCode, Json<KeyResponse>), AppError> {
    let binding = state.ir.binding(event.code).await;
    state
        .ir
        .record_event(event.code, event.value, binding.is_some())
        .await;

    let Some(binding) = binding else {
        // 200 on purpose: kird treats non-2xx as an error worth logging, and
        // an unmapped key is not an error. The event lands in /recent, which
        // is how the configurator captures new keys.
        tracing::info!(code = event.code, value = event.value, "unmapped IR key");
        return Ok((StatusCode::OK, KeyResponse::nothing(format!("Key {} is not mapped", event.code))));
    };

    let fire = event.value == 1 || (event.value == 2 && binding.repeat);
    if !fire {
        let message = format!("Key {} ignored (value {})", event.code, event.value);
        return Ok((StatusCode::OK, KeyResponse::nothing(message)));
    }

    // Phantom double-press filter (marginal IR reception splits one hold
    // into several presses — fatal for toggles, which cancel themselves).
    if event.value == 1 && !state.ir.accept_press(event.code).await {
        tracing::info!(code = event.code, "IR press debounced (phantom double)");
        return Ok((StatusCode::OK, KeyResponse::nothing(format!("Key {} debounced", event.code))));
    }

    // Accepted, then run: a TV power-on takes ~16 s, and kird hanging up must not stop
    // a binding halfway through its actions. Presses of one key keep their order.
    let code = event.code;
    let value = event.value;
    let worker = state.clone();
    state.ir.run_in_order(code, async move {
        let results = execute_all(&worker, &binding.actions).await;
        tracing::info!(code, value, label = binding.label.as_deref().unwrap_or(""), ?results, "IR key fired");
    });
    Ok((StatusCode::ACCEPTED, KeyResponse::nothing(format!("Key {code} accepted"))))
}

/// Runs every action in order; a failing action never stops the others.
async fn execute_all(state: &AppState, actions: &[IrAction]) -> Vec<String> {
    let mut results = Vec::with_capacity(actions.len());
    for action in actions {
        match execute(state, action).await {
            Ok(message) => results.push(format!("ok: {message}")),
            Err(error) => results.push(format!("failed: {error}")),
        }
    }
    results
}

async fn execute(state: &AppState, action: &IrAction) -> Result<String, AppError> {
    match action {
        IrAction::Nabaztag { command } => {
            state.nabaztag.send_command(command).await?;
            Ok(format!("Nabaztag: {command}"))
        }
        IrAction::ZigbeePower { lamp, state: switch } => {
            let on = switch
                .resolve(|| async {
                    let view = state.zigbee.get_lamp(lamp).await;
                    Ok(view.ok_or_else(|| AppError::not_found(format!("Unknown Zigbee lamp {lamp}")))?.state.is_on)
                })
                .await?;
            state.zigbee.set_power(lamp, on).await?;
            Ok(format!("Zigbee {lamp}: power {}", if on { "on" } else { "off" }))
        }
        IrAction::ZigbeeBrightness { lamp, brightness } => {
            state.zigbee.set_brightness(lamp, *brightness).await?;
            Ok(format!("Zigbee {lamp}: brightness {brightness}"))
        }
        IrAction::BroadlinkCode { host, code_id } => {
            state
                .broadlink
                .send_saved_code(host.clone(), None, code_id.clone())
                .await?;
            Ok(format!("Broadlink {host}: {code_id}"))
        }
        IrAction::MerossPower { device, state: switch } => {
            let on = switch.resolve(|| async { Ok(state.meross.get_status(device).await?.1.on) }).await?;
            state.meross.toggle(device, on).await?;
            Ok(format!("Meross {device}: {}", if on { "on" } else { "off" }))
        }
        IrAction::TvPower {
            state: switch,
            switch_to_box,
        } => {
            let power = tv::tv_power(state, *switch, *switch_to_box).await?;
            Ok(format!("TV: {power:?}"))
        }
        IrAction::TvKey { key } => {
            state.tv.send_key(*key).await?;
            Ok(format!("TV key: {key:?}"))
        }
        IrAction::TvVolume { level } => {
            let volume = state.tv.set_volume(*level, None).await?;
            Ok(format!("TV volume: {}", volume.current))
        }
        IrAction::TvAmbilight { state: switch } => {
            let on = switch.resolve(|| async { Ok(state.tv.ambilight().await?.power) }).await?;
            state.tv.set_ambilight_power(on).await?;
            Ok(format!("TV Ambilight: {}", if on { "on" } else { "off" }))
        }
        IrAction::AndroidTvApp {
            package,
            ensure_tv_on,
        } => {
            crate::androidtv::launch_with_tv(state, package, *ensure_tv_on).await?;
            Ok(format!("Android TV: launched {package}"))
        }
        IrAction::AndroidTvKey { key } => {
            state.androidtv.send_key(*key).await?;
            Ok(format!("Android TV key: {key:?}"))
        }
        IrAction::ClimateToggle {
            host,
            on_command,
            model,
        } => {
            // The remote keeps emitting repeat frames while the button is
            // held, and from parts of the room those frames also reach the
            // AC's receiver — colliding with the RM4 blast and corrupting
            // it. Wait for the button to be released before transmitting.
            tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
            // IR is one-way: the stored state is the last commanded one.
            let is_on = state
                .broadlink
                .climate_state()
                .await
                .map(|s| s.power)
                .unwrap_or(false);
            let command = if is_on { "state-off" } else { on_command.as_str() };
            state
                .broadlink
                .send_mitsubishi_command(host.clone(), None, command.to_string(), model.clone())
                .await?;
            Ok(format!("Climate {host}: {command}"))
        }
    }
}

async fn keymap(State(state): State<AppState>, _user: AuthenticatedUser) -> Json<KeymapResponse> {
    Json(KeymapResponse {
        success: true,
        keymap: state.ir.keymap().await,
    })
}

async fn set_binding(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(code): Path<u16>,
    Json(binding): Json<IrBinding>,
) -> Result<Json<SimpleResponse>, AppError> {
    if binding.actions.is_empty() {
        return Err(AppError::bad_request("A binding needs at least one action"));
    }
    crate::ir::validate_actions(&binding.actions).map_err(AppError::bad_request)?;
    state.ir.set_binding(code, binding).await?;
    Ok(SimpleResponse::ok(format!("Key {code} saved")))
}

async fn remove_binding(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(code): Path<u16>,
) -> Result<Json<SimpleResponse>, AppError> {
    if !state.ir.remove_binding(code).await? {
        return Err(AppError::not_found(format!("Key {code} is not mapped")));
    }
    Ok(SimpleResponse::ok(format!("Key {code} removed")))
}

/// Last received key events, most recent first — the configurator polls this
/// while asking the user to press the button they want to map.
async fn recent(State(state): State<AppState>, _admin: AdminUser) -> Json<RecentResponse> {
    Json(RecentResponse {
        success: true,
        events: state.ir.recent_events().await,
    })
}

#[derive(Debug, Deserialize)]
struct TestRequest {
    actions: Vec<IrAction>,
}

/// Dry-run a list of actions from the configurator ("test this binding"
/// button) without saving anything. Run inside the request: the configurator shows
/// each action's result.
async fn test_actions(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<TestRequest>,
) -> Result<Json<KeyResponse>, AppError> {
    if body.actions.is_empty() {
        return Err(AppError::bad_request("Nothing to test"));
    }
    crate::ir::validate_actions(&body.actions).map_err(AppError::bad_request)?;
    let results = execute_all(&state, &body.actions).await;
    let failures = results.iter().filter(|r| r.starts_with("failed")).count();
    Ok(Json(KeyResponse {
        success: failures == 0,
        message: format!("{}/{} actions ok", results.len() - failures, results.len()),
        results,
    }))
}
