//! Signing in with passkeys, entry by invitation, and the people (`passkey.rs` holds the
//! ceremonies, the limits and the addresses; `people.rs` the people and their passkeys).
//! Ariane's routes, every answer in the `{success, ..}` envelope:
//!
//! - `POST /api/passkeys/login/{start,finish}`: usernameless sign-in, opens a session (and
//!   ends the one this browser still held).
//! - `POST /api/passkeys/register/{start,finish}`: a first passkey from an invitation (opens
//!   a session; a recovery ends the person's other sessions), or one more for the person
//!   signed in (a passkey sign-in in the last minutes, `REAUTH_MS`).
//! - `GET /api/passkeys`, `PATCH`/`DELETE /api/passkeys/{id}`: my passkeys; never the last;
//!   renaming and removing want a recent passkey sign-in.
//! - `POST /api/invites/greet` `{token}`: who a link is for (the token never in an address:
//!   the link carries it in its fragment, `/invite#<token>`). `GET`/`POST /api/invites`,
//!   `DELETE /api/invites/{id}`: admins (an admin's or a recovery invitation wants a recent
//!   passkey sign-in).
//! - `GET /api/people`, `DELETE /api/people/{id}` (recent passkey sign-in), `POST
//!   /api/people/{id}/sessions/end`: admins.

use std::{net::SocketAddr, sync::Arc};

