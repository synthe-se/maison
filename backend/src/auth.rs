//! Who is asking, once signed in (signing in is `routes::passkeys`).
//!
//! - The access token (a JWT in an HttpOnly cookie, 15 min) says only who and when: the
//!   person's name and role are read from `people.rs` on every request, so a removed person
//!   or a changed role takes effect at once, and « sign out everywhere » (the person's
//!   `not_before_ms`) ends every access token too.
//! - The refresh token (7 days, rotating) is kept hashed; a spent one coming back means it
//!   was copied: its whole family (every token rotated from the same sign-in) ends.
//! - `MachineClient`: the IR bridge's bearer token, nothing else.

use std::{collections::HashMap, path::{Path, PathBuf}, sync::Arc};

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
    people::{Person, ADMIN},
    store::{self, Access, Corrupt},
    util::{constant_time_eq, hash_secret},
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
    pub exp: usize,
}

/// Who is signed in, as the store says now.
#[derive(Debug, Clone, Serialize)]
pub struct AuthUser {
    pub id: String,
    pub name: String,
    pub role: String,
    /// When they last signed in with a passkey (ms).
    #[serde(skip)]
    pub auth_ms: i64,
}

impl AuthUser {
    pub fn of(person: &Person, auth_ms: i64) -> Self {
        Self { id: person.id.clone(), name: person.name.clone(), role: person.role.clone(), auth_ms }
    }

    pub fn is_admin(&self) -> bool {
        self.role == ADMIN
    }
}

/// The session cookies' names: `__Host-` prefixed when secure (bound to this exact host,
/// path `/`, so a sibling subdomain can neither read nor plant them).
pub fn cookie_names(config: &Config) -> (String, String) {
    if config.auth_cookie_secure {
        (format!("__Host-{}", config.auth_cookie_name), "__Host-maison_refresh".into())
    } else {
        (config.auth_cookie_name.clone(), "maison_refresh".into())
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
        let token = extract_bearer_token(&parts.headers)
            .ok()
            .or_else(|| cookie_value(&parts.headers, &access))
            .ok_or(Refusal::NotSignedIn)?;
        let claims = decode_token(token, state.config.jwt_secret.as_bytes()).map_err(|_| Refusal::NotSignedIn)?.claims;
        let person = state.people.person(&claims.user_id).await?.ok_or(Refusal::NotSignedIn)?;
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

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Option<Self>, Self::Rejection> {
        Ok(<Self as FromRequestParts<S>>::from_request_parts(parts, state).await.ok())
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
        let Some(expected) = app_state.config.ir_api_token.as_deref() else {
            return Err(AppError::unauthorized("Machine API disabled: IR_API_TOKEN is not configured"));
        };
        let token = extract_bearer_token(&parts.headers)?;
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

pub fn extract_bearer_token(headers: &HeaderMap) -> Result<&str, AppError> {
    headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::unauthorized("No token provided"))
}

pub fn decode_token(token: &str, secret: &[u8]) -> Result<jsonwebtoken::TokenData<Claims>, jsonwebtoken::errors::Error> {
    decode::<Claims>(token, &DecodingKey::from_secret(secret), &Validation::default())
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
    Replayed { user_id: Option<String> },
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
        Self { inner: Arc::new(Mutex::new(tokens)), path: Some(Arc::new(path.to_path_buf())) }
    }

    pub async fn insert(&self, token: &str, entry: RefreshEntry) {
        let mut tokens = self.inner.lock().await;
        tokens.live.insert(hash_secret(token), entry);
        self.persist(&tokens);
    }

    /// The token, spent in the same breath (two refreshes racing with it: one wins).
    pub async fn take(&self, token: &str) -> Taken {
        let now = Utc::now().timestamp();
        let key = hash_secret(token);
        let mut tokens = self.inner.lock().await;
        prune(&mut tokens, now);
        let taken = if let Some(entry) = tokens.live.remove(&key) {
            tokens.spent.insert(key, Spent { family: entry.family.clone(), expires_at: entry.expires_at });
            Taken::Valid(entry)
        } else if let Some(spent) = tokens.spent.get(&key).cloned() {
            let user_id = tokens.live.values().find(|e| e.family == spent.family).map(|e| e.user_id.clone());
            tokens.live.retain(|_, e| e.family != spent.family);
            Taken::Replayed { user_id }
        } else {
            Taken::Unknown
        };
        self.persist(&tokens);
        taken
    }

    /// Every refresh token of a person: how many went.
    pub async fn remove_person(&self, user_id: &str) -> usize {
        let mut tokens = self.inner.lock().await;
        let before = tokens.live.len();
        tokens.live.retain(|_, e| e.user_id != user_id);
        let removed = before - tokens.live.len();
        if removed > 0 {
            self.persist(&tokens);
        }
        removed
    }

    /// One token (logout).
    pub async fn remove(&self, token: &str) {
        let mut tokens = self.inner.lock().await;
        if tokens.live.remove(&hash_secret(token)).is_some() {
            self.persist(&tokens);
        }
    }

    /// Write-through; failures are logged, never propagated: an unwritable disk must not
    /// break signing in (the tokens then live until the next restart).
    fn persist(&self, tokens: &Tokens) {
        if let Some(path) = self.path.as_deref() {
            if let Err(error) = store::write_json(path, tokens, Access::Private) {
                tracing::warn!(%error, "failed to persist refresh tokens");
            }
        }
    }
}

fn prune(tokens: &mut Tokens, now: i64) {
    tokens.live.retain(|_, e| e.expires_at > now);
    tokens.spent.retain(|_, s| s.expires_at > now);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(family: &str) -> RefreshEntry {
        RefreshEntry { user_id: "leonard".into(), expires_at: Utc::now().timestamp() + 60, family: family.into(), auth_ms: 0 }
    }

    #[tokio::test]
    async fn a_token_is_spent_once_and_a_replay_ends_its_family() {
        let path = std::env::temp_dir().join(format!("maison-refresh-{}", crate::util::random_secret())).join("t.json");
        let tokens = RefreshTokenStore::load(&path);
        tokens.insert("a", entry("f1")).await;
        tokens.insert("other", entry("f2")).await;
        assert!(matches!(tokens.take("a").await, Taken::Valid(_)));
        tokens.insert("b", entry("f1")).await; // rotated
        assert!(matches!(tokens.take("a").await, Taken::Replayed { .. }), "a copy of a spent token");
        assert!(matches!(tokens.take("b").await, Taken::Replayed { .. } | Taken::Unknown), "its family ended");
        assert!(matches!(tokens.take("other").await, Taken::Valid(_)), "another sign-in untouched");
        assert!(matches!(tokens.take("nope").await, Taken::Unknown));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("\"other\"") && !text.contains("\"b\""), "only hashes on disk");
        // reloaded from disk: a spent token is still known as spent
        let again = RefreshTokenStore::load(&path);
        assert!(matches!(again.take("other").await, Taken::Replayed { .. }));
    }

    #[tokio::test]
    async fn two_refreshes_racing_with_one_token_one_wins() {
        let tokens = RefreshTokenStore::default();
        tokens.insert("a", entry("f")).await;
        let (x, y) = tokio::join!(tokens.take("a"), tokens.take("a"));
        let valid = [x, y].iter().filter(|t| matches!(t, Taken::Valid(_))).count();
        assert_eq!(valid, 1);
    }
}
