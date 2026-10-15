//! Who is asking, once signed in (signing in is `routes::passkeys`).
//!
//! - The access token (a JWT in an HttpOnly cookie, 15 min) says only who and when: the
//!   person's name and role are read from `people.rs` on every request, so a removed person
//!   or a changed role takes effect at once, and « sign out everywhere » (the person's
//!   `not_before_ms`) ends every access token too.
//! - The refresh token (7 days, rotating) is kept hashed; a spent one coming back means it
//!   was copied: its whole family (every token rotated from the same sign-in) ends.
//! - `DeviceClient`: who drives the devices, the route layer of every device router:
//!   someone signed in, or a machine with a service token (`maison_…`, made by
//!   `maison-backend token create`, kept hashed in `auth.json`). A service token is refused
//!   everywhere else (`AuthenticatedUser`, `AdminUser`): people, passkeys, invitations,
//!   tokens and settings stay with people.
//! - `MachineClient`: the IR bridge's bearer token, nothing else.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use axum::{
    extract::{FromRef, FromRequestParts},
    http::{header::AUTHORIZATION, request::Parts, HeaderMap},
};
use chrono::Utc;
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{
    config::Config,
    error::AppError,
    passkey::Refusal,
    people::{People, Person, Role, Scope, StoredToken, TokenKind},
    store::{self, Access, Corrupt},
    util::{constant_time_eq, hash_secret, now_ms, random_id},
    AppState,
};

/// How recent the last passkey sign-in must be to add a passkey from a session.
pub const REAUTH_MS: i64 = 5 * 60 * 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Claims {
    pub user_id: String,
    /// When this token was issued (ms): older than the person's `not_before_ms` is void.
    pub issued_ms: i64,
    /// When the person last proved it with a passkey (ms), carried across refreshes.
    pub auth_ms: i64,
    pub exp: u64,
}

/// Who is signed in, as the store says now.
#[derive(Debug, Clone, Serialize)]
pub struct AuthUser {
    pub id: String,
    pub name: String,
    pub role: Role,
    /// When they last signed in with a passkey (ms).
    #[serde(skip)]
    pub auth_ms: i64,
}

impl AuthUser {
    pub fn of(person: &Person, auth_ms: i64) -> Self {
        Self {
            id: person.id.clone(),
            name: person.name.clone(),
            role: person.role,
            auth_ms,
        }
    }

    pub fn is_admin(&self) -> bool {
        self.role == Role::Admin
    }
}

/// The session cookies' names: `__Host-` prefixed when secure (bound to this exact host,
/// path `/`, so a sibling subdomain can neither read nor plant them).
pub fn cookie_names(config: &Config) -> (String, String) {
    if config.auth_cookie_secure {
        (
            format!("__Host-{}", config.auth_cookie_name),
            "__Host-maison_refresh".into(),
        )
    } else {
        (config.auth_cookie_name.clone(), "maison_refresh".into())
    }
}

/// A paired wall screen's cookie (`screens.rs`): its token, HttpOnly, `__Host-` when secure.
pub fn screen_cookie_name(config: &Config) -> String {
    if config.auth_cookie_secure {
        "__Host-maison_screen".into()
    } else {
        "maison_screen".into()
    }
}

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
        let (access, _) = cookie_names(&state.config);
        let bearer = extract_bearer_token(&parts.headers).ok();
        if bearer.is_some_and(is_service_token) {
            // a machine where only a person may be
            return Err(Refusal::Forbidden.into());
        }
        let token = bearer
            .or_else(|| cookie_value(&parts.headers, &access))
            .ok_or(Refusal::NotSignedIn)?;
        let claims = decode_token(token, state.config.jwt_secret.as_bytes())
            .map_err(|_| Refusal::NotSignedIn)?
            .claims;
        let person = state
            .people
            .person(&claims.user_id)
            .await?
            .ok_or(Refusal::NotSignedIn)?;
        if claims.issued_ms < person.not_before_ms {
            return Err(Refusal::NotSignedIn.into());
        }
        Ok(Self(AuthUser::of(&person, claims.auth_ms)))
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
/// control needs only `AuthenticatedUser` (the route layer in `lib.rs`).
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

/// Who drives the devices: a service token with the `devices` scope (an app's, sent as a bearer
/// header), someone signed in, or a paired wall screen (its token in its HttpOnly cookie), in
/// that order: someone signing in on a screen's browser is themselves. The device routers'
/// layer (`lib.rs`); a handler that needs more asks `AdminUser`, which a token never is.
#[derive(Debug, Clone)]
pub enum DeviceClient {
    Person(AuthUser),
    /// A service token, by its name.
    Service(String),
}

