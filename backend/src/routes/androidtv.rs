use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, State, multipart::MultipartError},
    routing::{get, post, put},
};
use futures::TryStreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    AppState,
    androidtv::{self, AndroidKey, AndroidTvConfig, AndroidTvStatus},
    auth::AdminUser,
    error::AppError,
    routes::{Answer, SimpleResponse},
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    config: AndroidTvConfig,
    status: AndroidTvStatus,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairFinishRequest {
    code: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeyRequest {
    key: AndroidKey,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaunchRequest {
    package: String,
    /// Power the television on and route it to the box first — launching an
    /// app on a set that is off or on another input is rarely what is meant.
    #[serde(default = "crate::util::default_true")]
    ensure_tv_on: bool,
}

/// 96 MB covers any sideloaded app worth the name while staying survivable
/// on a 512 MB Pi.
const APK_SIZE_LIMIT: usize = 96 * 1024 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(status))
        .route("/config", put(set_config))
        .route("/key", post(send_key))
        .route("/launch", post(launch))
        .route("/apps", get(apps))
        .route("/pair/start", post(pair_start))
        .route("/pair/finish", post(pair_finish))
        .route("/wake", post(wake))
        .route("/sleep", post(sleep))
        .route(
            "/apk",
            // APKs are large and the Pi is small; cap the upload well under
            // what the box and the backend can hold at once.
            post(install_apk).layer(DefaultBodyLimit::max(APK_SIZE_LIMIT)),
        )
}

async fn status(State(state): State<AppState>) -> Json<Answer<Status>> {
    Answer::ok(Status {
        config: state.androidtv.config().await,
        status: state.androidtv.status().await,
    })
}

async fn set_config(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<AndroidTvConfig>,
) -> Result<Json<SimpleResponse>, AppError> {
    state.androidtv.set_config(body).await?;
    Ok(SimpleResponse::ok("Android TV configuration saved"))
}

async fn send_key(
    State(state): State<AppState>,
    Json(body): Json<KeyRequest>,
) -> Result<Json<SimpleResponse>, AppError> {
    state.androidtv.send_key(body.key).await?;
    Ok(SimpleResponse::ok("Key sent"))
}

async fn launch(
    State(state): State<AppState>,
    Json(body): Json<LaunchRequest>,
) -> Result<Json<SimpleResponse>, AppError> {
    androidtv::launch_with_tv(&state, &body.package, body.ensure_tv_on).await?;
    Ok(SimpleResponse::ok(format!("Launched {}", body.package)))
}

async fn apps(State(state): State<AppState>) -> Result<Json<Answer<Value>>, AppError> {
    Ok(Answer::ok(json!({ "packages": state.androidtv.apps().await? })))
}

async fn wake(State(state): State<AppState>) -> Result<Json<SimpleResponse>, AppError> {
    state.androidtv.wake().await?;
    Ok(SimpleResponse::ok("Box woken (CEC should power the TV on)"))
}

async fn sleep(State(state): State<AppState>) -> Result<Json<SimpleResponse>, AppError> {
    state.androidtv.sleep().await?;
    Ok(SimpleResponse::ok("Box asleep (CEC should power the TV off)"))
}

/// Streams the `apk` field through to the box. One install at a time: a second one
/// is refused (409) before its upload is read.
async fn install_apk(
    State(state): State<AppState>,
    _admin: AdminUser,
    mut multipart: Multipart,
) -> Result<Json<SimpleResponse>, AppError> {
    let _permit = state.androidtv.install_permit()?;
    let malformed = |error: MultipartError| {
        tracing::debug!(%error, "unreadable APK upload");
        AppError::bad_request("malformed upload")
    };
    while let Some(field) = multipart.next_field().await.map_err(malformed)? {
        if field.name() == Some("apk") {
            let message = state.androidtv.install_apk(field.map_err(malformed)).await?;
            return Ok(SimpleResponse::ok(message));
        }
    }
    Err(AppError::bad_request("no `apk` field in the upload"))
}

/// Opens a pairing session. The TV shows a six hex-digit code once this
/// returns; the caller then posts it to `/pair/finish`.
async fn pair_start(State(state): State<AppState>, _admin: AdminUser) -> Result<Json<SimpleResponse>, AppError> {
    state.androidtv.start_pairing().await?;
    Ok(SimpleResponse::ok("Enter the code shown on the TV"))
}

async fn pair_finish(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<PairFinishRequest>,
) -> Result<Json<SimpleResponse>, AppError> {
    state.androidtv.finish_pairing(&body.code).await?;
    Ok(SimpleResponse::ok("Paired — keys now go over the fast channel"))
}
