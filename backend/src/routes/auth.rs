//! The session once someone is in (signing in is `routes::passkeys`): a short-lived access
//! token and a rotating refresh token, both HttpOnly cookies (`auth.rs` says what they hold).

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, header::SET_COOKIE},
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
    auth::{AuthUser, AuthenticatedUser, Claims, RefreshEntry, Taken, cookie_names, cookie_value},
    error::AppError,
    passkey::Refusal,
    people::Person,
    routes::{Answer, SimpleResponse},
    util::{now_ms, random_secret},
};

/// Access token lifetime in minutes.
const ACCESS_TOKEN_MINUTES: i64 = 15;

/// Refresh token lifetime in days (sliding window).
const REFRESH_TOKEN_DAYS: i64 = 7;

/// Who the session is: `{success, user}`.
#[derive(Debug, Serialize)]
pub(crate) struct Session {
    pub(crate) user: AuthUser,
}

pub(crate) fn session(user: AuthUser) -> Json<Answer<Session>> {
    Answer::ok(Session { user })
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/verify", post(verify_handler))
        .route("/logout", post(logout_handler))
        .route("/logout-everywhere", post(logout_everywhere_handler))
        .route("/refresh", post(refresh_handler))
}

pub(crate) type SessionCookies = AppendHeaders<[(axum::http::HeaderName, String); 2]>;

/// A fresh session for `person`: its two cookies, and who it is. `auth_ms`: when they last
/// signed in with a passkey; `family`: the sign-in a refresh rotates from (a new one when
/// none).
pub(crate) async fn open_session(
    state: &AppState,
    person: &Person,
    auth_ms: i64,
    family: Option<String>,
) -> Result<(SessionCookies, Json<Answer<Session>>), AppError> {
    let claims = Claims {
        user_id: person.id.clone(),
        issued_ms: now_ms(),
        auth_ms,
        exp: u64::try_from((Utc::now() + Duration::minutes(ACCESS_TOKEN_MINUTES)).timestamp()).unwrap_or_default(),
    };
    let token = encode(&Header::default(), &claims, &EncodingKey::from_secret(state.config.jwt_secret.as_bytes()))?;
    let refresh_token = random_secret();
    state
        .refresh_store
        .insert(
            &refresh_token,
            RefreshEntry {
                user_id: person.id.clone(),
                expires_at: (Utc::now() + Duration::days(REFRESH_TOKEN_DAYS)).timestamp(),
                family: family.unwrap_or_else(random_secret),
                auth_ms,
            },
        )
        .await;
    let (access, refresh) = cookie_names(&state.config);
    let cookies = AppendHeaders([
        (SET_COOKIE, cookie(state, access, token, CookieDuration::minutes(ACCESS_TOKEN_MINUTES))),
        (SET_COOKIE, cookie(state, refresh, refresh_token, CookieDuration::days(REFRESH_TOKEN_DAYS))),
    ]);
    Ok((cookies, session(AuthUser::of(person, auth_ms))))
}

/// Both cookies, emptied.
fn closed_session(state: &AppState) -> SessionCookies {
    let (access, refresh) = cookie_names(&state.config);
    AppendHeaders([
        (SET_COOKIE, cookie(state, access, String::new(), CookieDuration::ZERO)),
        (SET_COOKIE, cookie(state, refresh, String::new(), CookieDuration::ZERO)),
    ])
}

fn cookie(state: &AppState, name: String, value: String, max_age: CookieDuration) -> String {
    Cookie::build((name, value))
        .http_only(true)
        .secure(state.config.auth_cookie_secure)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(max_age)
        .build()
        .to_string()
}

async fn verify_handler(user: AuthenticatedUser) -> Json<Answer<Session>> {
    session(user.0)
}

async fn refresh_handler(State(state): State<AppState>, headers: HeaderMap) -> Result<impl IntoResponse, AppError> {
    let token = refresh_cookie(&state, &headers).ok_or(Refusal::NotSignedIn)?;
    let entry = match state.refresh_store.take(token).await {
        Taken::Valid(entry) => entry,
        Taken::Replayed { user_id } => {
            tracing::warn!(person = ?user_id, "a spent refresh token came back: its sign-in ended");
            return Err(Refusal::NotSignedIn.into());
        }
        Taken::Unknown => return Err(Refusal::NotSignedIn.into()),
    };
    let person = state.people.person(&entry.user_id).await?.ok_or(Refusal::NotSignedIn)?;
    open_session(&state, &person, entry.auth_ms, Some(entry.family)).await
}

async fn logout_handler(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Some(token) = refresh_cookie(&state, &headers) {
        state.refresh_store.remove(token).await;
    }
    (closed_session(&state), SimpleResponse::ok("Logged out successfully"))
}

/// A lost phone: every session of this person ends at once, access tokens included.
async fn logout_everywhere_handler(State(state): State<AppState>, user: AuthenticatedUser) -> Result<impl IntoResponse, AppError> {
    end_sessions(&state, &user.0.id).await?;
    Ok((closed_session(&state), SimpleResponse::ok("Logged out everywhere")))
}

/// Every session of `person` over, now: their refresh tokens go, their access tokens are void.
pub(crate) async fn end_sessions(state: &AppState, person: &str) -> Result<(), AppError> {
    state.people.end_sessions(person, now_ms()).await?;
    let ended = state.refresh_store.remove_person(person).await;
    tracing::info!(%person, ended, "every session ended");
    Ok(())
}

/// This browser's refresh token, if any.
pub(crate) fn refresh_cookie<'a>(state: &AppState, headers: &'a HeaderMap) -> Option<&'a str> {
    cookie_value(headers, &cookie_names(&state.config).1)
}
