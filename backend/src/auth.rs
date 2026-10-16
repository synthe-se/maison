//! Who is asking, once signed in (signing in is `routes::passkeys`, the session's own routes
//! `routes::auth`).
//!
//! - The access token (a JWT in an HttpOnly cookie, 15 min) says only who, when, and from which
//!   sign-in (`sid`, its refresh family): the person's name and role are read from `people.rs`
//!   on every request, so a removed person or a changed role takes effect at once.
//!   « Sign out everywhere » (the person's `not_before_ms`) ends every access token; ending one
//!   sign-in (signing out, a session ended from the list) ends that sign-in's access tokens
//!   too ([`RefreshTokenStore::ended`]): a copied token dies with it.
//! - The refresh token (7 days, rotating, never past `SESSION_MAX_DAYS` after the passkey
//!   sign-in) is kept hashed; a spent one coming back means it was copied: its whole family
//!   (every token rotated from the same sign-in) ends. Each family keeps when, from where and
//!   from what it was opened: the person's list of sessions.
//! - [`DeviceClient`]: who drives the devices, the route layer of every device router: someone
//!   signed in, an app's service token as a bearer header (`maison_…`, made by
//!   `maison-backend token create`), or a paired wall screen's token in its HttpOnly cookie
//!   (`screens.rs`). Each kind of token comes only by its own carrier: an app's never in a
//!   cookie, a screen's never as a header. A service token is refused everywhere else
//!   (`AuthenticatedUser`, `AdminUser`): people, passkeys, invitations, tokens and admin
//!   settings stay with people. [`SettingsClient`] is a device client that is not a wall
//!   screen: what changes the house's settings (a name, a schedule) is not for a frame on the
//!   wall.
//! - [`MachineClient`]: the IR bridge's bearer token (`IR_API_TOKEN`), its failures limited per
//!   address.
//!
//! Every extractor notes who it found ([`Principal`]): the request layer logs every 401 and
//! 403 with it, the path and the client's address (`lib.rs`).

use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    sync::Arc,
};

use axum::{
    extract::{FromRef, FromRequestParts},
    http::{header::AUTHORIZATION, request::Parts, Extensions, HeaderMap},
};
use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{
    config::Config,
    error::{AppError, Refusal},
    passkey::client_ip,
    people::{People, Person, Role, Scope, StoredToken, TokenKind},
    store::{self, Access, Corrupt},
    util::{constant_time_eq, hash_secret, lock, now_ms, random_id, random_secret},
    AppState,
};

/// How recent the last passkey sign-in must be for what could hand the house to someone else:
/// adding, renaming or removing a passkey, an admin's or a recovery invitation, pairing a
/// screen, removing a person. A stolen session cookie alone is not enough for those.
pub const REAUTH_MS: i64 = 5 * 60 * 1000;

/// The access token's lifetime.
pub const ACCESS_TOKEN_MINUTES: i64 = 15;
/// The refresh token's lifetime (sliding: each refresh makes a new one).
pub const REFRESH_TOKEN_DAYS: i64 = 7;
/// The longest a session lives, refreshed or not, from its passkey sign-in: then a passkey
/// again. A month: a phone left signed in on the sofa, not forever.
pub const SESSION_MAX_DAYS: i64 = 30;
/// What jsonwebtoken's validation tolerates past `exp`.
const JWT_LEEWAY_S: i64 = 60;

/// Whether `user` signed in with a passkey lately (`REAUTH_MS`); else `reauth_needed`.
pub fn recent_passkey(user: &AuthUser) -> Result<(), AppError> {
    if now_ms() - user.auth_ms > REAUTH_MS {
        return Err(Refusal::ReauthNeeded.into());
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Claims {
    pub user_id: String,
    /// When this token was issued (ms): older than the person's `not_before_ms` is void.
    pub issued_ms: i64,
    /// When the person last proved it with a passkey (ms), carried across refreshes.
    pub auth_ms: i64,
    /// The sign-in it belongs to (its refresh family): ended, its access tokens are void.
    /// Empty in tokens issued before it existed.
    #[serde(default)]
    pub sid: String,
    pub exp: u64,
}

/// Who is signed in, as the store says now.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthUser {
    pub id: String,
    pub name: String,
    pub role: Role,
    /// When they last signed in with a passkey (ms).
    #[serde(skip)]
    pub auth_ms: i64,
    /// The sign-in this request comes from (its refresh family).
    #[serde(skip)]
    pub sid: String,
}

impl AuthUser {
    pub fn of(person: &Person, auth_ms: i64, sid: &str) -> Self {
        Self {
            id: person.id.clone(),
            name: person.name.clone(),
            role: person.role,
            auth_ms,
            sid: sid.to_string(),
        }
    }

    pub fn is_admin(&self) -> bool {
        self.role == Role::Admin
    }
}

// ---------- cookies ----------

/// The cookies Maison sets, by what they hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CookieName {
    /// The session's access token (`AUTH_COOKIE_NAME`, `maison_session`).
    Access,
    /// The session's refresh token.
    Refresh,
    /// A paired wall screen's token (`maison_api::screens::COOKIE`, read by the frame too).
    Screen,
}