impl DeviceClient {
    pub fn is_admin(&self) -> bool {
        matches!(self, Self::Person(user) if user.is_admin())
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
        if let Some(token) = extract_bearer_token(&parts.headers)
            .ok()
            .filter(|token| is_service_token(token))
        {
            return service_token(&app_state.people, token, Scope::Devices, now_ms())
                .await
                .map(|t| Self::Service(t.name));
        }
        let refused = match AuthenticatedUser::from_request_parts(parts, state).await {
            Ok(user) => return Ok(Self::Person(user.0)),
            Err(refused) => refused,
        };
        match cookie_value(&parts.headers, &screen_cookie_name(&app_state.config))
            .filter(|token| is_service_token(token))
        {
            Some(token) => service_token(&app_state.people, token, Scope::Devices, now_ms())
                .await
                .map(|t| Self::Service(t.name)),
            None => Err(refused),
        }
    }
}

/// What every service token starts with: told apart from a session's JWT (`eyJ…`) at a glance,
/// by the request guard too.
pub const SERVICE_TOKEN_PREFIX: &str = "maison_";

/// How often a token's last use is written down, at most: an hour (the SD card counts writes).
pub const TOKEN_USE_STEP_MS: i64 = 60 * 60 * 1000;

pub fn is_service_token(token: &str) -> bool {
    token.starts_with(SERVICE_TOKEN_PREFIX)
}

/// Whether the request carries a service token (said, not checked: the guard's question).
pub fn has_service_token(headers: &HeaderMap) -> bool {
    extract_bearer_token(headers).is_ok_and(is_service_token)
}

/// A new service token named `name`, for `scope`, carried as `kind` says: the token, to show
/// once (only its hash is kept); `None` when another token has the name. 256 random bits,
/// base64url.
pub async fn issue_service_token(
    people: &People,
    name: &str,
    scope: Scope,
    kind: TokenKind,
) -> Result<Option<String>, AppError> {
    use base64::Engine;
    let name = crate::util::name(name)?.to_string();
    let token = format!(
        "{SERVICE_TOKEN_PREFIX}{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>())
    );
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

/// The service token `token`, when it exists and carries `scope`; its use is recorded
/// (`TOKEN_USE_STEP_MS`). Unknown or revoked: not signed in.
pub(crate) async fn service_token(
    people: &People,
    token: &str,
    scope: Scope,
    now_ms: i64,
) -> Result<StoredToken, AppError> {
    let found = people
        .token_by_hash(&hash_secret(token))
        .await?
        .ok_or(Refusal::NotSignedIn)?;
    if found.scope != scope {
        return Err(Refusal::Forbidden.into());
    }
    if found
        .last_used_ms
        .is_none_or(|at| now_ms - at >= TOKEN_USE_STEP_MS)
    {
        // a failed write costs only the date: the call goes on
        if let Err(error) = people.token_used(&found.id, now_ms).await {
            tracing::warn!(%error, token = %found.name, "service token use not recorded");
        }
    }
    Ok(found)
}

/// Extractor for machine-to-machine clients (the kird IR bridge on the STB): a raw bearer
/// token compared in constant time against `IR_API_TOKEN`. Fails closed when the token is
/// not configured. Deliberately not chained with the session: a machine route accepts the
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
        let token = extract_bearer_token(&parts.headers)?;
        // unset or wrong, the same answer: the LAN learns nothing of the setup
        let Some(expected) = app_state.config.ir_api_token.as_deref() else {
            tracing::warn!("a machine call, but IR_API_TOKEN is not set: refused");
            return Err(AppError::unauthorized("Invalid machine token"));
        };
        if !constant_time_eq(token.as_bytes(), expected.as_bytes()) {
            return Err(AppError::unauthorized("Invalid machine token"));
        }
        Ok(Self)
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

fn extract_bearer_token(headers: &HeaderMap) -> Result<&str, AppError> {
    headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::unauthorized("No token provided"))
}

fn decode_token(
    token: &str,
    secret: &[u8],
) -> Result<jsonwebtoken::TokenData<Claims>, jsonwebtoken::errors::Error> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret),
        &Validation::default(),
    )
}

// ---------- refresh tokens ----------

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

#[derive(Debug, Default, Serialize, Deserialize)]
struct Tokens {
    /// By `hash_secret(token)`: the file holds no usable token.
    #[serde(default)]
    live: HashMap<String, RefreshEntry>,
    #[serde(default)]
    spent: HashMap<String, Spent>,
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

/// Refresh tokens, persisted (0600) so a restart or a deploy keeps everyone signed in.
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

    pub async fn insert(&self, token: &str, entry: RefreshEntry) {
        let mut tokens = self.inner.lock().await;
        tokens.live.insert(hash_secret(token), entry);
        self.persist(&tokens).await;
    }

    /// The token, spent in the same breath (two refreshes racing with it: one wins).
    pub async fn take(&self, token: &str) -> Taken {
        let now = Utc::now().timestamp();
        let key = hash_secret(token);
        let mut tokens = self.inner.lock().await;
        let mut changed = prune(&mut tokens, now);
        let taken = if let Some(entry) = tokens.live.remove(&key) {
            tokens.spent.insert(
                key,
                Spent {
                    family: entry.family.clone(),
                    expires_at: entry.expires_at,
                },
            );
            changed = true;
            Taken::Valid(entry)
        } else if let Some(spent) = tokens.spent.get(&key).cloned() {
            let user_id = tokens
                .live
                .values()
                .find(|e| e.family == spent.family)
                .map(|e| e.user_id.clone());
            let before = tokens.live.len();
            tokens.live.retain(|_, e| e.family != spent.family);
            changed |= tokens.live.len() != before;
            Taken::Replayed { user_id }
        } else {
            Taken::Unknown
        };
        // a garbage cookie costs no write (nor an fsync)
        if changed {
            self.persist(&tokens).await;
        }
        taken
    }

