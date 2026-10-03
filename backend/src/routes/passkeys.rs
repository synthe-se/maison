//! Signing in with passkeys, entry by invitation (`passkey.rs` holds the ceremonies, the
//! limits and the addresses; `people.rs` the people and their passkeys). Ariane's routes:
//!
//! - `POST /api/passkeys/login/{start,finish}`: usernameless sign-in, opens a session.
//! - `POST /api/passkeys/register/{start,finish}`: a first passkey from an invitation (opens
//!   a session), or one more for the person signed in.
//! - `GET /api/passkeys`, `PATCH`/`DELETE /api/passkeys/{id}`: my passkeys; never the last.
//! - `GET /api/invites/{token}`: who a link is for. `GET`/`POST /api/invites`,
//!   `DELETE /api/invites/{id}`: admins.

use std::{net::SocketAddr, sync::Arc};

use axum::{
    Json, Router,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use serde::{Deserialize, Serialize};
use webauthn_rs::prelude::{
    CreationChallengeResponse, CredentialID, DiscoverableKey, Passkey, PublicKeyCredential, RegisterPublicKeyCredential,
    RequestChallengeResponse, Uuid,
};

use crate::{
    AppState,
    auth::{AdminUser, AuthenticatedUser},
    error::AppError,
    passkey::{Ceremony, INVITE_DAYS, PasskeyState, Refusal, Via, client_ip, device_label},
    people::{ADMIN, Invite, MEMBER, PasskeyInfo, PersonInfo, Removed, StoredInvite, StoredPasskey, People, clean_name, slug},
    util::{hash_secret, random_secret},
    auth::REAUTH_MS,
    routes::auth::{SessionResponse, end_sessions, open_session},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/passkeys", get(list))
        .route("/passkeys/{id}", delete(remove).patch(rename))
        .route("/passkeys/login/start", post(login_start))
        .route("/passkeys/login/finish", post(login_finish))
        .route("/passkeys/register/start", post(register_start))
        .route("/passkeys/register/finish", post(register_finish))
        .route("/invites", get(list_invites).post(create_invite))
        .route("/people", get(list_people))
        .route("/people/{id}", delete(remove_person))
        .route("/invites/{key}", get(greet).delete(delete_invite))
}

fn passkeys(state: &AppState) -> Result<&Arc<PasskeyState>, AppError> {
    state.passkeys.as_ref().ok_or_else(|| Refusal::PasskeyOff.into())
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

// ---------- credentials as stored ----------

/// A credential id as text: base64url, as webauthn-rs writes it.
fn credential_text(id: &CredentialID) -> Result<String, AppError> {
    match serde_json::to_value(id)? {
        serde_json::Value::String(s) => Ok(s),
        _ => Err(AppError::http(StatusCode::INTERNAL_SERVER_ERROR, "credential id")),
    }
}

fn credential_id(text: &str) -> Option<CredentialID> {
    serde_json::from_value(serde_json::Value::String(text.into())).ok()
}

/// The credential as JSON, and whether a keychain backs it up (what the list shows).
fn stored(pk: &Passkey) -> Result<(serde_json::Value, bool), AppError> {
    let json = serde_json::to_value(pk)?;
    let backed_up = json["cred"]["backup_state"].as_bool().unwrap_or(false);
    Ok((json, backed_up))
}

fn optional_name(name: Option<&str>) -> Result<Option<String>, AppError> {
    match name.map(str::trim) {
        None | Some("") => Ok(None),
        Some(n) => clean_name(n).map(|n| Some(n.to_string())).ok_or_else(|| Refusal::BadName.into()),
    }
}

// ---------- invitations ----------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedInvite {
    pub id: String,
    /// the one-time link; shown once, only its hash is kept
    pub url: String,
    pub expires_ms: i64,
}

/// Makes an invitation for `person` (who may not exist yet). Shared by the API and
/// `maison-backend invite`.
pub async fn new_invite(
    people: &People,
    public_url: &str,
    person: &str,
    name: &str,
    admin: bool,
    created_by: Option<&str>,
) -> Result<CreatedInvite, AppError> {
    let name = clean_name(name).ok_or(Refusal::BadName)?;
    if person.is_empty() || slug(person) != person {
        return Err(AppError::http(StatusCode::BAD_REQUEST, "A person's id: lowercase letters, digits and dashes"));
    }
    let token = random_secret();
    let id = random_secret()[..16].to_string();
    let now = now_ms();
    let expires_ms = now + INVITE_DAYS * 86_400_000;
    people
        .create_invite(StoredInvite {
            id: id.clone(),
            token_hash: hash_secret(&token),
            person: person.to_string(),
            name: name.to_string(),
            role: if admin { ADMIN } else { MEMBER }.to_string(),
            created_by: created_by.map(str::to_string),
            created_ms: now,
            expires_ms,
            used_ms: None,
        })
        .await?;
    Ok(CreatedInvite { id, url: format!("{}/invite/{token}", public_url.trim_end_matches('/')), expires_ms })
}

/// A pending invitation by its token.
async fn invite_by_token(state: &AppState, token: &str) -> Result<StoredInvite, AppError> {
    state.people.invite_by_hash(&hash_secret(token), now_ms()).await?.ok_or_else(|| Refusal::InviteInvalid.into())
}

#[derive(Deserialize)]
struct InviteRequest {
    /// Someone's id, to give them their access back (a new passkey for the same person).
    /// Absent: a new person, whose id comes from the name.
    person: Option<String>,
    name: String,
    #[serde(default)]
    admin: bool,
}

/// A new person's invitation, or someone's way back in when `person` names them. A name
/// that is already someone's id is refused: an invitation must never hand over an existing
/// account by accident (« Léonard » the guest is not Léonard).
async fn create_invite(State(state): State<AppState>, admin: AdminUser, Json(req): Json<InviteRequest>) -> Result<Json<CreatedInvite>, AppError> {
    let p = passkeys(&state)?;
    let (person, recovery) = match req.person {
        Some(person) => (person, true),
        None => {
            let person = slug(&req.name);
            if let Some(existing) = state.people.person(&person).await? {
                // who has it: the admin may mean them (their access back) or someone else
                let who = serde_json::json!({ "person": { "id": existing.id, "name": existing.name } });
                return Err(AppError::from(Refusal::PersonExists).with_detail(who));
            }
            (person, false)
        }
    };
    let created = new_invite(&state.people, &p.public_url, &person, &req.name, req.admin, Some(&admin.0.id)).await?;
    if recovery {
        tracing::warn!(by = %admin.0.id, %person, "invitation to recover an account");
    } else {
        tracing::info!(by = %admin.0.id, %person, admin = req.admin, "invitation made");
    }
    Ok(Json(created))
}

/// Everyone, for the admins.
async fn list_people(State(state): State<AppState>, _admin: AdminUser) -> Result<Json<Vec<PersonInfo>>, AppError> {
    Ok(Json(state.people.people().await?))
}

/// Someone goes: their passkeys, invitations and sessions with them. Never oneself (the
/// last admin cannot lock the house).
async fn remove_person(State(state): State<AppState>, admin: AdminUser, Path(id): Path<String>) -> Result<StatusCode, AppError> {
    if id == admin.0.id {
        return Err(Refusal::Forbidden.into());
    }
    end_sessions(&state, &id).await?;
    if !state.people.remove_person(&id).await? {
        return Err(Refusal::NotFound.into());
    }
    tracing::warn!(by = %admin.0.id, person = %id, "person removed");
    Ok(StatusCode::NO_CONTENT)
}

async fn list_invites(State(state): State<AppState>, _admin: AdminUser) -> Result<Json<Vec<Invite>>, AppError> {
    passkeys(&state)?;
    Ok(Json(state.people.invites(now_ms()).await?))
}

async fn delete_invite(State(state): State<AppState>, _admin: AdminUser, Path(id): Path<String>) -> Result<StatusCode, AppError> {
    passkeys(&state)?;
    if state.people.delete_invite(&id).await? { Ok(StatusCode::NO_CONTENT) } else { Err(Refusal::NotFound.into()) }
}

#[derive(Serialize)]
struct Greeting {
    name: String,
}

/// Who the link is for, to greet them.
async fn greet(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(token): Path<String>,
) -> Result<Json<Greeting>, AppError> {
    let p = passkeys(&state)?;
    let invite = p.guarded(client_ip(addr, &headers), invite_by_token(&state, &token)).await?;
    Ok(Json(Greeting { name: invite.name }))
}

// ---------- registration ----------

#[derive(Deserialize)]
struct RegisterStart {
    /// the invitation's token; absent to add a passkey while signed in
    invite: Option<String>,
}

#[derive(Serialize)]
struct CreationOptions {
    /// to send back with the answer
    ceremony: String,
    /// for `navigator.credentials.create()`: `PublicKeyCredential.parseCreationOptionsFromJSON(options.publicKey)`
    options: CreationChallengeResponse,
}

async fn register_start(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    user: Option<AuthenticatedUser>,
    Json(req): Json<RegisterStart>,
) -> Result<Json<CreationOptions>, AppError> {
    let p = passkeys(&state)?;
    let ip = client_ip(addr, &headers);
    if user.is_none() {
        p.start_allowed(ip)?;
    }
    let fresh = Uuid::new_v4().to_string();
    let (person, display, handle, via) = match req.invite {
        Some(token) => {
            let invite = p.guarded(ip, invite_by_token(&state, &token)).await?;
            let existing = state.people.person(&invite.person).await?;
            let handle = state.people.user_handle(&invite.person, &fresh).await?;
            let display = existing.map_or_else(|| invite.name.clone(), |x| x.name);
            (invite.person, display, handle, Via::Invite { id: invite.id })
        }
        None => {
            let user = user.ok_or(Refusal::NotSignedIn)?.0;
            // a stolen session cookie alone must not be enough to plant a passkey
            if now_ms() - user.auth_ms > REAUTH_MS {
                return Err(Refusal::ReauthNeeded.into());
            }
            let me = state.people.person(&user.id).await?.ok_or(Refusal::NotSignedIn)?;
            (me.id, me.name, me.user_handle, Via::Session)
        }
    };
    let handle = Uuid::parse_str(&handle).map_err(|_| AppError::http(StatusCode::INTERNAL_SERVER_ERROR, "user handle"))?;
    let exclude: Vec<CredentialID> =
        state.people.credential_ids_of(&person).await?.iter().filter_map(|c| credential_id(c)).collect();
    let (mut options, reg) = p.webauthn.start_passkey_registration(handle, &person, &display, Some(exclude)).map_err(|e| {
        tracing::error!(error = %e, "passkey registration could not start");
        AppError::http(StatusCode::INTERNAL_SERVER_ERROR, "passkey registration could not start")
    })?;
    // a discoverable credential, so that sign-in needs no name (webauthn-rs asks for
    // "discouraged"; its registration check does not depend on it)
    if let Some(sel) = options.public_key.authenticator_selection.as_mut() {
        sel.resident_key = Some(webauthn_rs_proto::ResidentKeyRequirement::Required);
        sel.require_resident_key = true;
    }
    let ceremony = p.ceremonies.put(ip, Ceremony::Register { state: reg, person, handle, via });
    Ok(Json(CreationOptions { ceremony, options }))
}

#[derive(Deserialize)]
struct RegisterFinish {
    ceremony: String,
    /// `PublicKeyCredential.toJSON()` of the new credential
    credential: RegisterPublicKeyCredential,
    /// what the person calls it; from the device when absent
    name: Option<String>,
}

#[derive(Serialize)]
struct Registered {
    passkey: PasskeyInfo,
    #[serde(flatten)]
    session: SessionResponse,
}

async fn register_finish(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    user: Option<AuthenticatedUser>,
    Json(req): Json<RegisterFinish>,
) -> Result<Response, AppError> {
    let p = passkeys(&state)?;
    p.guarded(client_ip(addr, &headers), finish_registration(&state, p, &headers, user, req)).await
}

async fn finish_registration(
    state: &AppState,
    p: &PasskeyState,
    headers: &HeaderMap,
    user: Option<AuthenticatedUser>,
    req: RegisterFinish,
) -> Result<Response, AppError> {
    let Some(Ceremony::Register { state: reg, person, handle, via }) = p.ceremonies.take(&req.ceremony) else {
        return Err(Refusal::CeremonyExpired.into());
    };
    if matches!(via, Via::Session) && user.as_ref().is_none_or(|u| u.0.id != person) {
        return Err(Refusal::Forbidden.into());
    }
    let pk = p.webauthn.finish_passkey_registration(&req.credential, &reg).map_err(|e| {
        tracing::warn!(error = %e, %person, "passkey registration refused");
        AppError::from(Refusal::PasskeyRejected)
    })?;
    let name = optional_name(req.name.as_deref())?.unwrap_or_else(|| {
        device_label(headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).unwrap_or("")).to_string()
    });
    let (json, backed_up) = stored(&pk)?;
    let key = StoredPasskey {
        id: random_secret()[..16].to_string(),
        person: person.clone(),
        credential_id: credential_text(pk.cred_id())?,
        passkey: json,
        name,
        backed_up,
        created_ms: now_ms(),
        last_used_ms: None,
    };
    let key_id = key.id.clone();
    let info = |person: &str| {
        let person = person.to_string();
        async move {
            let found = state.people.passkeys_of(&person).await?.into_iter().find(|k| k.id == key_id);
            found.ok_or_else(|| AppError::from(Refusal::NotFound))
        }
    };
    match via {
        Via::Invite { id } => {
            let redeemed = state.people.redeem_invite(&id, &handle.to_string(), key, now_ms()).await?;
            let person = redeemed.ok_or(Refusal::InviteInvalid)?.map_err(|_| Refusal::PasskeyExists)?;
            tracing::info!(person = %person.id, "invitation redeemed: passkey registered");
            let (cookies, Json(session)) = open_session(state, &person, now_ms(), None).await?;
            Ok((cookies, Json(Registered { passkey: info(&person.id).await?, session })).into_response())
        }
        Via::Session => {
            state.people.add_passkey(key).await?.map_err(|_| Refusal::PasskeyExists)?;
            tracing::warn!(%person, "passkey added");
            let user = user.ok_or(Refusal::NotSignedIn)?.0;
            let session = SessionResponse::of(user).0;
            Ok(Json(Registered { passkey: info(&person).await?, session }).into_response())
        }
    }
}