/// A cookie's name, said once: `__Host-` prefixed when secure (bound to this exact host, path
/// `/`, so a sibling subdomain can neither read nor plant it).
pub fn cookie_name(config: &Config, which: CookieName) -> String {
    let base = match which {
        CookieName::Access => config.auth_cookie_name.as_str(),
        CookieName::Refresh => "maison_refresh",
        CookieName::Screen => maison_api::screens::COOKIE,
    };
    if config.auth_cookie_secure {
        format!("__Host-{base}")
    } else {
        base.to_string()
    }
}

/// A named cookie's value from the request headers.
pub fn cookie_value<'a>(headers: &'a HeaderMap, cookie_name: &str) -> Option<&'a str> {
    headers
        .get(axum::http::header::COOKIE)
        .and_then(|value| value.to_str().ok())?
        .split(';')
        .filter_map(|cookie| cookie.trim().split_once('='))
        .find_map(|(name, value)| (name == cookie_name).then_some(value))
}

// ---------- who asked ----------

/// Where a request's answer notes who asked, for the log line of a refusal (`lib.rs` puts one
/// in every request; the extractors fill it). Never a secret: an id, a token's name.
#[derive(Debug, Clone, Default)]
pub struct Principal(Arc<std::sync::Mutex<Option<String>>>);

impl Principal {
    /// Who the extractors found, if anyone.
    pub fn who(&self) -> Option<String> {
        lock(&self.0).clone()
    }
}

fn note(parts: &Parts, who: impl FnOnce() -> String) {
    if let Some(principal) = parts.extensions.get::<Principal>() {
        *lock(&principal.0) = Some(who());
    }
}

/// The peer's socket address: the server's (`ConnectInfo`), or the one a test mocks.
pub fn peer_addr(extensions: &Extensions) -> Option<SocketAddr> {
    use axum::extract::{connect_info::MockConnectInfo, ConnectInfo};
    extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0)
        .or_else(|| extensions.get::<MockConnectInfo<SocketAddr>>().map(|m| m.0))
}

/// The client's address (through the tunnel, the one cloudflared names: `passkey::client_ip`).
pub fn request_ip(extensions: &Extensions, headers: &HeaderMap) -> IpAddr {
    let peer = peer_addr(extensions).unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 0)));
    client_ip(peer, headers)
}

// ---------- people ----------

#[derive(Debug, Clone)]
pub struct AuthenticatedUser(pub AuthUser);

impl<S> FromRequestParts<S> for AuthenticatedUser
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let state = AppState::from_ref(state);
        let access = cookie_name(&state.config, CookieName::Access);
        let bearer = bearer_token(&parts.headers);
        if bearer.is_some_and(is_service_token) {
            // a machine where only a person may be
            note(parts, || "a service token".into());
            return Err(Refusal::Forbidden.into());
        }
        let token = bearer
            .or_else(|| cookie_value(&parts.headers, &access))
            .ok_or(Refusal::NotSignedIn)?;
        let claims = decode_token(token, state.config.jwt_secret.as_bytes())
            .map_err(|_| Refusal::NotSignedIn)?
            .claims;
        note(parts, || format!("person {}", claims.user_id));
        let person = state
            .people
            .person(&claims.user_id)
            .await?
            .ok_or(Refusal::NotSignedIn)?;
        if claims.issued_ms < person.not_before_ms || state.refresh_store.ended(&claims.sid).await {
            return Err(Refusal::NotSignedIn.into());
        }
        Ok(Self(AuthUser::of(&person, claims.auth_ms, &claims.sid)))
    }
}

/// Signed in or not: the routes that serve both (adding a passkey, or a first one by invitation).
impl<S> axum::extract::OptionalFromRequestParts<S> for AuthenticatedUser
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> Result<Option<Self>, Self::Rejection> {
        Ok(
            <Self as FromRequestParts<S>>::from_request_parts(parts, state)
                .await
                .ok(),
        )
    }
}

/// Someone signed in with the `admin` role: inviting, configuring, installing. Everyday
/// control needs only the device layer (`DeviceClient`, the route layer in `lib.rs`).
#[derive(Debug, Clone)]
pub struct AdminUser(pub AuthUser);

impl<S> FromRequestParts<S> for AdminUser
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let user = AuthenticatedUser::from_request_parts(parts, state).await?.0;
        if !user.is_admin() {
            return Err(Refusal::Forbidden.into());
        }
        Ok(Self(user))
    }
}

// ---------- devices ----------

/// Who drives the devices: an app's service token (`devices` scope, sent as a bearer header),
/// someone signed in, or a paired wall screen (its token in its HttpOnly cookie), in that
/// order: someone signing in on a screen's browser is themselves. The device routers' layer
/// (`lib.rs`); a handler that needs more asks `AdminUser` (which a token never is) or
/// `SettingsClient` (which a screen never is).
#[derive(Debug, Clone)]
pub enum DeviceClient {
    Person(AuthUser),
    /// An app's service token, by its name.
    Service(String),
    /// A paired wall screen, by its name.
    Screen(String),
}

impl DeviceClient {
    pub fn is_admin(&self) -> bool {
        matches!(self, Self::Person(user) if user.is_admin())
    }

    /// Refused (`not_for_screens`) when this is a wall screen: settings are not for a frame
    /// anyone in the room can touch.
    pub fn not_a_screen(&self) -> Result<(), AppError> {
        match self {
            Self::Screen(_) => Err(Refusal::NotForScreens.into()),
            _ => Ok(()),
        }
    }
}

