use axum::{
    extract::{Path, State},
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    auth::AdminUser,
    error::AppError,
    ir::{self, IrAction},
    routes::{ir::KeyResponse, Answer, SimpleResponse},
    scenes::Scene,
    AppState,
};

#[derive(Debug, Serialize)]
struct Scenes {
    scenes: Vec<Scene>,
}

#[derive(Debug, Serialize)]
struct One {
    scene: Scene,
}

#[derive(Debug, Deserialize)]
struct SceneRequest {
    name: String,
    icon: String,
    actions: Vec<IrAction>,
}

/// Everyone signed in sees and runs the scenes; making them is an admin's.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/{id}", put(save).delete(remove))
        .route("/{id}/run", post(run))
}

async fn list(State(state): State<AppState>) -> Json<Answer<Scenes>> {
    Answer::ok(Scenes { scenes: state.scenes.list().await })
}

async fn save(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<String>,
    Json(body): Json<SceneRequest>,
) -> Result<Json<Answer<One>>, AppError> {
    let scene = state.scenes.put(&id, &body.name, &body.icon, body.actions).await?;
    Ok(Answer::ok(One { scene }))
}

async fn remove(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<String>,
) -> Result<Json<SimpleResponse>, AppError> {
    if !state.scenes.remove(&id).await? {
        return Err(unknown_scene());
    }
    Ok(SimpleResponse::ok(format!("Scene {id} removed")))
}

/// Run inside the request: the person waits on it and sees each action's result.
async fn run(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<KeyResponse>, AppError> {
    let scene = state.scenes.get(&id).await.ok_or_else(unknown_scene)?;
    Ok(KeyResponse::ran(ir::run_actions(&state, &scene.actions).await))
}

fn unknown_scene() -> AppError {
    AppError::not_found("Unknown scene")
}