// ---------- sign-in ----------

#[derive(Serialize)]
struct RequestOptions {
    ceremony: String,
    /// for `navigator.credentials.get()`: `publicKey` through `parseRequestOptionsFromJSON`
    options: RequestChallengeResponse,
}

/// A challenge for any passkey of this site, for a modal request from the sign-in button
/// (no autofill: the sign-in page has no field).
async fn login_start(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Json<RequestOptions>, AppError> {
    let p = passkeys(&state)?;
    let ip = client_ip(addr, &headers);
    p.start_allowed(ip)?;
    let (mut options, auth) = p.webauthn.start_discoverable_authentication().map_err(|e| {
        tracing::error!(error = %e, "passkey sign-in could not start");
        AppError::http(StatusCode::INTERNAL_SERVER_ERROR, "passkey sign-in could not start")
    })?;
    options.mediation = None;
    let ceremony = p.ceremonies.put(ip, Ceremony::Login { state: auth });
    Ok(Json(RequestOptions { ceremony, options }))
}

#[derive(Deserialize)]
struct LoginFinish {
    ceremony: String,
    /// `PublicKeyCredential.toJSON()` of the assertion
    credential: PublicKeyCredential,
}

async fn login_finish(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<LoginFinish>,
) -> Result<Response, AppError> {
    let p = passkeys(&state)?;
    p.guarded(client_ip(addr, &headers), finish_login(&state, p, req)).await
}

async fn finish_login(state: &AppState, p: &PasskeyState, req: LoginFinish) -> Result<Response, AppError> {
    let Some(Ceremony::Login { state: auth }) = p.ceremonies.take(&req.ceremony) else {
        return Err(Refusal::CeremonyExpired.into());
    };
    let (handle, raw_id) =
        p.webauthn.identify_discoverable_authentication(&req.credential).map_err(|_| Refusal::PasskeyRejected)?;
    let cred_id = credential_text(&CredentialID::from(raw_id.to_vec()))?;
    // unknown here: the app may tell the device to forget it (signalUnknownCredential)
    let (row, person) = state.people.passkey_by_credential(&cred_id).await?.ok_or(Refusal::UnknownPasskey)?;
    if person.user_handle != handle.to_string() {
        tracing::warn!(person = %person.id, "passkey sign-in refused: not this person's");
        return Err(Refusal::PasskeyRejected.into());
    }
    let mut pk: Passkey = serde_json::from_value(row.passkey)?;
    let result = p.webauthn.finish_discoverable_authentication(&req.credential, auth, &[DiscoverableKey::from(&pk)]).map_err(|e| {
        tracing::warn!(error = %e, person = %person.id, "passkey sign-in refused");
        AppError::from(Refusal::PasskeyRejected)
    })?;
    if !result.user_verified() {
        return Err(Refusal::PasskeyRejected.into());
    }
    let update = if pk.update_credential(&result) == Some(true) { Some(stored(&pk)?) } else { None };
    state.people.passkey_used(&row.id, update, now_ms()).await?;
    tracing::info!(person = %person.id, "signed in with a passkey");
    Ok(open_session(state, &person, now_ms(), None).await?.into_response())
}

// ---------- my passkeys ----------

async fn list(State(state): State<AppState>, user: AuthenticatedUser) -> Result<Json<Vec<PasskeyInfo>>, AppError> {
    passkeys(&state)?;
    Ok(Json(state.people.passkeys_of(&user.0.id).await?))
}

#[derive(Deserialize)]
struct Rename {
    name: String,
}

async fn rename(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(id): Path<String>,
    Json(req): Json<Rename>,
) -> Result<Json<PasskeyInfo>, AppError> {
    passkeys(&state)?;
    let name = clean_name(&req.name).ok_or(Refusal::BadName)?;
    state.people.rename_passkey(&user.0.id, &id, name).await?.map(Json).ok_or_else(|| Refusal::NotFound.into())
}

/// Never the last one. Every session it may have opened ends; this browser gets a fresh one.
async fn remove(State(state): State<AppState>, user: AuthenticatedUser, Path(id): Path<String>) -> Result<Response, AppError> {
    passkeys(&state)?;
    let user = user.0;
    match state.people.delete_passkey(&user.id, &id).await? {
        Removed::Yes => {
            end_sessions(&state, &user.id).await?;
            let person = state.people.person(&user.id).await?.ok_or(Refusal::NotSignedIn)?;
            let (cookies, _) = open_session(&state, &person, user.auth_ms, None).await?;
            tracing::warn!(person = %user.id, "passkey removed");
            Ok((StatusCode::NO_CONTENT, cookies).into_response())
        }
        Removed::NotFound => Err(Refusal::NotFound.into()),
        Removed::Last => Err(Refusal::LastPasskey.into()),
    }
}