impl<S> FromRequestParts<S> for DeviceClient
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);
        if let Some(token) = bearer_token(&parts.headers).filter(|token| is_service_token(token)) {
            let found = token_by_carrier(&app_state, parts, token, TokenKind::App).await?;
            return Ok(Self::Service(found.name));
        }
        let refused = match AuthenticatedUser::from_request_parts(parts, state).await {
            Ok(user) => return Ok(Self::Person(user.0)),
            Err(refused) => refused,
        };
        let screen = cookie_name(&app_state.config, CookieName::Screen);
        match cookie_value(&parts.headers, &screen).filter(|token| is_service_token(token)) {
            Some(token) => {
                let token = token.to_string();
                let found = token_by_carrier(&app_state, parts, &token, TokenKind::Screen).await?;
                Ok(Self::Screen(found.name))
            }
            None => Err(refused),
        }
    }
}

/// A device client that may change the house's settings (a lamp's or a shutter's name, the sun
/// schedule, the rabbit's reboot, a Broadlink address of one's choosing): someone signed in or
/// an app's token, never a wall screen (`not_for_screens`). Taken by the handler, under the
/// device layer.
#[derive(Debug, Clone)]
pub struct SettingsClient(pub DeviceClient);

impl<S> FromRequestParts<S> for SettingsClient
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let client = DeviceClient::from_request_parts(parts, state).await?;
        client.not_a_screen()?;
        Ok(Self(client))
    }
}

/// The service token `token`, come by the carrier of `kind` (an app's as a bearer header, a
/// screen's in its cookie): any other token, or one by the wrong carrier, is not signed in.
/// Failures count against the address (`AppState::auth_failures`).
async fn token_by_carrier(
    state: &AppState,
    parts: &Parts,
    token: &str,
    kind: TokenKind,
) -> Result<StoredToken, AppError> {
    let ip = request_ip(&parts.extensions, &parts.headers);
    state.auth_failures.check(ip).map_err(AppError::too_many)?;
    let found = match service_token(&state.people, token, now_ms()).await? {
        Some(found) => found,
        None => {
            state.auth_failures.fail(ip);
            note(parts, || "an unknown service token".into());
            tracing::warn!(%ip, carrier = kind.as_str(), "unknown or revoked service token");
            return Err(Refusal::NotSignedIn.into());
        }
    };
    note(parts, || {
        format!("{} token {}", found.kind.as_str(), found.name)
    });
    if found.kind != kind {
        state.auth_failures.fail(ip);
        tracing::warn!(
            %ip,
            token = %found.name,
            kind = found.kind.as_str(),
            carrier = kind.as_str(),
            "a service token by the wrong carrier: refused"
        );
        return Err(Refusal::NotSignedIn.into());
    }
    Ok(found)
}

/// What every service token starts with: told apart from a session's JWT (`eyJ…`) at a glance,
/// by the request guard too.
pub const SERVICE_TOKEN_PREFIX: &str = "maison_";

/// How often a token's last use is written down, at most: an hour (the SD card counts writes).
pub const TOKEN_USE_STEP_MS: i64 = 60 * 60 * 1000;

pub fn is_service_token(token: &str) -> bool {
    token.starts_with(SERVICE_TOKEN_PREFIX)
}

/// Whether the request carries a service token as a bearer header (said, not checked: the
/// guard's question).
pub fn has_service_token(headers: &HeaderMap) -> bool {
    bearer_token(headers).is_some_and(is_service_token)
}

/// A new service token named `name`, for `scope`, carried as `kind` says: the token, to show
/// once (only its hash is kept); `None` when another token has the name. 256 random bits
/// (`util::random_secret`), prefixed.
pub async fn issue_service_token(
    people: &People,
    name: &str,
    scope: Scope,
    kind: TokenKind,
) -> Result<Option<String>, AppError> {
    let name = crate::util::name(name)?.to_string();
    let token = format!("{SERVICE_TOKEN_PREFIX}{}", random_secret());
    let stored = StoredToken {
        id: random_id(),
        name,
        scope,
        kind,
        token_hash: hash_secret(&token),
        created_ms: now_ms(),
        last_used_ms: None,
    };
    Ok(people.create_token(stored).await?.then_some(token))
}

/// The service token `token`, when it exists; its use is recorded (`TOKEN_USE_STEP_MS`).
/// `None`: unknown or revoked. (One scope exists, `devices`: every token carries it.)
pub(crate) async fn service_token(
    people: &People,
    token: &str,
    now_ms: i64,
) -> Result<Option<StoredToken>, AppError> {
    let Some(found) = people.token_by_hash(&hash_secret(token)).await? else {
        return Ok(None);
    };
    if found
        .last_used_ms
        .is_none_or(|at| now_ms - at >= TOKEN_USE_STEP_MS)
    {
        // a failed write costs only the date: the call goes on
        if let Err(error) = people.token_used(&found.id, now_ms).await {
            tracing::warn!(%error, token = %found.name, "service token use not recorded");
        }
    }
    Ok(Some(found))
}

// ---------- the IR bridge ----------

/// Extractor for machine-to-machine clients (the kird IR bridge on the STB): a raw bearer
/// token compared in constant time against `IR_API_TOKEN` (32 characters at least, or the
/// backend refuses to start). Fails closed when the token is not configured; failures count
/// against the address, which waits once over the limit. The request guard keeps the route to
/// the LAN (`lib.rs`). Deliberately not chained with the session: a machine route accepts the
/// machine token only.
#[derive(Debug, Clone)]
pub struct MachineClient;

