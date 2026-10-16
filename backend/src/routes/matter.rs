use axum::{
    extract::{rejection::JsonRejection, Path, Query, State},
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use maison_api::{
    lamps::RenameRequest,
    matter::{Covers, OneCover, PositionRequest, SkipRequest, SunEvent},
    sun::{NextSun, Where},
    Message,
};

use crate::{
    auth::AdminUser,
    error::{AppError, Refusal},
    matter::{Cover, CoverCommand, SunSchedule},
    routes::{Answer, Reply},
    sun::Place,
    AppState,
};

type CoverAnswer = Result<Json<Answer<OneCover>>, AppError>;

fn cover(cover: Cover) -> CoverAnswer {
    Ok(Answer::ok(OneCover { cover }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommissionRequest {
    /// Manual pairing code or `MT:` QR payload.
    code: String,
    name: String,
}

/// The sun at the house, as `GET /api/sun` says it too (`routes/root.rs`); the manager
/// computes it.
pub(crate) async fn sun_now(state: &AppState) -> Option<NextSun> {
    state.matter.sun_now().await
}

async fn where_now(state: &AppState) -> Where {
    Where {
        place: state.matter.place().await,
        sun: state.matter.sun_now().await,
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Places {
    places: Vec<Place>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchQuery {
    q: String,
    /// The reader's language, for the place names (« Londres » / « London »).
    #[serde(default = "default_language")]
    lang: String,
}

fn default_language() -> String {
    "fr".into()
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/covers", get(list))
        .route("/commission", post(commission))
        .route("/covers/{id}", get(get_cover).patch(rename).delete(remove))
        .route("/covers/{id}/open", post(open))
        .route("/covers/{id}/close", post(close))
        .route("/covers/{id}/stop", post(stop))
        .route("/covers/{id}/position", post(position))
        .route("/covers/{id}/schedule", put(schedule))
        .route("/covers/{id}/skip", post(skip))
        .route("/place", get(place).put(set_place))
        .route("/place/search", get(search_places))
}

async fn schedule(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<SunSchedule>,
) -> CoverAnswer {
    cover(state.matter.set_schedule(&id, body).await?)
}

/// « Not tonight »: skips only the schedule's next opening or closing. A body that is not
/// one (an event other than `open` or `close`) is the coded `invalid`, a 400.
async fn skip(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Result<Json<SkipRequest>, JsonRejection>,
) -> CoverAnswer {
    let Json(body) = body.map_err(|rejection| {
        tracing::debug!(%rejection, "not a skip");
        AppError::from(Refusal::Invalid)
    })?;
    let event = match body.event {
        SunEvent::Open => CoverCommand::Open,
        SunEvent::Close => CoverCommand::Close,
    };
    cover(state.matter.set_skip(&id, event, body.skip).await?)
}

async fn place(State(state): State<AppState>) -> Json<Answer<Where>> {
    Answer::ok(where_now(&state).await)
}

async fn set_place(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<Place>,
) -> Result<Json<Answer<Where>>, AppError> {
    state.matter.set_place(body).await?;
    Ok(Answer::ok(where_now(&state).await))
}

async fn search_places(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Answer<Places>>, AppError> {
    let places = state.matter.search_places(&query.q, &query.lang).await?;
    Ok(Answer::ok(Places { places }))
}

async fn list(State(state): State<AppState>) -> Json<Answer<Covers>> {
    Answer::ok(Covers {
        covers: state.matter.list().await,
    })
}

async fn get_cover(State(state): State<AppState>, Path(id): Path<String>) -> CoverAnswer {
    cover(state.matter.get(&id).await?)
}

async fn commission(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<CommissionRequest>,
) -> CoverAnswer {
    cover(state.matter.commission(&body.code, &body.name).await?)
}

async fn rename(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<RenameRequest>,
) -> CoverAnswer {
    cover(state.matter.rename(&id, &body.name).await?)
}

async fn remove(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<String>,
) -> Result<Json<Answer<Message>>, AppError> {
    state.matter.remove(&id).await?;
    Ok(Json(Answer::said("Shutter removed")))
}

async fn open(state: State<AppState>, id: Path<String>) -> CoverAnswer {
    run(state, id, CoverCommand::Open).await
}

async fn close(state: State<AppState>, id: Path<String>) -> CoverAnswer {
    run(state, id, CoverCommand::Close).await
}

async fn stop(state: State<AppState>, id: Path<String>) -> CoverAnswer {
    run(state, id, CoverCommand::Stop).await
}

/// `openPercent` 0 to 100; past it, the coded `invalid`.
async fn position(
    state: State<AppState>,
    id: Path<String>,
    Json(body): Json<PositionRequest>,
) -> CoverAnswer {
    if body.open_percent > 100 {
        return Err(Refusal::Invalid.into());
    }
    run(state, id, CoverCommand::OpenPercent(body.open_percent)).await
}

async fn run(
    State(state): State<AppState>,
    Path(id): Path<String>,
    command: CoverCommand,
) -> CoverAnswer {
    cover(state.matter.command(&id, command).await?)
}
