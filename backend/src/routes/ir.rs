use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use maison_api::{remote::KeyResponse, Message};
use serde::{Deserialize, Serialize};

use crate::{
    auth::{AdminUser, AuthenticatedUser, MachineClient},
    error::{AppError, Refusal},
    ir::{self, Described, IrAction, IrBinding, IrEventLog, Pressed},
    routes::{Answer, Reply},
    util::refused,
    AppState,
};

/// One evdev key event as forwarded by kird from the STB:
/// value 1 = press, 2 = autorepeat, 0 = release.
#[derive(Debug, Deserialize)]
struct KeyEvent {
    code: u16,
    value: i32,
}

fn nothing(message: String) -> Json<KeyResponse> {
    Json(KeyResponse::nothing(message))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Keymap<'a> {
    keymap: std::collections::HashMap<u16, IrBinding<Described<'a>>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
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
    let seq = state
        .ir
        .record_event(event.code, event.value, binding.is_some())
        .await;

    let Some(binding) = binding else {
        // 200 on purpose: kird treats non-2xx as an error worth logging, and
        // an unmapped key is not an error. The event lands in /recent, which
        // is how the configurator captures new keys.
        tracing::info!(code = event.code, value = event.value, "unmapped IR key");
        return Ok((
            StatusCode::OK,
            nothing(format!("Key {} is not mapped", event.code)),
        ));
    };

    let fire = event.value == 1 || (event.value == 2 && binding.repeat);
    if !fire {
        let message = format!("Key {} ignored (value {})", event.code, event.value);
        return Ok((StatusCode::OK, nothing(message)));
    }

    // Phantom double-press filter (marginal IR reception splits one hold
    // into several presses — fatal for toggles, which cancel themselves).
    if event.value == 1 && !state.ir.accept_press(event.code).await {
        tracing::info!(code = event.code, "IR press debounced (phantom double)");
        return Ok((
            StatusCode::OK,
            nothing(format!("Key {} debounced", event.code)),
        ));
    }

    // Accepted, then run: a TV power-on takes ~16 s, and kird hanging up must not stop
    // a binding halfway through its actions. Presses of one key keep their order.
    let code = event.code;
    let value = event.value;
    let worker = state.clone();
    let pressed = Pressed { code, seq };
    let queued = state.ir.run_in_order(code, async move {
        let results = ir::run_actions(&worker, &binding.actions, Some(pressed)).await;
        tracing::info!(
            code,
            value,
            label = binding.label.as_deref().unwrap_or(""),
            ?results,
            "IR key fired"
        );
    });
    if !queued {
        // 200 like a debounced press: a held key outrunning its actions is no error
        return Ok((
            StatusCode::OK,
            nothing(format!("Key {code} dropped: still busy")),
        ));
    }
    Ok((
        StatusCode::ACCEPTED,
        nothing(format!("Key {code} accepted")),
    ))
}

async fn keymap(State(state): State<AppState>, _user: AuthenticatedUser) -> Response {
    let kept = state.ir.keymap().await;
    let keymap = kept
        .iter()
        .map(|(code, binding)| (*code, binding.described()))
        .collect();
    // the bindings are borrowed from `kept`: answered while it lives
    Answer::ok(Keymap { keymap }).into_response()
}

async fn set_binding(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(code): Path<u16>,
    Json(binding): Json<IrBinding>,
) -> Result<Json<Answer<Message>>, AppError> {
    if binding.actions.is_empty() {
        return Err(refused(Refusal::Invalid, "a binding needs an action"));
    }
    ir::validate_actions(&binding.actions)?;
    state.ir.set_binding(code, binding).await?;
    Ok(Json(Answer::said(format!("Key {code} saved"))))
}

async fn remove_binding(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(code): Path<u16>,
) -> Result<Json<Answer<Message>>, AppError> {
    if !state.ir.remove_binding(code).await? {
        return Err(Refusal::NotFound.into());
    }
    Ok(Json(Answer::said(format!("Key {code} removed"))))
}

/// Last received key events, most recent first — the configurator polls this
/// while asking the user to press the button they want to map.
async fn recent(State(state): State<AppState>, _admin: AdminUser) -> Json<Answer<Recent>> {
    Answer::ok(Recent {
        events: state.ir.recent_events().await,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
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
        return Err(refused(Refusal::Invalid, "nothing to test"));
    }
    ir::validate_actions(&body.actions)?;
    Ok(Json(KeyResponse::ran(
        ir::run_actions(&state, &body.actions, None).await,
    )))
}