use axum::{
    extract::{ConnectInfo, Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use webauthn_rs::prelude::{
    CreationChallengeResponse, CredentialID, DiscoverableKey, Passkey, PublicKeyCredential,
    RegisterPublicKeyCredential, RequestChallengeResponse, Uuid,
};

use crate::{
    auth::{recent_passkey, AdminUser, AuthUser, AuthenticatedUser},
    error::{AppError, Refusal},
    passkey::{client_ip, device_label, invite_lifetime_ms, Ceremony, PasskeyState, Via},
    people::{
        slug, Invite, PasskeyInfo, People, PersonInfo, Removed, Role, StoredInvite, StoredPasskey,
    },
    routes::{
        auth::{end_previous, end_sessions, open_session, opened_by},
        Answer, Reply, SimpleResponse,
    },
    util::{hash_secret, now_ms, random_id, random_secret},
    AppState,
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
        .route("/invites/greet", post(greet))
        .route("/invites/{id}", delete(delete_invite))
        .route("/people", get(list_people))
        .route("/people/{id}", delete(remove_person))
        .route("/people/{id}/sessions/end", post(end_person_sessions))
}

fn passkeys(state: &AppState) -> Result<&Arc<PasskeyState>, AppError> {
    state
        .passkeys
        .as_ref()
        .ok_or_else(|| Refusal::PasskeyOff.into())
}

// ---------- credentials as stored ----------

/// A credential id as text: base64url, as webauthn-rs writes it.
fn credential_text(id: &CredentialID) -> Result<String, AppError> {
    match serde_json::to_value(id)? {
        serde_json::Value::String(s) => Ok(s),
        _ => Err(AppError::http(
            StatusCode::INTERNAL_SERVER_ERROR,
            "credential id",
        )),
    }
}

fn credential_id(text: &str) -> Option<CredentialID> {
    serde_json::from_value(serde_json::Value::String(text.into())).ok()
}

/// What a stored credential says of its backup: webauthn-rs keeps it inside its credential,
/// public only behind its « danger » feature, so it is read here, typed, and a change of that
/// shape fails loudly (the log, and `the_backup_state_is_where_webauthn_rs_keeps_it`).
#[derive(Deserialize)]
struct BackedUp {
    cred: BackupState,
}

#[derive(Deserialize)]
struct BackupState {
    backup_state: bool,
}

/// The credential as JSON, and whether a keychain backs it up (what the list shows).
fn stored(pk: &Passkey) -> Result<(serde_json::Value, bool), AppError> {
    let json = serde_json::to_value(pk)?;
    let backed_up = match BackedUp::deserialize(&json) {
        Ok(found) => found.cred.backup_state,
        Err(error) => {
            tracing::error!(%error, "a passkey's backup state is no longer where webauthn-rs kept it");
            false
        }
    };
    Ok((json, backed_up))
}

/// A name the person gave, when given (blank: none).
fn optional_name(name: Option<&str>) -> Result<Option<String>, AppError> {
    match name.map(str::trim) {
        None | Some("") => Ok(None),
        Some(n) => crate::util::name(n).map(|n| Some(n.to_string())),
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
///
/// The link carries its token in its fragment (`/invite#<token>`): a fragment never leaves the
/// browser, so the token reaches no server log, no proxy, no `Referer`; the page sends it in
/// a request body (`POST /api/invites/greet`, `register/start`). An admin's link lasts
/// `ADMIN_INVITE_HOURS`, a member's `INVITE_DAYS` (`passkey::invite_lifetime_ms`).
pub async fn new_invite(
    people: &People,
    public_url: &str,
    person: &str,
    name: &str,
    admin: bool,
    created_by: Option<&str>,
) -> Result<CreatedInvite, AppError> {
    let name = crate::util::name(name)?;
    if person.is_empty() || slug(person) != person {
        return Err(Refusal::BadPerson.into());
    }
    let token = random_secret();
    let id = random_id();
    let now = now_ms();
    let expires_ms = now + invite_lifetime_ms(admin);
    people
        .create_invite(StoredInvite {
            id: id.clone(),
            token_hash: hash_secret(&token),
            person: person.to_string(),
            name: name.to_string(),
            role: if admin { Role::Admin } else { Role::Member },
            created_by: created_by.map(str::to_string),
            created_ms: now,
            expires_ms,
            used_ms: None,
        })
        .await?;
    Ok(CreatedInvite {
        id,
        url: invite_url(public_url, &token),
        expires_ms,
    })
}

/// The link of an invitation: its token in the fragment (`public_url` has no trailing slash:
/// `Config::load` trims it).
pub fn invite_url(public_url: &str, token: &str) -> String {
    format!("{public_url}/invite#{token}")
}

/// A pending invitation by its token.
async fn invite_by_token(state: &AppState, token: &str) -> Result<StoredInvite, AppError> {
    state
        .people
        .invite_by_hash(&hash_secret(token), now_ms())
        .await?
        .ok_or_else(|| Refusal::InviteInvalid.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
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
/// account by accident (« Léonard » the guest is not Léonard). An admin's invitation, or a
/// recovery (it adds a passkey to an existing account), wants a recent passkey sign-in: a
/// stolen session cookie alone must not plant a passkey nor make an admin.
async fn create_invite(
    State(state): State<AppState>,
    admin: AdminUser,
    Json(req): Json<InviteRequest>,
) -> Result<Json<Answer<CreatedInvite>>, AppError> {
    let p = passkeys(&state)?;
    if req.admin || req.person.is_some() {
        recent_passkey(&admin.0)?;
    }
    let (person, recovery) = match req.person {
        Some(person) => (person, true),
        None => {
            let person = slug(&req.name);
            if let Some(existing) = state.people.person(&person).await? {
                // who has it: the admin may mean them (their access back) or someone else
                let who =
                    serde_json::json!({ "person": { "id": existing.id, "name": existing.name } });
                return Err(AppError::from(Refusal::PersonExists).with_detail(who));
            }
            (person, false)
        }
    };
    let created = new_invite(
        &state.people,
        &p.public_url,
        &person,
        &req.name,
        req.admin,
        Some(&admin.0.id),
    )
    .await?;
    if recovery {
        tracing::warn!(by = %admin.0.id, %person, "invitation to recover an account");
    } else {
        tracing::info!(by = %admin.0.id, %person, admin = req.admin, "invitation made");
    }
    Ok(Answer::ok(created))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PeopleList {
    people: Vec<PersonInfo>,
}

/// Everyone, for the admins: `{success, people}`.
async fn list_people(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<Json<Answer<PeopleList>>, AppError> {
    Ok(Answer::ok(PeopleList {
        people: state.people.people().await?,
    }))
}

/// Someone goes: their passkeys, invitations and sessions with them. Never oneself (the
/// last admin cannot lock the house: `not_yourself`); a recent passkey sign-in first.
async fn remove_person(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    if id == admin.0.id {
        return Err(Refusal::NotYourself.into());
    }
    recent_passkey(&admin.0)?;
    end_sessions(&state, &id).await?;
    if !state.people.remove_person(&id).await? {
        return Err(Refusal::NotFound.into());
    }
    tracing::warn!(by = %admin.0.id, person = %id, "person removed");
    Ok(StatusCode::NO_CONTENT)
}

/// An admin ends someone's sessions (a lost phone they cannot reach themselves): every
/// sign-in of theirs, access tokens included. Their passkeys stay.
async fn end_person_sessions(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(id): Path<String>,
) -> Result<Json<SimpleResponse>, AppError> {
    if state.people.person(&id).await?.is_none() {
        return Err(Refusal::NotFound.into());
    }
    end_sessions(&state, &id).await?;
    tracing::warn!(by = %admin.0.id, person = %id, "an admin ended someone's sessions");
    Ok(SimpleResponse::ok("Sessions ended"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Invites {
    invites: Vec<Invite>,
}

/// The pending invitations: `{success, invites}`.
async fn list_invites(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<Json<Answer<Invites>>, AppError> {
    passkeys(&state)?;
    Ok(Answer::ok(Invites {
        invites: state.people.invites(now_ms()).await?,
    }))
}

async fn delete_invite(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    passkeys(&state)?;
    if state.people.delete_invite(&id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(Refusal::NotFound.into())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GreetRequest {
    /// the invitation's token, from the link's fragment
    token: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Greeting {
    name: String,
}

/// Who the link is for, to greet them: `{success, name}`.
async fn greet(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<GreetRequest>,
) -> Result<Json<Answer<Greeting>>, AppError> {
    let p = passkeys(&state)?;
    let invite = p
        .guarded(
            client_ip(addr, &headers),
            invite_by_token(&state, &req.token),
        )
        .await?;
    Ok(Answer::ok(Greeting { name: invite.name }))
}

// ---------- registration ----------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegisterStart {
    /// the invitation's token; absent to add a passkey while signed in
    invite: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
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
) -> Result<Json<Answer<CreationOptions>>, AppError> {
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
            (
                invite.person,
                display,
                handle,
                Via::Invite { id: invite.id },
            )
        }
        None => {
            let user = user.ok_or(Refusal::NotSignedIn)?.0;
            // a stolen session cookie alone must not be enough to plant a passkey
            recent_passkey(&user)?;
            let me = state
                .people
                .person(&user.id)
                .await?
                .ok_or(Refusal::NotSignedIn)?;
            (me.id, me.name, me.user_handle, Via::Session)
        }
    };
    let handle = Uuid::parse_str(&handle)
        .map_err(|_| AppError::http(StatusCode::INTERNAL_SERVER_ERROR, "user handle"))?;
    let exclude: Vec<CredentialID> = state
        .people
        .credential_ids_of(&person)
        .await?
        .iter()
        .filter_map(|c| credential_id(c))
        .collect();
    let (mut options, reg) = p
        .webauthn
        .start_passkey_registration(handle, &person, &display, Some(exclude))
        .map_err(|e| {
            tracing::error!(error = %e, "passkey registration could not start");
            AppError::http(
                StatusCode::INTERNAL_SERVER_ERROR,
                "passkey registration could not start",
            )
        })?;
    // a discoverable credential, so that sign-in needs no name (webauthn-rs asks for
    // "discouraged"; its registration check does not depend on it)
    if let Some(sel) = options.public_key.authenticator_selection.as_mut() {
        sel.resident_key = Some(webauthn_rs_proto::ResidentKeyRequirement::Required);
        sel.require_resident_key = true;
    }
    let ceremony = p.ceremonies.put(
        ip,
        Ceremony::Register {
            state: reg,
            person,
            handle,
            via,
        },
    );
    Ok(Answer::ok(CreationOptions { ceremony, options }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegisterFinish {
    ceremony: String,
    /// `PublicKeyCredential.toJSON()` of the new credential
    credential: RegisterPublicKeyCredential,
    /// what the person calls it; from the device when absent
    name: Option<String>,
}

/// `{success, passkey, user}`: the new passkey, and who the session is.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Registered {
    passkey: PasskeyInfo,
    user: AuthUser,
}

async fn register_finish(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    user: Option<AuthenticatedUser>,
    Json(req): Json<RegisterFinish>,
) -> Result<Response, AppError> {
    let p = passkeys(&state)?;
    p.guarded(
        client_ip(addr, &headers),
        finish_registration(&state, p, addr, &headers, user, req),
    )
    .await
}

async fn finish_registration(
    state: &AppState,
    p: &PasskeyState,
    addr: SocketAddr,
    headers: &HeaderMap,
    user: Option<AuthenticatedUser>,
    req: RegisterFinish,
) -> Result<Response, AppError> {
    let Some(Ceremony::Register {
        state: reg,
        person,
        handle,
        via,
    }) = p.ceremonies.take(&req.ceremony)
    else {
        return Err(Refusal::CeremonyExpired.into());
    };
    if matches!(via, Via::Session) && user.as_ref().is_none_or(|u| u.0.id != person) {
        return Err(Refusal::Forbidden.into());
    }
    let pk = p
        .webauthn
        .finish_passkey_registration(&req.credential, &reg)
        .map_err(|e| {
            tracing::warn!(error = %e, %person, "passkey registration refused");
            AppError::from(Refusal::PasskeyRejected)
        })?;
    let name = optional_name(req.name.as_deref())?.unwrap_or_else(|| {
        device_label(
            headers
                .get(header::USER_AGENT)
                .and_then(|v| v.to_str().ok())
                .unwrap_or(""),
        )
        .to_string()
    });
    let (json, backed_up) = stored(&pk)?;
    let key = StoredPasskey {
        id: random_id(),
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
            let found = state
                .people
                .passkeys_of(&person)
                .await?
                .into_iter()
                .find(|k| k.id == key_id);
            found.ok_or_else(|| AppError::from(Refusal::NotFound))
        }
    };
    match via {
        Via::Invite { id } => {
            let recovery = state.people.person(&person).await?.is_some();
            let redeemed = state
                .people
                .redeem_invite(&id, &handle.to_string(), key, now_ms())
                .await?;
            let person = redeemed
                .ok_or(Refusal::InviteInvalid)?
                .map_err(|_| Refusal::PasskeyExists)?;
            if recovery {
                // the lost phone's sessions (a thief's included) end with the way back in
                end_sessions(state, &person.id).await?;
                tracing::warn!(person = %person.id, "account recovered: a new passkey, every other session ended");
            } else {
                tracing::info!(person = %person.id, "invitation redeemed: passkey registered");
            }
            end_previous(state, headers).await?;
            let (cookies, Json(opened)) = open_session(
                state,
                &person,
                now_ms(),
                Err(opened_by(&person.id, addr, headers)),
            )
            .await?;
            let user = opened.body.user;
            Ok((
                cookies,
                Answer::ok(Registered {
                    passkey: info(&person.id).await?,
                    user,
                }),
            )
                .into_response())
        }
        Via::Session => {
            state
                .people
                .add_passkey(key)
                .await?
                .map_err(|_| Refusal::PasskeyExists)?;
            tracing::warn!(%person, "passkey added");
            let user = user.ok_or(Refusal::NotSignedIn)?.0;
            Ok(Answer::ok(Registered {
                passkey: info(&person).await?,
                user,
            })
            .into_response())
        }
    }
}

// ---------- sign-in ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
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
) -> Result<Json<Answer<RequestOptions>>, AppError> {
    let p = passkeys(&state)?;
    let ip = client_ip(addr, &headers);
    p.start_allowed(ip)?;
    let (mut options, auth) = p
        .webauthn
        .start_discoverable_authentication()
        .map_err(|e| {
            tracing::error!(error = %e, "passkey sign-in could not start");
            AppError::http(
                StatusCode::INTERNAL_SERVER_ERROR,
                "passkey sign-in could not start",
            )
        })?;
    options.mediation = None;
    let ceremony = p.ceremonies.put(ip, Ceremony::Login { state: auth });
    Ok(Answer::ok(RequestOptions { ceremony, options }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
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
    p.guarded(
        client_ip(addr, &headers),
        finish_login(&state, p, addr, &headers, req),
    )
    .await
}

async fn finish_login(
    state: &AppState,
    p: &PasskeyState,
    addr: SocketAddr,
    headers: &HeaderMap,
    req: LoginFinish,
) -> Result<Response, AppError> {
    let Some(Ceremony::Login { state: auth }) = p.ceremonies.take(&req.ceremony) else {
        return Err(Refusal::CeremonyExpired.into());
    };
    let (handle, raw_id) = p
        .webauthn
        .identify_discoverable_authentication(&req.credential)
        .map_err(|_| Refusal::PasskeyRejected)?;
    let cred_id = credential_text(&CredentialID::from(raw_id.to_vec()))?;
    // unknown here: the app may tell the device to forget it (signalUnknownCredential)
    let Some((row, person)) = state.people.passkey_by_credential(&cred_id).await? else {
        tracing::warn!(ip = %client_ip(addr, headers), "passkey sign-in refused: an unknown passkey");
        return Err(Refusal::UnknownPasskey.into());
    };
    if person.user_handle != handle.to_string() {
        tracing::warn!(person = %person.id, "passkey sign-in refused: not this person's");
        return Err(Refusal::PasskeyRejected.into());
    }
    let mut pk: Passkey = serde_json::from_value(row.passkey)?;
    let result = p
        .webauthn
        .finish_discoverable_authentication(&req.credential, auth, &[DiscoverableKey::from(&pk)])
        .map_err(|e| {
            tracing::warn!(error = %e, person = %person.id, "passkey sign-in refused");
            AppError::from(Refusal::PasskeyRejected)
        })?;
    if !result.user_verified() {
        tracing::warn!(person = %person.id, "passkey sign-in refused: no user verification");
        return Err(Refusal::PasskeyRejected.into());
    }
    let update = if pk.update_credential(&result) == Some(true) {
        // the authenticator's own word on its backup, from this very assertion
        let (json, _) = stored(&pk)?;
        Some((json, result.backup_state()))
    } else {
        None
    };
    state.people.passkey_used(&row.id, update, now_ms()).await?;
    tracing::info!(person = %person.id, ip = %client_ip(addr, headers), "signed in with a passkey");
    end_previous(state, headers).await?;
    Ok(open_session(
        state,
        &person,
        now_ms(),
        Err(opened_by(&person.id, addr, headers)),
    )
    .await?
    .into_response())
}

// ---------- my passkeys ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Passkeys {
    passkeys: Vec<PasskeyInfo>,
}

/// My passkeys: `{success, passkeys}`.
async fn list(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<Answer<Passkeys>>, AppError> {
    passkeys(&state)?;
    Ok(Answer::ok(Passkeys {
        passkeys: state.people.passkeys_of(&user.0.id).await?,
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Rename {
    name: String,
}

/// `{success, ..passkey}`, after a recent passkey sign-in.
async fn rename(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(id): Path<String>,
    Json(req): Json<Rename>,
) -> Result<Json<Answer<PasskeyInfo>>, AppError> {
    passkeys(&state)?;
    recent_passkey(&user.0)?;
    let name = crate::util::name(&req.name)?;
    state
        .people
        .rename_passkey(&user.0.id, &id, name)
        .await?
        .map(Answer::ok)
        .ok_or_else(|| Refusal::NotFound.into())
}

/// Never the last one, and after a recent passkey sign-in. Every session it may have opened
/// ends; this browser gets a fresh one.
async fn remove(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    user: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    passkeys(&state)?;
    let user = user.0;
    recent_passkey(&user)?;
    match state.people.delete_passkey(&user.id, &id).await? {
        Removed::Yes => {
            end_sessions(&state, &user.id).await?;
            let person = state
                .people
                .person(&user.id)
                .await?
                .ok_or(Refusal::NotSignedIn)?;
            let (cookies, _) = open_session(
                &state,
                &person,
                user.auth_ms,
                Err(opened_by(&person.id, addr, &headers)),
            )
            .await?;
            tracing::warn!(person = %user.id, "passkey removed");
            Ok((StatusCode::NO_CONTENT, cookies).into_response())
        }
        Removed::NotFound => Err(Refusal::NotFound.into()),
        Removed::Last => Err(Refusal::LastPasskey.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_invitation_link_carries_its_token_in_the_fragment() {
        assert_eq!(
            invite_url("https://home.example.com", "tok"),
            "https://home.example.com/invite#tok"
        );
    }

    /// webauthn-rs keeps a credential's backup state at `cred.backup_state`: an upgrade that
    /// moves it fails here, not silently in « Mon compte ».
    #[test]
    fn the_backup_state_is_where_webauthn_rs_keeps_it() {
        let json = serde_json::json!({ "cred": { "backup_state": true, "other": 1 } });
        assert!(BackedUp::deserialize(&json).unwrap().cred.backup_state);
        assert!(BackedUp::deserialize(&serde_json::json!({ "cred": {} })).is_err());
    }
}
