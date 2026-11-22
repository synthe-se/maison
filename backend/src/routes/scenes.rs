use crate::error::OrRefused;
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post, put},
};
use maison_api::{Message, actions::RunResponse, scenes::SceneList};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    actions::{self, Action},
    auth::AdminUser,
    error::{AppError, Refusal},
    routes::{Answer, Reply},
    scenes::Scene,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct One {
    scene: Scene,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SceneRequest {
    name: String,
    icon: String,
    actions: Vec<Action>,
}

/// Everyone signed in sees and runs the scenes; making them is an admin's.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/{id}", put(save).delete(remove))
        .route("/{id}/run", post(run))
}

async fn list(State(state): State<AppState>) -> Json<Answer<SceneList>> {
    Answer::ok(SceneList {
        scenes: state.scenes.list().await,
    })
}

async fn save(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<String>,
    Json(body): Json<SceneRequest>,
) -> Result<Json<Answer<One>>, AppError> {
    let scene = state
        .scenes
        .put(&id, &body.name, &body.icon, body.actions)
        .await?;
    Ok(Answer::ok(One { scene }))
}

async fn remove(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<String>,
) -> Result<Json<Answer<Message>>, AppError> {
    if !state.scenes.remove(&id).await? {
        return Err(Refusal::NotFound.into());
    }
    Ok(Json(Answer::said(format!("Scene {id} removed"))))
}

/// Run inside the request: the person waits on it and sees each action's result.
async fn run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RunResponse>, AppError> {
    let scene = state.scenes.get(&id).await.or_not_found()?;
    Ok(Json(RunResponse::ran(
        actions::run(&state, &scene.actions).await,
    )))
}
