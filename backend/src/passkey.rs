//! Passkeys (WebAuthn), Ariane's implementation: what the sign-in routes rest on.
//!
//! - An admin (or `maison-backend invite`) makes a one-time link; only its hash is kept.
//! - The link registers a first passkey: a discoverable credential, user verification
//!   required, no attestation. The person exists from then on.
//! - Sign-in asks for any passkey of this site (usernameless): the authenticator says whose
//!   it is, the server checks the signature.
//! - A signed-in person adds more passkeys, names them, removes all but the last. Lost
//!   everything? An admin makes a new invitation for the same person.
//!
//! webauthn-rs checks challenge, origin, RP ID, user verification, signature and counter.
//! Ceremony states stay here, in memory, single-use, five minutes at most: webauthn-rs
//! insists they never reach the client. See docs/dependances/passkeys.md.

use std::collections::HashMap;
use std::future::Future;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::http::{HeaderMap, StatusCode};
use webauthn_rs::prelude::{DiscoverableAuthentication, PasskeyRegistration, Url, Uuid, Webauthn, WebauthnBuilder};

use crate::{config::Config, error::AppError, people::random_secret};

/// How long a ceremony may take, from options to answer; also the browser's timeout.
pub const CEREMONY_TTL: Duration = Duration::from_secs(300);
/// Ceremonies in flight at most; the oldest goes first.
const CEREMONY_CAP: usize = 10_000;
/// Ceremonies in flight per address at most; its oldest goes first.
const CEREMONIES_PER_ADDRESS: usize = 20;
/// Failures per address and window on the endpoints that verify something.
const ATTEMPTS: u32 = 10;
const ATTEMPT_WINDOW: Duration = Duration::from_secs(60);
/// Ceremonies started signed out, per address and window, successful or not.
const STARTS: u32 = 30;
/// How long an invitation link stays good.
pub const INVITE_DAYS: i64 = 7;

// ---------- refusals ----------

/// What the app says when it refuses, by name (the web app words each one).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    NotSignedIn,
    Forbidden,
    BadName,
    PasskeyOff,
    InviteInvalid,
    CeremonyExpired,
    PasskeyRejected,
    UnknownPasskey,
    PasskeyExists,
    LastPasskey,
    NotFound,
    TooManyAttempts { retry_after_s: u64 },
}

impl From<Refusal> for AppError {
    fn from(r: Refusal) -> Self {
        // no 401 but for « not signed in »: the web app reads a 401 as a session that ended
        let (status, code, message) = match r {
            Refusal::NotSignedIn => (StatusCode::UNAUTHORIZED, "not_signed_in", "Not signed in"),
            Refusal::Forbidden => (StatusCode::FORBIDDEN, "forbidden", "Admin privileges required"),
            Refusal::BadName => (StatusCode::BAD_REQUEST, "bad_name", "A name, 60 characters at most"),
            Refusal::PasskeyOff => (StatusCode::SERVICE_UNAVAILABLE, "passkey_off", "Passkeys need PUBLIC_URL (https)"),
            Refusal::InviteInvalid => (StatusCode::NOT_FOUND, "invite_invalid", "This invitation is no longer valid"),
            Refusal::CeremonyExpired => (StatusCode::BAD_REQUEST, "ceremony_expired", "Too late: start again"),
            Refusal::PasskeyRejected => (StatusCode::BAD_REQUEST, "passkey_rejected", "Passkey not accepted"),
            Refusal::UnknownPasskey => (StatusCode::NOT_FOUND, "unknown_passkey", "Unknown passkey"),
            Refusal::PasskeyExists => (StatusCode::CONFLICT, "passkey_exists", "Passkey already registered"),
            Refusal::LastPasskey => (StatusCode::CONFLICT, "last_passkey", "The last passkey stays"),
            Refusal::NotFound => (StatusCode::NOT_FOUND, "not_found", "Not found"),
            Refusal::TooManyAttempts { retry_after_s } => {
                return AppError::Coded {
                    status: StatusCode::TOO_MANY_REQUESTS,
                    code: "too_many_attempts",
                    message: "Too many attempts",
                    retry_after_s: Some(retry_after_s),
                };
            }
        };
        AppError::coded(status, code, message)
    }
}

// ---------- state ----------

/// Values kept server-side for a short while, each taken at most once.
pub struct Ceremonies<T> {
    ttl: Duration,
    cap: usize,
    per_address: usize,
    map: Mutex<HashMap<String, (Instant, IpAddr, T)>>,
}