impl<S> FromRequestParts<S> for MachineClient
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);
        let ip = request_ip(&parts.extensions, &parts.headers);
        app_state
            .auth_failures
            .check(ip)
            .map_err(AppError::too_many)?;
        let token = bearer_token(&parts.headers);
        // unset or wrong, the same answer: the LAN learns nothing of the setup
        let good = match (app_state.config.ir_api_token.as_deref(), token) {
            (Some(expected), Some(token)) => {
                constant_time_eq(token.as_bytes(), expected.as_bytes())
            }
            (None, _) => {
                tracing::warn!(%ip, "a machine call, but IR_API_TOKEN is not set: refused");
                false
            }
            (Some(_), None) => false,
        };
        if !good {
            app_state.auth_failures.fail(ip);
            note(parts, || "the IR bridge (bad token)".into());
            tracing::warn!(%ip, "IR bridge: wrong or missing machine token");
            return Err(Refusal::NotSignedIn.into());
        }
        note(parts, || "the IR bridge".into());
        Ok(Self)
    }
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer "))
}

/// The access token's claims, when its signature and dates hold (HS256 only, `exp` checked).
pub fn decode_token(
    token: &str,
    secret: &[u8],
) -> Result<jsonwebtoken::TokenData<Claims>, jsonwebtoken::errors::Error> {
    hs256::install();
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret),
        &Validation::default(),
    )
}

/// An access token for `claims`, signed with `secret` (HS256).
pub fn encode_token(claims: &Claims, secret: &[u8]) -> Result<String, jsonwebtoken::errors::Error> {
    hs256::install();
    encode(
        &Header::default(),
        claims,
        &EncodingKey::from_secret(secret),
    )
}

/// The one algorithm the access token uses, HS256, as jsonwebtoken's crypto provider: its own
/// providers (`rust_crypto`, `aws_lc_rs`) bring RSA, ECDSA and EdDSA, `rsa` 0.9 among them
/// (RUSTSEC-2023-0071), none of which Maison signs with. HMAC-SHA-256 (RFC 2104) over `sha2`,
/// checked against RFC 4231's vectors; any other algorithm is refused.
mod hs256 {
    use jsonwebtoken::{
        crypto::{CryptoProvider, JwtSigner, JwtVerifier, KeyUtils},
        errors::{ErrorKind, Result},
        signature, Algorithm, AlgorithmFamily, DecodingKey, DecodingKeyKind, EncodingKey,
    };
    use sha2::{Digest, Sha256};

    use crate::util::constant_time_eq;

    const BLOCK: usize = 64;

    /// HMAC-SHA-256 of `message` under `key`.
    pub(super) fn hmac(key: &[u8], message: &[u8]) -> [u8; 32] {
        let mut block = [0u8; BLOCK];
        if key.len() > BLOCK {
            block[..32].copy_from_slice(&Sha256::digest(key));
        } else {
            block[..key.len()].copy_from_slice(key);
        }
        let pad = |byte: u8| block.map(|b| b ^ byte);
        let inner = Sha256::new()
            .chain_update(pad(0x36))
            .chain_update(message)
            .finalize();
        Sha256::new()
            .chain_update(pad(0x5c))
            .chain_update(inner)
            .finalize()
            .into()
    }

    struct Signer(Vec<u8>);

    impl signature::Signer<Vec<u8>> for Signer {
        fn try_sign(&self, message: &[u8]) -> std::result::Result<Vec<u8>, signature::Error> {
            Ok(hmac(&self.0, message).to_vec())
        }
    }

    impl JwtSigner for Signer {
        fn algorithm(&self) -> Algorithm {
            Algorithm::HS256
        }
    }

    struct Verifier(Vec<u8>);

    impl signature::Verifier<Vec<u8>> for Verifier {
        fn verify(
            &self,
            message: &[u8],
            tag: &Vec<u8>,
        ) -> std::result::Result<(), signature::Error> {
            if constant_time_eq(&hmac(&self.0, message), tag) {
                Ok(())
            } else {
                Err(signature::Error::new())
            }
        }
    }

    impl JwtVerifier for Verifier {
        fn algorithm(&self) -> Algorithm {
            Algorithm::HS256
        }
    }

    fn signer(algorithm: &Algorithm, key: &EncodingKey) -> Result<Box<dyn JwtSigner>> {
        if *algorithm != Algorithm::HS256 || key.family() != AlgorithmFamily::Hmac {
            return Err(ErrorKind::InvalidAlgorithm.into());
        }
        Ok(Box::new(Signer(key.as_bytes().to_vec())))
    }

    fn verifier(algorithm: &Algorithm, key: &DecodingKey) -> Result<Box<dyn JwtVerifier>> {
        match key.kind() {
            DecodingKeyKind::SecretOrDer(secret)
                if *algorithm == Algorithm::HS256 && key.family() == AlgorithmFamily::Hmac =>
            {
                Ok(Box::new(Verifier(secret.clone())))
            }
            _ => Err(ErrorKind::InvalidAlgorithm.into()),
        }
    }

    static PROVIDER: CryptoProvider = CryptoProvider {
        signer_factory: signer,
        verifier_factory: verifier,
        key_utils: KeyUtils::new_unimplemented(),
    };

