//! The session once someone is in (signing in is `routes::passkeys`): a short-lived access
//! token and a rotating refresh token, both HttpOnly cookies (`auth.rs` says what they hold).
//!
//! - `POST /api/auth/verify`: who the session is.
//! - `POST /api/auth/refresh`: a new pair of cookies, until `SESSION_MAX_DAYS` after the
//!   passkey sign-in.
//! - `POST /api/auth/logout`: this sign-in ends (its access tokens too); `POST
//!   /api/auth/logout-everywhere`: every one of mine. Both clear the browser's cookies and
//!   storage (`Clear-Site-Data`).
//! - `GET /api/auth/sessions`: my sign-ins (when, where, from what); `DELETE
//!   /api/auth/sessions/{id}`: one of them ends.

use std::net::SocketAddr;

use axum::{
    extract::{Path, State},
    http::{
        header::{HeaderName, SET_COOKIE, USER_AGENT},
        HeaderMap, HeaderValue,
    },
    response::{AppendHeaders, IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use axum_extra::extract::cookie::{Cookie, SameSite};
use chrono::{Duration, Utc};
use serde::Serialize;
use time::Duration as CookieDuration;

use crate::{
    auth::{
        cookie_name, cookie_value, decode_token, encode_token, AuthUser, AuthenticatedUser, Claims,
        CookieName, Opened, RefreshEntry, SessionInfo, Taken, ACCESS_TOKEN_MINUTES,
        REFRESH_TOKEN_DAYS, SESSION_MAX_DAYS,
    },
    error::{AppError, Refusal},
    passkey::client_ip,
    people::Person,
    routes::{Answer, Reply, SimpleResponse},
    util::{now_ms, random_secret},
    AppState,
};

/// Who the session is: `{success, user}`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Session {
    pub(crate) user: AuthUser,
}

pub(crate) fn session(user: AuthUser) -> Json<Answer<Session>> {
    Answer::ok(Session { user })
}

/// My sign-ins: `{success, sessions}`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Sessions {
    sessions: Vec<SessionInfo>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/verify", post(verify_handler))
        .route("/logout", post(logout_handler))
        .route("/logout-everywhere", post(logout_everywhere_handler))
        .route("/refresh", post(refresh_handler))
        .route("/sessions", get(sessions_handler))
        .route("/sessions/{id}", delete(end_session_handler))
}

pub(crate) type SessionCookies = AppendHeaders<[(HeaderName, String); 2]>;