impl<T> Ceremonies<T> {
    pub fn new(ttl: Duration, cap: usize, per_address: usize) -> Self {
        Self { ttl, cap, per_address, map: Mutex::new(HashMap::new()) }
    }

    /// Keeps `value`, started from `ip`; returns the id to give back with the answer.
    pub fn put(&self, ip: IpAddr, value: T) -> String {
        let id = random_secret();
        let from = address_key(ip);
        let mut map = self.map.lock().expect("ceremonies");
        map.retain(|_, (t, _, _)| t.elapsed() < self.ttl);
        let oldest = |map: &HashMap<String, (Instant, IpAddr, T)>, only: Option<IpAddr>| {
            map.iter()
                .filter(|(_, (_, a, _))| only.is_none_or(|o| *a == o))
                .min_by_key(|(_, (t, _, _))| *t)
                .map(|(k, _)| k.clone())
        };
        if map.values().filter(|(_, a, _)| *a == from).count() >= self.per_address {
            if let Some(k) = oldest(&map, Some(from)) {
                map.remove(&k);
            }
        }
        if map.len() >= self.cap {
            if let Some(k) = oldest(&map, None) {
                map.remove(&k);
            }
        }
        map.insert(id.clone(), (Instant::now(), from, value));
        id
    }

    /// The value, once, if it is still fresh.
    pub fn take(&self, id: &str) -> Option<T> {
        let (t, _, v) = self.map.lock().expect("ceremonies").remove(id)?;
        (t.elapsed() < self.ttl).then_some(v)
    }