    /// This process signs and checks its tokens with [`PROVIDER`] (once; again is harmless).
    pub(super) fn install() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            if PROVIDER.install_default().is_err() {
                tracing::error!("another JWT crypto provider was installed first");
            }
        });
    }
}

// ---------- refresh tokens and sessions ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshEntry {
    /// The person's id; their name and role are read again at each refresh.
    pub user_id: String,
    /// Seconds since epoch when this refresh token expires.
    pub expires_at: i64,
    /// Every token rotated from one sign-in shares it: a replayed one ends them all.
    pub family: String,
    /// When the person last signed in with a passkey (ms).
    pub auth_ms: i64,
}

/// A spent token, remembered until it would have expired: seeing it again is a replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spent {
    family: String,
    expires_at: i64,
}

/// Where and when a sign-in (a refresh family) was opened, and last refreshed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Opened {
    pub user_id: String,
    pub created_ms: i64,
    pub last_seen_ms: i64,
    /// The client's address at sign-in.
    pub ip: String,
    /// Its browser's user agent at sign-in, cut to `USER_AGENT_MAX` characters.
    pub user_agent: String,
}

const USER_AGENT_MAX: usize = 200;

impl Opened {
    /// A sign-in by `user_id` now, from `ip` with `user_agent`.
    pub fn now(user_id: &str, ip: IpAddr, user_agent: &str) -> Self {
        let now = now_ms();
        Self {
            user_id: user_id.to_string(),
            created_ms: now,
            last_seen_ms: now,
            ip: ip.to_string(),
            user_agent: user_agent
                .chars()
                .filter(|c| !c.is_control())
                .take(USER_AGENT_MAX)
                .collect(),
        }
    }
}

/// A sign-in as its person's list shows it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    /// What names it in `DELETE /api/auth/sessions/{id}` (not the family itself).
    pub id: String,
    pub created_ms: i64,
    pub last_seen_ms: i64,
    pub ip: String,
    pub user_agent: String,
    /// The device, from its user agent (`iPhone`, `Mac`…); empty when unknown.
    pub device: String,
    /// The session this request comes from.
    pub current: bool,
}