/// Where a sign-in comes from: its address and browser, kept with its family (the session
/// list). From the request a route already has.
pub(crate) fn opened_by(person: &str, addr: SocketAddr, headers: &HeaderMap) -> Opened {
    let user_agent = headers
        .get(USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    Opened::now(person, client_ip(addr, headers), user_agent)
}

/// A fresh session for `person`: its two cookies, and who it is. `auth_ms`: when they last
/// signed in with a passkey; `family`: the sign-in a refresh rotates from, or, for a new
/// sign-in, where it was opened.
pub(crate) async fn open_session(
    state: &AppState,
    person: &Person,
    auth_ms: i64,
    family: Result<String, Opened>,
) -> Result<(SessionCookies, Json<Answer<Session>>), AppError> {
    let (family, opened) = match family {
        Ok(family) => (family, None),
        Err(opened) => (random_secret(), Some(opened)),
    };
    let claims = Claims {
        user_id: person.id.clone(),
        issued_ms: now_ms(),
        auth_ms,
        sid: family.clone(),
        exp: u64::try_from((Utc::now() + Duration::minutes(ACCESS_TOKEN_MINUTES)).timestamp())
            .unwrap_or_default(),
    };
    let token = encode_token(&claims, state.config.jwt_secret.as_bytes())?;
    let refresh_token = random_secret();
    state
        .refresh_store
        .insert(
            &refresh_token,
            RefreshEntry {
                user_id: person.id.clone(),
                expires_at: (Utc::now() + Duration::days(REFRESH_TOKEN_DAYS)).timestamp(),
                family: family.clone(),
                auth_ms,
            },
            opened,
        )
        .await?;
    let cookies = AppendHeaders([
        (
            SET_COOKIE,
            cookie(
                state,
                cookie_name(&state.config, CookieName::Access),
                token,
                CookieDuration::minutes(ACCESS_TOKEN_MINUTES),
            ),
        ),
        (
            SET_COOKIE,
            cookie(
                state,
                cookie_name(&state.config, CookieName::Refresh),
                refresh_token,
                CookieDuration::days(REFRESH_TOKEN_DAYS),
            ),
        ),
    ]);
    Ok((cookies, session(AuthUser::of(person, auth_ms, &family))))
}

/// A new sign-in from this browser: the sign-in its refresh cookie still holds, if any, ends
/// first (a session's tokens never outlive the next sign-in).
pub(crate) async fn end_previous(state: &AppState, headers: &HeaderMap) -> Result<(), AppError> {
    match refresh_cookie(state, headers) {
        Some(old) => state.refresh_store.remove(old).await,
        None => Ok(()),
    }
}

/// Both cookies, emptied.
fn closed_session(state: &AppState) -> SessionCookies {
    let empty = |which| {
        (
            SET_COOKIE,
            cookie(
                state,
                cookie_name(&state.config, which),
                String::new(),
                CookieDuration::ZERO,
            ),
        )
    };
    AppendHeaders([empty(CookieName::Access), empty(CookieName::Refresh)])
}

/// What signing out asks the browser to forget: its cookies and what the page stored (theme,
/// language, the sun's side: nothing secret, but nothing of the person stays behind).
fn clear_site_data() -> [(HeaderName, HeaderValue); 1] {
    [(
        HeaderName::from_static("clear-site-data"),
        HeaderValue::from_static("\"cookies\", \"storage\""),
    )]
}

/// An HttpOnly, SameSite=Lax cookie on the whole site, Secure when the config says (always
/// behind the tunnel).
pub(crate) fn cookie(
    state: &AppState,
    name: String,
    value: String,
    max_age: CookieDuration,
) -> String {
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

async fn refresh_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let token = refresh_cookie(&state, &headers).ok_or(Refusal::NotSignedIn)?;
    let entry = match state.refresh_store.take(token).await? {
        Taken::Valid(entry) => entry,
        Taken::Replayed { user_id } => {
            tracing::warn!(person = ?user_id, "a spent refresh token came back: its sign-in ended");
            return Err(Refusal::NotSignedIn.into());
        }
        Taken::Unknown => return Err(Refusal::NotSignedIn.into()),
    };
    if now_ms() - entry.auth_ms > SESSION_MAX_DAYS * 86_400_000 {
        tracing::info!(person = %entry.user_id, "a session past its {SESSION_MAX_DAYS} days: a passkey again");
        state.refresh_store.end(&entry.family).await?;
        return Err(Refusal::NotSignedIn.into());
    }
    let person = state
        .people
        .person(&entry.user_id)
        .await?
        .ok_or(Refusal::NotSignedIn)?;
    open_session(&state, &person, entry.auth_ms, Ok(entry.family)).await
}

/// This sign-in ends: its refresh token and its access tokens, a copy included. Signed out
/// already, it still clears the cookies.
async fn logout_handler(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let access = cookie_name(&state.config, CookieName::Access);
    let sid = cookie_value(&headers, &access)
        .and_then(|token| decode_token(token, state.config.jwt_secret.as_bytes()).ok())
        .map(|data| data.claims.sid);
    let ended = async {
        if let Some(token) = refresh_cookie(&state, &headers) {
            state.refresh_store.remove(token).await?;
        }
        if let Some(sid) = sid {
            state.refresh_store.end(&sid).await?;
        }
        Ok::<(), AppError>(())
    };
    if let Err(error) = ended.await {
        // the cookies go anyway; the server side is said in the log
        tracing::error!(%error, "signing out: the sign-in could not be ended on the server");
    }
    (
        clear_site_data(),
        closed_session(&state),
        SimpleResponse::ok("Logged out successfully"),
    )
        .into_response()
}

/// A lost phone: every session of this person ends at once, access tokens included.
async fn logout_everywhere_handler(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<impl IntoResponse, AppError> {
    end_sessions(&state, &user.0.id).await?;
    Ok((
        clear_site_data(),
        closed_session(&state),
        SimpleResponse::ok("Logged out everywhere"),
    ))
}

/// My sign-ins, the latest first; `current` is this one.
async fn sessions_handler(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Json<Answer<Sessions>> {
    let user = user.0;
    Answer::ok(Sessions {
        sessions: state.refresh_store.sessions(&user.id, &user.sid).await,
    })
}

/// One of my sign-ins ends (its refresh and access tokens). This one too, if named: then its
/// cookies are cleared.
async fn end_session_handler(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let user = user.0;
    let current = state
        .refresh_store
        .sessions(&user.id, &user.sid)
        .await
        .into_iter()
        .any(|s| s.current && s.id == id);
    if !state.refresh_store.end_listed(&user.id, &id).await? {
        return Err(Refusal::NotFound.into());
    }
    tracing::info!(person = %user.id, session = %id, "a session ended from the list");
    let said = SimpleResponse::ok("Session ended");
    Ok(if current {
        (closed_session(&state), said).into_response()
    } else {
        said.into_response()
    })
}

/// Every session of `person` over, now: their refresh tokens go, their access tokens are void.
pub(crate) async fn end_sessions(state: &AppState, person: &str) -> Result<(), AppError> {
    state.people.end_sessions(person, now_ms()).await?;
    let ended = state.refresh_store.remove_person(person).await?;
    tracing::info!(%person, ended, "every session ended");
    Ok(())
}

/// This browser's refresh token, if any.
pub(crate) fn refresh_cookie<'a>(state: &AppState, headers: &'a HeaderMap) -> Option<&'a str> {
    cookie_value(headers, &cookie_name(&state.config, CookieName::Refresh))
}