    pub fn len(&self) -> usize {
        self.map.lock().expect("ceremonies").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Attempts per address, in a fixed window: failures only ([`Limiter::fail`]), or every
/// call ([`Limiter::hit`]). An IPv6 address counts as its /64. In memory: one process,
/// and a restart forgets, which is fine at this scale.
pub struct Limiter {
    max: u32,
    window: Duration,
    map: Mutex<HashMap<IpAddr, (Instant, u32)>>,
}

impl Limiter {
    pub fn new(max: u32, window: Duration) -> Self {
        Self { max, window, map: Mutex::new(HashMap::new()) }
    }

    /// `Err(seconds)` when the address has failed too often and must wait.
    pub fn check(&self, ip: IpAddr) -> Result<(), u64> {
        let map = self.map.lock().expect("limiter");
        match map.get(&address_key(ip)) {
            Some((start, n)) if *n >= self.max && start.elapsed() < self.window => Err(self.wait(*start)),
            _ => Ok(()),
        }
    }

    /// Counts a failure.
    pub fn fail(&self, ip: IpAddr) {
        self.count(ip);
    }

    /// Counts this call; `Err(seconds)` once the address is over the limit.
    pub fn hit(&self, ip: IpAddr) -> Result<(), u64> {
        self.check(ip)?;
        self.count(ip);
        Ok(())
    }

    fn count(&self, ip: IpAddr) {
        let mut map = self.map.lock().expect("limiter");
        if map.len() > 10_000 {
            map.retain(|_, (t, _)| t.elapsed() < self.window);
        }
        let now = Instant::now();
        let e = map.entry(address_key(ip)).or_insert((now, 0));
        if e.0.elapsed() >= self.window {
            *e = (now, 0);
        }
        e.1 += 1;
    }

    fn wait(&self, start: Instant) -> u64 {
        self.window.saturating_sub(start.elapsed()).as_secs().max(1)
    }
}

/// What counts as one address: an IPv4 address, or an IPv6 /64 (one household's prefix).
pub fn address_key(ip: IpAddr) -> IpAddr {
    match ip.to_canonical() {
        IpAddr::V6(v6) => IpAddr::V6(Ipv6Addr::from_bits(v6.to_bits() & (u128::MAX << 64))),
        v4 => v4,
    }
}

pub enum Via {
    /// first passkey (or a new one after a loss), by invitation
    Invite { id: String },
    /// another passkey, by someone signed in
    Session,
}

pub enum Ceremony {
    Register { state: PasskeyRegistration, person: String, handle: Uuid, via: Via },
    Login { state: DiscoverableAuthentication },
}

pub struct PasskeyState {
    pub webauthn: Webauthn,
    pub ceremonies: Ceremonies<Ceremony>,
    /// failures on the endpoints that verify something
    limiter: Limiter,
    /// ceremonies started signed out
    starts: Limiter,
    /// where invitation links point
    pub public_url: String,
}

impl PasskeyState {
    /// Passkeys at `PUBLIC_URL`; `None` (sign-in off, said in the log) without a usable one.
    pub fn new(config: &Config) -> Option<Self> {
        let built = config.public_url.as_deref().ok_or_else(|| "PUBLIC_URL is not set".to_string()).and_then(|url| {
            let (rp_id, origin) = relying_party(url)?;
            let webauthn = WebauthnBuilder::new(&rp_id, &origin)
                .and_then(|b| b.rp_name("Maison").timeout(CEREMONY_TTL).build())
                .map_err(|e| e.to_string())?;
            tracing::info!(%rp_id, %origin, "passkeys ready");
            Ok(webauthn)
        });
        match built {
            Ok(webauthn) => Some(Self {
                webauthn,
                ceremonies: Ceremonies::new(CEREMONY_TTL, CEREMONY_CAP, CEREMONIES_PER_ADDRESS),
                limiter: Limiter::new(ATTEMPTS, ATTEMPT_WINDOW),
                starts: Limiter::new(STARTS, ATTEMPT_WINDOW),
                public_url: config.public_url.clone().unwrap_or_default().trim_end_matches('/').to_string(),
            }),
            Err(error) => {
                tracing::warn!(%error, "passkeys off: nobody can sign in");
                None
            }
        }
    }

    /// Counts a ceremony started signed out; refuses past the limit.
    pub fn start_allowed(&self, ip: IpAddr) -> Result<(), AppError> {
        self.starts.hit(ip).map_err(|retry_after_s| {
            tracing::warn!(%ip, "passkey ceremonies: too many started");
            Refusal::TooManyAttempts { retry_after_s }.into()
        })
    }

    /// Runs `f` unless the address failed too often lately; a failure counts against it.
    pub async fn guarded<T>(&self, ip: IpAddr, f: impl Future<Output = Result<T, AppError>>) -> Result<T, AppError> {
        self.limiter.check(ip).map_err(|retry_after_s| {
            tracing::warn!(%ip, "passkey attempts: too many failures");
            AppError::from(Refusal::TooManyAttempts { retry_after_s })
        })?;
        let r = f.await;
        if r.is_err() {
            self.limiter.fail(ip);
        }
        r
    }
}

/// The RP ID and origin from the public address: a domain (never an IP address), https
/// (http only on localhost), no path. The RP ID never changes without losing every passkey.
pub fn relying_party(public_url: &str) -> Result<(String, Url), String> {
    let url = Url::parse(public_url).map_err(|e| format!("PUBLIC_URL: {e}"))?;
    let host = url.domain().ok_or("PUBLIC_URL: a domain name, not an IP address")?.to_string();
    let local = host == "localhost";
    if url.scheme() != "https" && !(local && url.scheme() == "http") {
        return Err("PUBLIC_URL: https (plain http on localhost only)".into());
    }
    if url.path() != "/" || url.query().is_some() {
        return Err("PUBLIC_URL: an origin, without a path".into());
    }
    Ok((host, url))
}

/// The address to count attempts against. cloudflared runs on the Pi: a request from
/// loopback says who it is for in `CF-Connecting-IP` (or `X-Real-IP`, or the last
/// `X-Forwarded-For` entry); from anywhere else, those headers are not believed.
pub fn client_ip(addr: SocketAddr, headers: &HeaderMap) -> IpAddr {
    let peer = addr.ip().to_canonical();
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let forwarded = peer
        .is_loopback()
        .then(|| {
            ["cf-connecting-ip", "x-real-ip"]
                .iter()
                .find_map(|h| header(h)?.trim().parse::<IpAddr>().ok())
                .or_else(|| header("x-forwarded-for")?.rsplit(',').next()?.trim().parse().ok())
        })
        .flatten();
    forwarded.map_or(peer, |f| f.to_canonical())
}

/// A passkey's first name, from the device it was made on; the person renames it.
/// Empty when unknown: the app shows its own word.
pub fn device_label(user_agent: &str) -> &'static str {
    const DEVICES: &[(&str, &str)] = &[
        ("iPhone", "iPhone"),
        ("iPad", "iPad"),
        ("Android", "Android"),
        ("CrOS", "ChromeOS"),
        ("Macintosh", "Mac"),
        ("Windows", "Windows"),
        ("Linux", "Linux"),
    ];
    DEVICES.iter().find(|(needle, _)| user_agent.contains(needle)).map_or("", |(_, label)| label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ceremonies_are_single_use_and_expire() {
        let ip: IpAddr = "100.64.0.1".parse().unwrap();
        let c = Ceremonies::new(Duration::from_secs(60), 3, 3);
        let a = c.put(ip, 1);
        assert_eq!(c.take(&a), Some(1));
        assert_eq!(c.take(&a), None, "single use");
        assert_eq!(c.take("nope"), None);

        let expired = Ceremonies::new(Duration::ZERO, 3, 3);
        let b = expired.put(ip, 2);
        assert_eq!(expired.take(&b), None, "older than its time limit");

        // full: the oldest goes
        let ids: Vec<_> = (0..4).map(|n| c.put(IpAddr::from([10, 0, 0, n as u8]), n)).collect();
        assert_eq!(c.len(), 3);
        assert_eq!(c.take(&ids[0]), None);
        assert_eq!(c.take(&ids[3]), Some(3));

        // one address has a few at most: its own oldest goes, not someone else's
        let c = Ceremonies::new(Duration::from_secs(60), 100, 2);
        let other = c.put("100.64.0.9".parse().unwrap(), 0);
        let ids: Vec<_> = (1..=3).map(|n| c.put(ip, n)).collect();
        assert_eq!(c.len(), 3);
        assert_eq!(c.take(&ids[0]), None);
        assert_eq!(c.take(&ids[2]), Some(3));
        assert_eq!(c.take(&other), Some(0));
    }

    #[test]
    fn every_call_counts_where_asked() {
        let l = Limiter::new(2, Duration::from_secs(60));
        let a: IpAddr = "2001:db8:1:2:aaaa::1".parse().unwrap();
        let same_prefix: IpAddr = "2001:db8:1:2:bbbb::2".parse().unwrap();
        let other: IpAddr = "2001:db8:1:3::1".parse().unwrap();
        assert!(l.hit(a).is_ok());
        assert!(l.hit(same_prefix).is_ok());
        assert!(l.hit(a).is_err(), "an IPv6 /64 is one address");
        assert!(l.hit(other).is_ok());
        assert_eq!(address_key("::ffff:192.0.2.1".parse().unwrap()), "192.0.2.1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn failures_are_limited_per_address() {
        let l = Limiter::new(2, Duration::from_secs(60));
        let a: IpAddr = "100.64.0.1".parse().unwrap();
        let b: IpAddr = "100.64.0.2".parse().unwrap();
        assert!(l.check(a).is_ok());
        l.fail(a);
        assert!(l.check(a).is_ok());
        l.fail(a);
        let wait = l.check(a).unwrap_err();
        assert!((1..=60).contains(&wait));
        assert!(l.check(b).is_ok(), "another address has its own count");
        let short = Limiter::new(1, Duration::ZERO);
        short.fail(a);
        assert!(short.check(a).is_ok(), "the window is over");
    }

    #[test]
    fn forwarded_address_only_from_the_local_tunnel() {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "203.0.113.9, 100.101.102.103".parse().unwrap());
        let local: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        let remote: SocketAddr = "192.168.1.20:5000".parse().unwrap();
        let ip = |s: &str| s.parse::<IpAddr>().unwrap();
        assert_eq!(client_ip(local, &h), ip("100.101.102.103"));
        assert_eq!(client_ip(remote, &h), ip("192.168.1.20"));
        assert_eq!(client_ip(local, &HeaderMap::new()), ip("127.0.0.1"));
        h.insert("cf-connecting-ip", "198.51.100.7".parse().unwrap());
        assert_eq!(client_ip(local, &h), ip("198.51.100.7"), "Cloudflare's word first");
        assert_eq!(client_ip(remote, &h), ip("192.168.1.20"), "not believed from the LAN");
    }

    #[test]
    fn devices_name_passkeys() {
        let iphone = "Mozilla/5.0 (iPhone; CPU iPhone OS 19_0 like Mac OS X) AppleWebKit/605.1.15";
        assert_eq!(device_label(iphone), "iPhone");
        assert_eq!(device_label("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)"), "Mac");
        assert_eq!(device_label("Mozilla/5.0 (Linux; Android 16; Pixel 10)"), "Android");
        assert_eq!(device_label("curl/8"), "");
    }

    #[test]
    fn relying_party_from_the_public_address() {
        let (id, origin) = relying_party("https://home.kahn.studio").unwrap();
        assert_eq!(id, "home.kahn.studio");
        assert_eq!(origin.as_str(), "https://home.kahn.studio/");
        assert!(relying_party("http://localhost:5173").is_ok());
        assert!(relying_party("https://192.168.1.103").is_err(), "no IP address");
        assert!(relying_party("http://home.kahn.studio").is_err(), "no plain http");
        assert!(relying_party("https://home.kahn.studio/maison").is_err(), "an origin, no path");
    }
}
