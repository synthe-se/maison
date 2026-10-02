//! The session once someone is in (signing in is `routes::passkeys`): a short-lived access
//! token and a rotating refresh token, both HttpOnly cookies. A refresh reads the person
//! again, so a removed person or a changed role takes effect within one access token.

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header::SET_COOKIE},
    response::{AppendHeaders, IntoResponse},
    routing::post,
};
use axum_extra::extract::cookie::{Cookie, SameSite};
use chrono::{Duration, Utc};
use jsonwebtoken::{EncodingKey, Header, encode};
use serde::Serialize;
use time::Duration as CookieDuration;

use crate::{
    AppState,
    auth::{AuthUser, AuthenticatedUser, Claims, RefreshEntry, cookie_value, decode_token, extract_auth_token},
    error::AppError,
    people::{Person, random_secret},
};

/// Cookie name for the refresh token.
const REFRESH_COOKIE_NAME: &str = "maison_refresh";
/// The refresh cookie only travels to these routes.
const REFRESH_COOKIE_PATH: &str = "/api/auth";

/// Access token lifetime in minutes.
const ACCESS_TOKEN_MINUTES: i64 = 15;

/// Refresh token lifetime in days (sliding window).
const REFRESH_TOKEN_DAYS: i64 = 7;

#[derive(Debug, Serialize)]
pub(crate) struct SessionResponse {
    success: bool,
    user: AuthUser,
}

impl SessionResponse {
    pub(crate) fn of(person: &Person) -> Json<Self> {
        Json(Self { success: true, user: AuthUser::of(person) })
    }
}

#[derive(Debug, Serialize)]
struct LogoutResponse {
    success: bool,
    message: &'static str,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/verify", post(verify_handler))
        .route("/logout", post(logout_handler))
        .route("/logout-everywhere", post(logout_everywhere_handler))
        .route("/refresh", post(refresh_handler))
}

type SessionCookies = AppendHeaders<[(axum::http::HeaderName, String); 2]>;

/// The two cookies of a fresh session for `person`: access token and refresh token.
pub(crate) async fn open_session(state: &AppState, person: &Person) -> Result<SessionCookies, AppError> {
    let claims = Claims {
        user_id: person.id.clone(),
        name: person.name.clone(),
        role: person.role.clone(),
        exp: (Utc::now() + Duration::minutes(ACCESS_TOKEN_MINUTES)).timestamp() as usize,
    };
    let token = encode(&Header::default(), &claims, &EncodingKey::from_secret(state.config.jwt_secret.as_bytes()))?;
    let refresh_token = random_secret();
    state
        .refresh_store
        .insert(
            refresh_token.clone(),
            RefreshEntry {
                user_id: person.id.clone(),
                expires_at: (Utc::now() + Duration::days(REFRESH_TOKEN_DAYS)).timestamp(),
            },
        )
        .await;
    Ok(AppendHeaders([
        (SET_COOKIE, cookie(state, state.config.auth_cookie_name.clone(), token, "/", CookieDuration::minutes(ACCESS_TOKEN_MINUTES))),
        (SET_COOKIE, cookie(state, REFRESH_COOKIE_NAME.into(), refresh_token, REFRESH_COOKIE_PATH, CookieDuration::days(REFRESH_TOKEN_DAYS))),
    ]))
}

/// Both cookies, emptied.
fn closed_session(state: &AppState) -> SessionCookies {
    AppendHeaders([
        (SET_COOKIE, cookie(state, state.config.auth_cookie_name.clone(), String::new(), "/", CookieDuration::ZERO)),
        (SET_COOKIE, cookie(state, REFRESH_COOKIE_NAME.into(), String::new(), REFRESH_COOKIE_PATH, CookieDuration::ZERO)),
    ])
}

fn cookie(state: &AppState, name: String, value: String, path: &'static str, max_age: CookieDuration) -> String {
    Cookie::build((name, value))
        .http_only(true)
        .secure(state.config.auth_cookie_secure)
        .same_site(SameSite::Lax)
        .path(path)
        .max_age(max_age)
        .build()
        .to_string()
}

async fn verify_handler(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<SessionResponse>, AppError> {
    let token = extract_auth_token(&headers, &state.config.auth_cookie_name)?;
    let decoded = decode_token(token, state.config.jwt_secret.as_bytes())
        .map_err(|_| AppError::http(StatusCode::UNAUTHORIZED, "Invalid token"))?;
    Ok(Json(SessionResponse { success: true, user: decoded.claims.into() }))
}

async fn refresh_handler(State(state): State<AppState>, headers: HeaderMap) -> Result<impl IntoResponse, AppError> {
    let refresh_token = refresh_cookie(&headers).ok_or_else(|| AppError::http(StatusCode::UNAUTHORIZED, "No refresh token"))?;
    let entry = state
        .refresh_store
        .validate(refresh_token)
        .await
        .ok_or_else(|| AppError::http(StatusCode::UNAUTHORIZED, "Invalid or expired refresh token"))?;
    // Rotate: the old token goes whatever happens next.
    state.refresh_store.remove(refresh_token).await;
    let person = state
        .people
        .person(&entry.user_id)
        .await?
        .ok_or_else(|| AppError::http(StatusCode::UNAUTHORIZED, "No such person any more"))?;
    Ok((open_session(&state, &person).await?, SessionResponse::of(&person)))
}

async fn logout_handler(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Some(refresh_token) = refresh_cookie(&headers) {
        state.refresh_store.remove(refresh_token).await;
    }
    (closed_session(&state), Json(LogoutResponse { success: true, message: "Logged out successfully" }))
}

/// A lost phone: every refresh token of this person goes, this browser's too (its access
/// token lasts its few minutes at most).
async fn logout_everywhere_handler(State(state): State<AppState>, user: AuthenticatedUser) -> impl IntoResponse {
    let ended = state.refresh_store.remove_person(&user.0.id, None).await;
    tracing::info!(person = %user.0.id, ended, "signed out everywhere");
    (closed_session(&state), Json(LogoutResponse { success: true, message: "Logged out everywhere" }))
}

/// This browser's refresh token, if any.
pub(crate) fn refresh_cookie(headers: &HeaderMap) -> Option<&str> {
    cookie_value(headers, REFRESH_COOKIE_NAME)
}
