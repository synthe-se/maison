use axum::{
    extract::{multipart::MultipartError, DefaultBodyLimit, Multipart, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use maison_api::{
    photos::{Added, PhotoList, MAX_PHOTO_BYTES, UPLOAD_FIELD},
    Message,
};

use crate::{
    auth::AdminUser,
    error::{AppError, Refusal},
    routes::{Answer, Reply},
    util::refused,
    AppState,
};

/// One photo and its multipart wrapping: a photo over the limit is refused by its own check when
/// it fits here, by this one when it does not. Held in memory: small, one upload at a time.
const BODY_LIMIT: usize = MAX_PHOTO_BYTES + 16 * 1024;

/// Whoever drives the house reads them, a paired wall screen first of all; adding and removing
/// are an admin's, which a service token never is.
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(list).post(add).layer(DefaultBodyLimit::max(BODY_LIMIT)),
        )
        .route("/{id}", get(one).delete(remove))
}

async fn list(State(state): State<AppState>) -> Result<Json<Answer<PhotoList>>, AppError> {
    Ok(Answer::ok(PhotoList {
        photos: state.photos.list().await?,
    }))
}

/// The bytes as they came; their id is their hash, so `lib.rs`'s cache policy keeps them.
async fn one(State(state): State<AppState>, Path(id): Path<String>) -> Result<Response, AppError> {
    let bytes = state.photos.read(&id).await?;
    Ok(([(header::CONTENT_TYPE, "image/jpeg")], bytes).into_response())
}

async fn add(
    State(state): State<AppState>,
    _admin: AdminUser,
    mut multipart: Multipart,
) -> Result<Json<Answer<Added>>, AppError> {
    let unread = |error: MultipartError| match error.status() {
        StatusCode::PAYLOAD_TOO_LARGE => AppError::from(Refusal::TooLarge),
        _ => refused(
            Refusal::Invalid,
            format!("unreadable photo upload: {error}"),
        ),
    };
    while let Some(field) = multipart.next_field().await.map_err(unread)? {
        if field.name() == Some(UPLOAD_FIELD) {
            let bytes = field.bytes().await.map_err(unread)?;
            let photo = state.photos.add(bytes).await?;
            return Ok(Answer::ok(Added { photo }));
        }
    }
    Err(refused(Refusal::Invalid, "no `photo` field in the upload"))
}

async fn remove(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<String>,
) -> Result<Json<Answer<Message>>, AppError> {
    if !state.photos.remove(&id).await? {
        return Err(Refusal::NotFound.into());
    }
    Ok(Json(Answer::said("Photo removed")))
}
