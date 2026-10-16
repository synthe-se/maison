//! Wall screens (`screens.rs` says how pairing goes): a screen asks for a pairing and claims it,
//! signed out; an admin pairs it by its code, lists the screens and revokes one. A paired screen
//! is a `TokenKind::Screen` service token, in a cookie only its browser holds.
//!
//! - `POST /api/screens/pairing`, `POST /api/screens/pairing/claim` `{secret}`: the screen,
//!   signed out.
//! - `GET /api/screens/me`: who the screen is (a read: nothing changes); `POST
//!   /api/screens/me`: the same, its cookie renewed (each start of its page).
//! - `GET /api/screens`, `POST /api/screens` `{code, name}` (a recent passkey sign-in),
//!   `DELETE /api/screens/{id}`: admins.

use std::net::SocketAddr;

use axum::{
    extract::{ConnectInfo, Path, State},
    http::{header::SET_COOKIE, HeaderMap},
    response::{AppendHeaders, IntoResponse},
    routing::{delete, get, post},
    Json, Router,
};
use time::Duration as CookieDuration;

use crate::{
    auth::{
        cookie_name, cookie_value, issue_service_token, recent_passkey, service_token, AdminUser,
        CookieName,
    },
    error::{AppError, Refusal},
    passkey::client_ip,
    people::{Scope, StoredToken, TokenKind},
    routes::{auth::cookie, Answer, Reply, SimpleResponse},
    screens::{Claim, Waiting, PAIRING_TTL, POLL},
    util::now_ms,
    AppState,
};

// The answers and requests: the API crate's, shared with the frame.
use maison_api::screens::{
    ClaimAnswer, ClaimRequest, Me, PairRequest, Paired, Pairing, Screen, ScreenName, Screens,
};

/// How long a screen's cookie lives without being seen: the most browsers keep one (400 days);
/// every start of the page renews it (`me`).
const SCREEN_COOKIE_DAYS: i64 = 400;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(pair))
        .route("/{id}", delete(revoke))
        .route("/pairing", post(start))
        .route("/pairing/claim", post(claim))
        .route("/me", get(me).post(renew))
}

/// A duration as the API says it, in whole milliseconds.
fn millis(d: std::time::Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

fn too_many(retry_after_s: u64) -> AppError {
    AppError::too_many(retry_after_s)
}

fn screen_cookie(
    state: &AppState,
    token: String,
) -> AppendHeaders<[(axum::http::HeaderName, String); 1]> {
    AppendHeaders([(
        SET_COOKIE,
        cookie(
            state,
            cookie_name(&state.config, CookieName::Screen),
            token,
            CookieDuration::days(SCREEN_COOKIE_DAYS),
        ),
    )])
}

/// Signed out: a browser asks to become a screen.
async fn start(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Json<Answer<Pairing>>, AppError> {
    let (secret, code) = state
        .pairings
        .start(client_ip(addr, &headers))
        .map_err(too_many)?;
    Ok(Answer::ok(Pairing {
        code,
        secret,
        expires_in_ms: millis(PAIRING_TTL),
        poll_ms: millis(POLL),
    }))
}

/// The screen asking whether an admin paired it: once paired, its cookie, once.
async fn claim(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<ClaimRequest>,
) -> Result<impl IntoResponse, AppError> {
    match state
        .pairings
        .claim(client_ip(addr, &headers), &body.secret)
        .map_err(too_many)?
    {
        Claim::Waiting => Ok((
            None,
            Answer::ok(ClaimAnswer {
                paired: false,
                screen: None,
            }),
        )),
        Claim::Paired { token, name } => {
            tracing::info!(screen = %name, "a wall screen is paired");
            Ok((
                Some(screen_cookie(&state, token)),
                Answer::ok(ClaimAnswer {
                    paired: true,
                    screen: Some(ScreenName { name }),
                }),
            ))
        }
        Claim::Gone => Err(Refusal::PairingUnknown.into()),
    }
}

/// The paired screen this cookie names, and its token. Anything but a paired screen's cookie
/// is not signed in.
async fn this_screen(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(String, StoredToken), AppError> {
    let token = cookie_value(headers, &cookie_name(&state.config, CookieName::Screen))
        .ok_or(Refusal::NotSignedIn)?
        .to_string();
    match service_token(&state.people, &token, now_ms()).await? {
        Some(found) if found.kind == TokenKind::Screen => Ok((token, found)),
        _ => Err(Refusal::NotSignedIn.into()),
    }
}

/// Who the screen is: a read, nothing set.
async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Answer<Me>>, AppError> {
    let (_, found) = this_screen(&state, &headers).await?;
    Ok(Answer::ok(Me {
        screen: ScreenName { name: found.name },
    }))
}

/// The screen, at each start of its page: who it is, its cookie renewed (a change: a POST,
/// never a link's GET).
async fn renew(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let (token, found) = this_screen(&state, &headers).await?;
    Ok((
        screen_cookie(&state, token),
        Answer::ok(Me {
            screen: ScreenName { name: found.name },
        }),
    ))
}

/// An admin pairs the screen showing `code`, as `name`, from the screen's own network, after a
/// recent passkey sign-in (a paired screen drives the house for months).
async fn pair(
    State(state): State<AppState>,
    admin_user: AdminUser,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<PairRequest>,
) -> Result<Json<Answer<Paired>>, AppError> {
    recent_passkey(&admin_user.0)?;
    let admin = client_ip(addr, &headers);
    match state.pairings.waiting(&body.code, admin) {
        Waiting::Here => {}
        Waiting::Elsewhere => return Err(Refusal::PairingElsewhere.into()),
        Waiting::No => return Err(Refusal::PairingUnknown.into()),
    }
    let name = crate::util::name(&body.name)?.to_string();
    let token = issue_service_token(&state.people, &name, Scope::Devices, TokenKind::Screen)
        .await?
        .ok_or(Refusal::ScreenExists)?;
    if !state.pairings.grant(&body.code, admin, token, name.clone()) {
        // the screen's ten minutes ended in between: its token goes with it
        state.people.revoke_token(&name).await?;
        return Err(Refusal::PairingUnknown.into());
    }
    tracing::info!(by = %admin_user.0.id, screen = %name, ip = %admin, "an admin paired a wall screen");
    Ok(Answer::ok(Paired {
        screen: ScreenName { name },
    }))
}

async fn list(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<Json<Answer<Screens>>, AppError> {
    let screens = state
        .people
        .tokens()
        .await?
        .into_iter()
        .filter(|t| t.kind == TokenKind::Screen)
        .map(|t| Screen {
            id: t.id,
            name: t.name,
            created_ms: t.created_ms,
            last_used_ms: t.last_used_ms,
        })
        .collect();
    Ok(Answer::ok(Screens { screens }))
}

/// The screen goes at once: its next request is not signed in, and it offers to pair again.
/// Only a screen's token (checked and revoked in one edit of the file).
async fn revoke(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    if !state.people.revoke_token_of(&id, TokenKind::Screen).await? {
        return Err(Refusal::NotFound.into());
    }
    tracing::warn!(by = %admin.0.id, screen = %id, "an admin revoked a wall screen");
    Ok(SimpleResponse::ok("Screen revoked"))
}