    /// Every refresh token of a person: how many went.
    pub async fn remove_person(&self, user_id: &str) -> usize {
        let mut tokens = self.inner.lock().await;
        let before = tokens.live.len();
        tokens.live.retain(|_, e| e.user_id != user_id);
        let removed = before - tokens.live.len();
        if removed > 0 {
            self.persist(&tokens).await;
        }
        removed
    }

    /// One token (logout).
    pub async fn remove(&self, token: &str) {
        let mut tokens = self.inner.lock().await;
        if tokens.live.remove(&hash_secret(token)).is_some() {
            self.persist(&tokens).await;
        }
    }

    /// Write-through; failures are logged, never propagated: an unwritable disk must not
    /// break signing in (the tokens then live until the next restart).
    async fn persist(&self, tokens: &Tokens) {
        if let Some(path) = self.path.as_deref() {
            if let Err(error) = store::write_json_async(path, tokens, Access::Private).await {
                tracing::warn!(%error, "failed to persist refresh tokens");
            }
        }
    }
}

/// The expired go; whether any did.
fn prune(tokens: &mut Tokens, now: i64) -> bool {
    let before = tokens.live.len() + tokens.spent.len();
    tokens.live.retain(|_, e| e.expires_at > now);
    tokens.spent.retain(|_, s| s.expires_at > now);
    tokens.live.len() + tokens.spent.len() != before
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

        assert_eq!(
            service_token(&people, &token, Scope::Devices, 1_000)
                .await
                .unwrap()
                .name,
            "achi"
        );
        let used = || async { people.tokens().await.unwrap()[0].last_used_ms };
        assert_eq!(used().await, Some(1_000));
        service_token(
            &people,
            &token,
            Scope::Devices,
            1_000 + TOKEN_USE_STEP_MS - 1,
        )
        .await
        .unwrap();
        assert_eq!(used().await, Some(1_000), "not rewritten within the hour");
        service_token(&people, &token, Scope::Devices, 1_000 + TOKEN_USE_STEP_MS)
            .await
            .unwrap();
        assert_eq!(used().await, Some(1_000 + TOKEN_USE_STEP_MS));

        let forged = format!("{SERVICE_TOKEN_PREFIX}{}", "A".repeat(43));
        assert!(service_token(&people, &forged, Scope::Devices, 0)
            .await
            .is_err());
        assert!(people.revoke_token("achi").await.unwrap());
        assert!(
            service_token(&people, &token, Scope::Devices, 0)
                .await
                .is_err(),
            "revoked at once"
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

    #[tokio::test]
    async fn a_token_is_spent_once_and_a_replay_ends_its_family() {
        let dir = crate::util::test_dir();
        let path = dir.path().join("t.json");
        let tokens = RefreshTokenStore::load(&path);
        tokens.insert("a", entry("f1")).await;
        tokens.insert("other", entry("f2")).await;
        assert!(matches!(tokens.take("a").await, Taken::Valid(_)));
        tokens.insert("b", entry("f1")).await; // rotated
        assert!(
            matches!(tokens.take("a").await, Taken::Replayed { .. }),
            "a copy of a spent token"
        );
        assert!(
            matches!(
                tokens.take("b").await,
                Taken::Replayed { .. } | Taken::Unknown
            ),
            "its family ended"
        );
        assert!(
            matches!(tokens.take("other").await, Taken::Valid(_)),
            "another sign-in untouched"
        );
        assert!(matches!(tokens.take("nope").await, Taken::Unknown));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            !text.contains("\"other\"") && !text.contains("\"b\""),
            "only hashes on disk"
        );
        // reloaded from disk: a spent token is still known as spent
        let again = RefreshTokenStore::load(&path);
        assert!(matches!(again.take("other").await, Taken::Replayed { .. }));
    }

    #[tokio::test]
    async fn two_refreshes_racing_with_one_token_one_wins() {
        let tokens = RefreshTokenStore::default();
        tokens.insert("a", entry("f")).await;
        let (x, y) = tokio::join!(tokens.take("a"), tokens.take("a"));
        let valid = [x, y]
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
        assert!(matches!(tokens.take("nope").await, Taken::Unknown));
        assert!(!path.exists(), "no write for nothing");
        tokens.insert("a", entry("f")).await;
        let written = std::fs::metadata(&path).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(matches!(tokens.take("nope").await, Taken::Unknown));
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
            )
            .await;
        assert!(matches!(tokens.take("nope").await, Taken::Unknown));
        assert!(
            !std::fs::read_to_string(&path)
                .unwrap()
                .contains(&hash_secret("old")),
            "pruned on disk"
        );
    }
}
