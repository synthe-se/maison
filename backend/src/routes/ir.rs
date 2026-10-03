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
    ir::{self, IrAction, IrBinding, IrEventLog},
    routes::{Answer, SimpleResponse},
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
pub(crate) struct KeyResponse {
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

    /// What a run said, action by action: the answer of `/test` and of a scene's `/run`.
    pub(crate) fn ran(results: Vec<String>) -> Json<Self> {
        let failures = results.iter().filter(|r| r.starts_with("failed")).count();
        Json(Self {
            success: failures == 0,
            message: format!("{}/{} actions ok", results.len() - failures, results.len()),
            results,
        })
    }
}

#[derive(Debug, Serialize)]
struct Keymap {
    keymap: HashMap<u16, serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct Recent {
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
    let queued = state.ir.run_in_order(code, async move {
        let results = ir::run_actions(&worker, &binding.actions).await;
        tracing::info!(code, value, label = binding.label.as_deref().unwrap_or(""), ?results, "IR key fired");
    });
    if !queued {
        // 200 like a debounced press: a held key outrunning its actions is no error
        return Ok((StatusCode::OK, KeyResponse::nothing(format!("Key {code} dropped: still busy"))));
    }
    Ok((StatusCode::ACCEPTED, KeyResponse::nothing(format!("Key {code} accepted"))))
}

async fn keymap(State(state): State<AppState>, _user: AuthenticatedUser) -> Json<Answer<Keymap>> {
    let keymap = state.ir.keymap().await.into_iter().map(|(code, binding)| (code, ir::described(&binding))).collect();
    Answer::ok(Keymap { keymap })
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
    ir::validate_actions(&binding.actions).map_err(AppError::bad_request)?;
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
async fn recent(State(state): State<AppState>, _admin: AdminUser) -> Json<Answer<Recent>> {
    Answer::ok(Recent { events: state.ir.recent_events().await })
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
    ir::validate_actions(&body.actions).map_err(AppError::bad_request)?;
    Ok(KeyResponse::ran(ir::run_actions(&state, &body.actions).await))
}