/// A family's public id: what the session list says, never the family itself.
fn public_id(family: &str) -> String {
    hash_secret(family)[..16].to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Tokens {
    /// By `hash_secret(token)`: the file holds no usable token.
    #[serde(default)]
    live: HashMap<String, RefreshEntry>,
    #[serde(default)]
    spent: HashMap<String, Spent>,
    /// The sign-ins, by family.
    #[serde(default)]
    families: HashMap<String, Opened>,
    /// Sign-ins ended while their access tokens may still run: family → until when (s).
    #[serde(default)]
    ended: HashMap<String, i64>,
}

impl Tokens {
    /// The family ends now: its live tokens go, its access tokens are void until they expire.
    fn end_family(&mut self, family: &str, now_s: i64) {
        self.live.retain(|_, e| e.family != family);
        self.families.remove(family);
        self.ended.insert(
            family.to_string(),
            now_s + ACCESS_TOKEN_MINUTES * 60 + JWT_LEEWAY_S,
        );
    }
}

/// What a refresh token turned out to be.
#[derive(Debug)]
pub enum Taken {
    /// Good, and now spent: rotate.
    Valid(RefreshEntry),
    /// Spent already: someone else has a copy. Its family has been ended.
    Replayed {
        user_id: Option<String>,
    },
    Unknown,
}

/// Refresh tokens and sign-ins, persisted (0600) so a restart or a deploy keeps everyone signed
/// in. Every change is written before it counts: when the file cannot be written the change
/// is refused (an error, the session not refreshed), never kept in memory only, where a
/// restart would bring spent and revoked tokens back.
#[derive(Clone, Default)]
pub struct RefreshTokenStore {
    inner: Arc<Mutex<Tokens>>,
    path: Option<Arc<PathBuf>>,
}

impl RefreshTokenStore {
    /// From disk; a torn file only signs everyone out (kept aside, logged).
    pub fn load(path: &Path) -> Self {
        let mut tokens: Tokens = store::read_json(path, Corrupt::Reset).unwrap_or_else(|error| {
            tracing::warn!(%error, "refresh tokens unreadable: everyone signs in again");
            Tokens::default()
        });
        prune(&mut tokens, Utc::now().timestamp());
        Self {
            inner: Arc::new(Mutex::new(tokens)),
            path: Some(Arc::new(path.to_path_buf())),
        }
    }

    /// Changes the tokens through `f` (which says whether it changed anything): written first,
    /// then kept; a failed write leaves them as they were and is the caller's error.
    async fn edit<T>(&self, f: impl FnOnce(&mut Tokens, i64) -> (T, bool)) -> Result<T, AppError> {
        let now = Utc::now().timestamp();
        let mut tokens = self.inner.lock().await;
        let mut next = tokens.clone();
        let pruned = prune(&mut next, now);
        let (out, changed) = f(&mut next, now);
        if changed || pruned {
            if let Some(path) = self.path.as_deref() {
                store::write_json_async(path, &next, Access::Private)
                    .await
                    .inspect_err(|error| {
                        tracing::error!(%error, "refresh tokens not written: the change is refused");
                    })?;
            }
            *tokens = next;
        }
        Ok(out)
    }

    /// A new refresh token; `opened` when it starts a sign-in (a new family), else the
    /// family's last refresh is now.
    pub async fn insert(
        &self,
        token: &str,
        entry: RefreshEntry,
        opened: Option<Opened>,
    ) -> Result<(), AppError> {
        let key = hash_secret(token);
        self.edit(move |tokens, _| {
            match opened {
                Some(opened) => {
                    tokens.families.insert(entry.family.clone(), opened);
                }
                None => {
                    if let Some(family) = tokens.families.get_mut(&entry.family) {
                        family.last_seen_ms = now_ms();
                    }
                }
            }
            tokens.live.insert(key, entry);
            ((), true)
        })
        .await
    }

    /// The token, spent in the same breath (two refreshes racing with it: one wins).
    pub async fn take(&self, token: &str) -> Result<Taken, AppError> {
        let key = hash_secret(token);
        self.edit(move |tokens, now| {
            if let Some(entry) = tokens.live.remove(&key) {
                tokens.spent.insert(
                    key,
                    Spent {
                        family: entry.family.clone(),
                        expires_at: entry.expires_at,
                    },
                );
                (Taken::Valid(entry), true)
            } else if let Some(spent) = tokens.spent.get(&key).cloned() {
                let user_id = tokens
                    .families
                    .get(&spent.family)
                    .map(|f| f.user_id.clone())
                    .or_else(|| {
                        tokens
                            .live
                            .values()
                            .find(|e| e.family == spent.family)
                            .map(|e| e.user_id.clone())
                    });
                tokens.end_family(&spent.family, now);
                (Taken::Replayed { user_id }, true)
            } else {
                // a garbage cookie costs no write (nor an fsync)
                (Taken::Unknown, false)
            }
        })
        .await
    }

    /// Every sign-in of a person: how many went.
    pub async fn remove_person(&self, user_id: &str) -> Result<usize, AppError> {
        self.edit(|tokens, now| {
            let families: Vec<String> = tokens
                .families
                .iter()
                .filter(|(_, f)| f.user_id == user_id)
                .map(|(k, _)| k.clone())
                .chain(
                    tokens
                        .live
                        .values()
                        .filter(|e| e.user_id == user_id)
                        .map(|e| e.family.clone()),
                )
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();
            for family in &families {
                tokens.end_family(family, now);
            }
            (families.len(), !families.is_empty())
        })
        .await
    }

    /// The sign-in this refresh token belongs to ends (signing out): the token, the others of
    /// its family and its access tokens. Nothing for an unknown token.
    pub async fn remove(&self, token: &str) -> Result<(), AppError> {
        let key = hash_secret(token);
        self.edit(move |tokens, now| match tokens.live.get(&key).cloned() {
            Some(entry) => {
                tokens.end_family(&entry.family, now);
                ((), true)
            }
            None => ((), false),
        })
        .await
    }

    /// The sign-in `family` ends, whoever's it is (signing out by an access token alone).
    pub async fn end(&self, family: &str) -> Result<(), AppError> {
        if family.is_empty() {
            return Ok(());
        }
        self.edit(|tokens, now| {
            tokens.end_family(family, now);
            ((), true)
        })
        .await
    }

    /// `user_id`'s sign-in listed as `id` ends. False when they have none such.
    pub async fn end_listed(&self, user_id: &str, id: &str) -> Result<bool, AppError> {
        self.edit(|tokens, now| {
            let found = tokens
                .families
                .iter()
                .find(|(family, f)| f.user_id == user_id && public_id(family) == id)
                .map(|(family, _)| family.clone());
            match found {
                Some(family) => {
                    tokens.end_family(&family, now);
                    (true, true)
                }
                None => (false, false),
            }
        })
        .await
    }

    /// The family of this refresh token, if it is live.
    pub async fn family_of(&self, token: &str) -> Option<String> {
        let key = hash_secret(token);
        self.inner
            .lock()
            .await
            .live
            .get(&key)
            .map(|e| e.family.clone())
    }

    /// Whether the sign-in `sid` was ended while its access tokens may still run.
    pub async fn ended(&self, sid: &str) -> bool {
        if sid.is_empty() {
            return false;
        }
        self.inner
            .lock()
            .await
            .ended
            .get(sid)
            .is_some_and(|until| *until > Utc::now().timestamp())
    }

    /// `user_id`'s open sign-ins, the latest first; `current` is the one of this request.
    pub async fn sessions(&self, user_id: &str, current: &str) -> Vec<SessionInfo> {
        let tokens = self.inner.lock().await;
        let mut list: Vec<SessionInfo> = tokens
            .families
            .iter()
            .filter(|(family, f)| {
                f.user_id == user_id && tokens.live.values().any(|e| &e.family == *family)
            })
            .map(|(family, f)| SessionInfo {
                id: public_id(family),
                created_ms: f.created_ms,
                last_seen_ms: f.last_seen_ms,
                ip: f.ip.clone(),
                user_agent: f.user_agent.clone(),
                device: crate::passkey::device_label(&f.user_agent).to_string(),
                current: family == current,
            })
            .collect();
        list.sort_by_key(|s| std::cmp::Reverse(s.last_seen_ms));
        list
    }
}

/// The expired go (and the sign-ins left with no live token); whether any did.
fn prune(tokens: &mut Tokens, now: i64) -> bool {
    let before =
        tokens.live.len() + tokens.spent.len() + tokens.families.len() + tokens.ended.len();
    tokens.live.retain(|_, e| e.expires_at > now);
    tokens.spent.retain(|_, s| s.expires_at > now);
    tokens.ended.retain(|_, until| *until > now);
    let live = &tokens.live;
    tokens
        .families
        .retain(|family, _| live.values().any(|e| &e.family == family));
    tokens.live.len() + tokens.spent.len() + tokens.families.len() + tokens.ended.len() != before
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_service_token_is_random_base64url_shown_once_and_its_use_dated_hourly() {
        let dir = crate::util::test_dir();
        let people = People::new(&dir.path().join("auth.json"));
        let token = issue_service_token(&people, " achi ", Scope::Devices, TokenKind::App)
            .await
            .unwrap()
            .unwrap();
        let secret = token.strip_prefix(SERVICE_TOKEN_PREFIX).expect("prefixed");
        assert_eq!(secret.len(), 43, "32 bytes, base64url without padding");
        assert!(secret
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
        assert!(
            issue_service_token(&people, "achi", Scope::Devices, TokenKind::App)
                .await
                .unwrap()
                .is_none(),
            "the name is taken"
        );
        assert!(
            issue_service_token(&people, " ", Scope::Devices, TokenKind::App)
                .await
                .is_err(),
            "a name"
        );
        assert!(is_service_token(&token) && !is_service_token("eyJhbGciOiJIUzI1NiJ9.x.y"));

        let found = |at: i64| {
            let (people, token) = (&people, token.clone());
            async move { service_token(people, &token, at).await.unwrap() }
        };
        assert_eq!(found(1_000).await.unwrap().name, "achi");
        let used = || async { people.tokens().await.unwrap()[0].last_used_ms };
        assert_eq!(used().await, Some(1_000));
        found(1_000 + TOKEN_USE_STEP_MS - 1).await.unwrap();
        assert_eq!(used().await, Some(1_000), "not rewritten within the hour");
        found(1_000 + TOKEN_USE_STEP_MS).await.unwrap();
        assert_eq!(used().await, Some(1_000 + TOKEN_USE_STEP_MS));

        let forged = format!("{SERVICE_TOKEN_PREFIX}{}", "A".repeat(43));
        assert!(service_token(&people, &forged, 0).await.unwrap().is_none());
        assert!(people.revoke_token("achi").await.unwrap());
        assert!(found(0).await.is_none(), "revoked at once");
    }

    /// RFC 4231, test cases 1, 2 and 6 (a key longer than a block).
    #[test]
    fn hmac_sha256_is_rfc_4231s() {
        let hex = |b: &[u8]| crate::util::hex(b, "");
        assert_eq!(
            hex(&hs256::hmac(&[0x0b; 20], b"Hi There")),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
        assert_eq!(
            hex(&hs256::hmac(b"Jefe", b"what do ya want for nothing?")),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        assert_eq!(
            hex(&hs256::hmac(
                &[0xaa; 131],
                b"Test Using Larger Than Block-Size Key - Hash Key First"
            )),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    #[test]
    fn an_access_token_round_trips_and_another_algorithm_or_key_is_refused() {
        let claims = Claims {
            user_id: "leonard".into(),
            issued_ms: 1,
            auth_ms: 1,
            sid: "f".into(),
            exp: 4_102_444_800,
        };
        let token = encode_token(&claims, b"k".repeat(32).as_slice()).unwrap();
        let back = decode_token(&token, b"k".repeat(32).as_slice()).unwrap();
        assert_eq!(
            (back.claims.user_id.as_str(), back.claims.sid.as_str()),
            ("leonard", "f")
        );
        assert!(decode_token(&token, b"other key, long enough, 32 bytes").is_err());
        // the same claims under `alg: none`, or HS512: refused
        let body = token.split('.').nth(1).unwrap();
        assert!(decode_token(&format!("eyJhbGciOiJub25lIn0.{body}."), b"k").is_err());
        assert!(decode_token(&format!("eyJhbGciOiJIUzUxMiJ9.{body}.x"), b"k").is_err());
    }

    #[test]
    fn one_cookie_name_builder_the_frames_screen_cookie_included() {
        let mut config = Config::defaults("/x".into());
        config.auth_cookie_secure = true;
        assert_eq!(
            cookie_name(&config, CookieName::Screen),
            maison_api::screens::COOKIE_SECURE
        );
        assert_eq!(
            cookie_name(&config, CookieName::Refresh),
            "__Host-maison_refresh"
        );
        assert_eq!(
            cookie_name(&config, CookieName::Access),
            "__Host-maison_session"
        );
        config.auth_cookie_secure = false;
        assert_eq!(
            cookie_name(&config, CookieName::Screen),
            maison_api::screens::COOKIE
        );
    }

    fn entry(family: &str) -> RefreshEntry {
        RefreshEntry {
            user_id: "leonard".into(),
            expires_at: Utc::now().timestamp() + 60,
            family: family.into(),
            auth_ms: 0,
        }
    }

    fn opened() -> Option<Opened> {
        Some(Opened::now(
            "leonard",
            "198.51.100.7".parse().unwrap(),
            "Mozilla/5.0 (iPhone; CPU iPhone OS 19_0 like Mac OS X)",
        ))
    }

    #[tokio::test]
    async fn a_token_is_spent_once_and_a_replay_ends_its_family() {
        let dir = crate::util::test_dir();
        let path = dir.path().join("t.json");
        let tokens = RefreshTokenStore::load(&path);
        tokens.insert("a", entry("f1"), opened()).await.unwrap();
        tokens.insert("other", entry("f2"), opened()).await.unwrap();
        assert!(matches!(tokens.take("a").await.unwrap(), Taken::Valid(_)));
        tokens.insert("b", entry("f1"), None).await.unwrap(); // rotated
        assert!(
            matches!(
                tokens.take("a").await.unwrap(),
                Taken::Replayed { user_id: Some(_) }
            ),
            "a copy of a spent token"
        );
        assert!(tokens.ended("f1").await, "its access tokens end too");
        assert!(
            matches!(
                tokens.take("b").await.unwrap(),
                Taken::Replayed { .. } | Taken::Unknown
            ),
            "its family ended"
        );
        assert!(
            matches!(tokens.take("other").await.unwrap(), Taken::Valid(_)),
            "another sign-in untouched"
        );
        assert!(matches!(tokens.take("nope").await.unwrap(), Taken::Unknown));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            !text.contains("\"other\"") && !text.contains("\"b\""),
            "only hashes on disk"
        );
        // reloaded from disk: a spent token is still known as spent
        let again = RefreshTokenStore::load(&path);
        assert!(matches!(
            again.take("other").await.unwrap(),
            Taken::Replayed { .. }
        ));
        assert!(again.ended("f1").await, "remembered across a restart");
    }

    #[tokio::test]
    async fn two_refreshes_racing_with_one_token_one_wins() {
        let tokens = RefreshTokenStore::default();
        tokens.insert("a", entry("f"), opened()).await.unwrap();
        let (x, y) = tokio::join!(tokens.take("a"), tokens.take("a"));
        let valid = [x.unwrap(), y.unwrap()]
            .iter()
            .filter(|t| matches!(t, Taken::Valid(_)))
            .count();
        assert_eq!(valid, 1);
    }

    /// An unknown token (a stale or garbage cookie) writes nothing.
    #[tokio::test]
    async fn an_unknown_token_writes_nothing() {
        let dir = crate::util::test_dir();
        let path = dir.path().join("t.json");
        let tokens = RefreshTokenStore::load(&path);
        assert!(matches!(tokens.take("nope").await.unwrap(), Taken::Unknown));
        assert!(!path.exists(), "no write for nothing");
        tokens.insert("a", entry("f"), opened()).await.unwrap();
        let written = std::fs::metadata(&path).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(matches!(tokens.take("nope").await.unwrap(), Taken::Unknown));
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            written
        );
        // an expired one going is a change worth keeping
        tokens
            .insert(
                "old",
                RefreshEntry {
                    expires_at: Utc::now().timestamp() - 1,
                    ..entry("g")
                },
                opened(),
            )
            .await
            .unwrap();
        assert!(matches!(tokens.take("nope").await.unwrap(), Taken::Unknown));
        assert!(
            !std::fs::read_to_string(&path)
                .unwrap()
                .contains(&hash_secret("old")),
            "pruned on disk"
        );
    }

    /// The file cannot be written: the refresh is refused, and nothing changes in memory (a
    /// restart would otherwise bring the spent token back).
    #[tokio::test]
    async fn a_store_that_cannot_write_refuses_the_change() {
        let dir = crate::util::test_dir();
        let path = dir.path().join("t.json");
        let tokens = RefreshTokenStore::load(&path);
        tokens.insert("a", entry("f"), opened()).await.unwrap();
        // the file's place becomes a directory: no write can replace it
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(tokens.take("a").await.is_err(), "refused, said");
        std::fs::remove_dir(&path).unwrap();
        assert!(
            matches!(tokens.take("a").await.unwrap(), Taken::Valid(_)),
            "still live: the failed take changed nothing"
        );
    }

    #[tokio::test]
    async fn sessions_are_listed_ended_one_by_one_and_by_person() {
        let tokens = RefreshTokenStore::default();
        tokens.insert("a", entry("f1"), opened()).await.unwrap();
        tokens.insert("b", entry("f2"), opened()).await.unwrap();
        tokens
            .insert(
                "c",
                RefreshEntry {
                    user_id: "alex".into(),
                    ..entry("f3")
                },
                Some(Opened::now("alex", "192.0.2.1".parse().unwrap(), "curl/8")),
            )
            .await
            .unwrap();
        let mine = tokens.sessions("leonard", "f1").await;
        assert_eq!(mine.len(), 2);
        let current = mine.iter().find(|s| s.current).unwrap();
        assert_eq!(
            (current.device.as_str(), current.ip.as_str()),
            ("iPhone", "198.51.100.7")
        );
        let other = mine.iter().find(|s| !s.current).unwrap().id.clone();
        assert!(
            !tokens.end_listed("alex", &other).await.unwrap(),
            "not someone else's"
        );
        assert!(tokens.end_listed("leonard", &other).await.unwrap());
        assert!(tokens.ended("f2").await);
        assert!(matches!(tokens.take("b").await.unwrap(), Taken::Unknown));
        assert_eq!(tokens.sessions("leonard", "f1").await.len(), 1);

        tokens.remove("a").await.unwrap();
        assert!(
            tokens.ended("f1").await,
            "signing out ends its access tokens"
        );
        assert!(tokens.sessions("leonard", "").await.is_empty());
        assert_eq!(tokens.remove_person("alex").await.unwrap(), 1);
        assert!(tokens.ended("f3").await);
        assert!(!tokens.ended("").await, "an old token without a sid");
    }
}
