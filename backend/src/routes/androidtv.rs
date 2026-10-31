use axum::{
    extract::{multipart::MultipartError, DefaultBodyLimit, Multipart, State},
    routing::{get, post, put},
    Json, Router,
};
use futures::TryStreamExt;
use maison_api::{
    androidtv::{Cleanup, KeyRequest, LaunchRequest},
    Message,
};
use serde::{Deserialize, Serialize};

use crate::{
    androidtv::{self, AndroidTvConfig, AndroidTvStatus},
    auth::AdminUser,
    box_clean,
    error::{AppError, Refusal},
    routes::{Answer, Reply},
    util::refused,
    AppState,
};

/// `{success: true, message}`.
type Said = Json<Answer<Message>>;

fn said(message: impl Into<String>) -> Said {
    Json(Answer::said(message))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Apps {
    packages: Vec<String>,
}

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
#[serde(rename_all = "snake_case")]
enum CleanAction {
    Clean,
    Restore,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CleanRequest {
    action: CleanAction,
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
        .route("/cleanup", get(cleanup).post(clean))
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
) -> Result<Said, AppError> {
    state.androidtv.set_config(body).await?;
    Ok(said("Android TV configuration saved"))
}

async fn send_key(
    State(state): State<AppState>,
    Json(body): Json<KeyRequest>,
) -> Result<Said, AppError> {
    state.androidtv.send_key(body.key).await?;
    Ok(said("Key sent"))
}

async fn launch(
    State(state): State<AppState>,
    Json(body): Json<LaunchRequest>,
) -> Result<Said, AppError> {
    androidtv::launch_with_tv(&state, &body.package, body.ensure_tv_on).await?;
    Ok(said(format!("Launched {}", body.package)))
}

async fn apps(State(state): State<AppState>) -> Result<Json<Answer<Apps>>, AppError> {
    Ok(Answer::ok(Apps {
        packages: state.androidtv.apps().await?,
    }))
}

async fn cleanup(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<Json<Answer<Cleanup>>, AppError> {
    Ok(Answer::ok(box_clean::read(&state.androidtv).await?))
}

/// Cuts what the table names, or gives it all back: reversible both ways, an admin's.
async fn clean(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<CleanRequest>,
) -> Result<Json<Answer<Cleanup>>, AppError> {
    let box_ = &state.androidtv;
    Ok(Answer::ok(match body.action {
        CleanAction::Clean => box_clean::clean(box_).await?,
        CleanAction::Restore => box_clean::restore(box_).await?,
    }))
}

async fn wake(State(state): State<AppState>) -> Result<Said, AppError> {
    state.androidtv.wake().await?;
    Ok(said("Box woken (CEC should power the TV on)"))
}

async fn sleep(State(state): State<AppState>) -> Result<Said, AppError> {
    state.androidtv.sleep().await?;
    Ok(said("Box asleep (CEC should power the TV off)"))
}

/// Streams the `apk` field through to the box. One install at a time: a second one
/// is refused (409) before its upload is read.
async fn install_apk(
    State(state): State<AppState>,
    _admin: AdminUser,
    mut multipart: Multipart,
) -> Result<Said, AppError> {
    let _permit = state.androidtv.install_permit()?;
    let malformed = |error: MultipartError| {
        refused(Refusal::Invalid, format!("unreadable APK upload: {error}"))
    };
    while let Some(field) = multipart.next_field().await.map_err(malformed)? {
        if field.name() == Some("apk") {
            state
                .androidtv
                .install_apk(field.map_err(malformed))
                .await?;
            return Ok(said("APK installed"));
        }
    }
    Err(refused(Refusal::Invalid, "no `apk` field in the upload"))
}

/// Opens a pairing session. The TV shows a six hex-digit code once this
/// returns; the caller then posts it to `/pair/finish`.
async fn pair_start(State(state): State<AppState>, _admin: AdminUser) -> Result<Said, AppError> {
    state.androidtv.start_pairing().await?;
    Ok(said("Enter the code shown on the TV"))
}

async fn pair_finish(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<PairFinishRequest>,
) -> Result<Said, AppError> {
    state.androidtv.finish_pairing(&body.code).await?;
    Ok(said("Paired — keys now go over the fast channel"))
}
