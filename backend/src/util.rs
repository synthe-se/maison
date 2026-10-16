//! Small helpers every module shares, said once.

use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    sync::{Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

use sha2::{Digest, Sha256};

/// The house's time zone (never `chrono::Local`: UTC on the Pi), said once in the API crate.
pub use maison_api::HOUSE_TZ;

/// Today, in the house.
pub fn house_today() -> chrono::NaiveDate {
    chrono::Utc::now().with_timezone(&HOUSE_TZ).date_naive()
}

/// Bytes in lowercase hex, `sep` between them (« "" », « ":" » for a MAC, « " " » for a dump).
pub fn hex(bytes: &[u8], sep: &str) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(sep)
}

/// 256 random bits, base64url without padding (43 characters): every secret the backend hands
/// out (refresh, invitation and service tokens, pairing secrets, ceremony ids), made here only.
pub fn random_secret() -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>())
}

/// What is kept of a secret (a refresh, invitation or service token, a screen's pairing
/// secret): SHA-256 with a domain prefix. The secrets are 256 random bits, so a plain hash is
/// enough (no password stretching).
pub fn hash_secret(secret: &str) -> String {
    hex(
        &Sha256::new()
            .chain_update(b"maison/secret/v1\0")
            .chain_update(secret.as_bytes())
            .finalize(),
        "",
    )
}

/// Byte-wise comparison without a data-dependent early exit; a length mismatch only says
/// the length.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Now, in milliseconds since the epoch.
pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// A short random id (64 bits, hex): records, ceremonies, temp files. Not a secret.
pub fn random_id() -> String {
    hex(&rand::random::<[u8; 8]>(), "")
}

/// serde's `default = "util::default_true"` for flags that are on unless said otherwise.
pub fn default_true() -> bool {
    true
}

/// Characters in a name (a person's, a passkey's, a lamp's, a screen's…), at most.
pub const MAX_NAME: usize = 60;

/// A name as kept: trimmed, not empty, `MAX_NAME` characters at most, no control characters
/// (they would forge lines in the log). `None` otherwise.
pub fn clean_name(name: &str) -> Option<&str> {
    let name = name.trim();
    let ok =
        !name.is_empty() && name.chars().count() <= MAX_NAME && !name.chars().any(char::is_control);
    ok.then_some(name)
}

/// A name as kept (a person's, a passkey's, a lamp's, a shutter's, a scene's), see
/// [`clean_name`]; else the coded `bad_name` refusal.
pub fn name(value: &str) -> Result<&str, crate::error::AppError> {
    clean_name(value).ok_or_else(|| crate::error::Refusal::BadName.into())
}

/// A text field as kept: trimmed, and blank means absent.
pub fn non_blank(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// The keys of `value` that break the HTTP API's casing, with their path: every key is
/// camelCase ASCII (`peakStart`, never `peak_start` nor `dateDébut`), but for map keys that
/// are data, not names: numbers (keycodes, Tuya data points) and the Tempo colours
/// (`BLUE`, `WHITE`, `RED`). The tests run every API answer through it.
pub fn casing_offences(value: &serde_json::Value) -> Vec<String> {
    fn camel(key: &str) -> bool {
        key.starts_with(|c: char| c.is_ascii_lowercase())
            && key.chars().all(|c| c.is_ascii_alphanumeric())
    }
    fn data(key: &str) -> bool {
        (!key.is_empty() && key.chars().all(|c| c.is_ascii_digit()))
            || matches!(key, "BLUE" | "WHITE" | "RED")
    }
    fn walk(value: &serde_json::Value, path: &str, out: &mut Vec<String>) {
        match value {
            serde_json::Value::Object(entries) => {
                for (key, value) in entries {
                    let here = format!("{path}.{key}");
                    if !camel(key) && !data(key) {
                        out.push(here.clone());
                    }
                    walk(value, &here, out);
                }
            }
            serde_json::Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    walk(item, &format!("{path}[{index}]"), out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(value, "", &mut out);
    out
}

/// A std mutex's guard. A poisoned one (a panic while it was held) is taken anyway: what these
/// mutexes guard (limiter counts, ceremonies, pairings) stays consistent at every step, and a
/// panic once must not make every later sign-in or pairing panic too.
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

// ---------- client addresses ----------

/// What counts as one client address: an IPv4 address, or an IPv6 /64 (one household's prefix).
pub fn ip_group(ip: IpAddr) -> IpAddr {
    match ip.to_canonical() {
        IpAddr::V6(v6) => IpAddr::V6(Ipv6Addr::from_bits(v6.to_bits() & (u128::MAX << 64))),
        v4 => v4,
    }
}

/// A wider neighbourhood: an IPv4 /24, an IPv6 /48 (one site's allocation). Someone holding
/// many addresses usually holds them there.
pub fn ip_prefix(ip: IpAddr) -> IpAddr {
    match ip.to_canonical() {
        IpAddr::V6(v6) => IpAddr::V6(Ipv6Addr::from_bits(v6.to_bits() & (u128::MAX << 80))),
        IpAddr::V4(v4) => IpAddr::V4(Ipv4Addr::from_bits(v4.to_bits() & (u32::MAX << 8))),
    }
}

// ---------- bounded maps ----------

/// How many entries a [`Bounded`] map holds: in all, per client address ([`ip_group`]) and per
/// prefix ([`ip_prefix`]).
#[derive(Debug, Clone, Copy)]
pub struct Caps {
    pub total: usize,
    pub per_address: usize,
    pub per_prefix: usize,
}

/// One entry: when it came, from which client address ([`ip_group`]), and its value.
pub struct Held<V> {
    pub at: Instant,
    pub from: IpAddr,
    pub value: V,
}

type Keep<'a, V> = &'a dyn Fn(&Held<V>) -> bool;

/// Values kept a short while (`ttl`), by key, each counted against the client that asked: what
/// passkey ceremonies and screen pairings wait in. A full address loses its own oldest entry, a
/// full prefix its own oldest, a full map the oldest of its most crowded prefix: someone with
/// many addresses evicts their own entries before anyone else's. Not locked: its owner keeps it
/// in a `Mutex` ([`lock`]).
pub struct Bounded<V> {
    ttl: Duration,
    caps: Caps,
    entries: HashMap<String, Held<V>>,
}

impl<V> Bounded<V> {
    pub fn new(ttl: Duration, caps: Caps) -> Self {
        Self {
            ttl,
            caps,
            entries: HashMap::new(),
        }
    }

    fn fresh(&self, held: &Held<V>) -> bool {
        held.at.elapsed() < self.ttl
    }

    /// The key of the oldest entry `keep` accepts.
    fn oldest(&self, keep: Keep<'_, V>) -> Option<String> {
        self.entries
            .iter()
            .filter(|(_, h)| keep(h))
            .min_by_key(|(_, h)| h.at)
            .map(|(k, _)| k.clone())
    }

    /// Keeps `value` under `key`, from `ip`; the entries that had to go for it (stale ones
    /// included), for the caller to clean up after.
    pub fn insert(&mut self, key: String, ip: IpAddr, value: V) -> Vec<V> {
        let mut gone = self.expired();
        let from = ip_group(ip);
        let prefix = ip_prefix(from);
        let same_address = |h: &Held<V>| h.from == from;
        let same_prefix = |h: &Held<V>| ip_prefix(h.from) == prefix;
        let limits: [(Keep<'_, V>, usize); 2] = [
            (&same_address, self.caps.per_address),
            (&same_prefix, self.caps.per_prefix),
        ];
        for (keep, cap) in limits {
            if self.entries.values().filter(|h| keep(h)).count() >= cap {
                if let Some(k) = self.oldest(keep) {
                    gone.extend(self.entries.remove(&k).map(|h| h.value));
                }
            }
        }
        if self.entries.len() >= self.caps.total {
            let mut crowds: HashMap<IpAddr, usize> = HashMap::new();
            for h in self.entries.values() {
                *crowds.entry(ip_prefix(h.from)).or_default() += 1;
            }
            let most = crowds.values().copied().max().unwrap_or(0);
            let crowded = |h: &Held<V>| crowds.get(&ip_prefix(h.from)) == Some(&most);
            if let Some(k) = self.oldest(&crowded) {
                gone.extend(self.entries.remove(&k).map(|h| h.value));
            }
        }
        self.entries.insert(
            key,
            Held {
                at: Instant::now(),
                from,
                value,
            },
        );
        gone
    }

    /// The entry under `key`, taken out; `None` when missing or stale.
    pub fn take(&mut self, key: &str) -> Option<Held<V>> {
        let held = self.entries.remove(key)?;
        self.fresh(&held).then_some(held)
    }

    /// The fresh entry under `key`.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Held<V>> {
        let ttl = self.ttl;
        self.entries.get_mut(key).filter(|h| h.at.elapsed() < ttl)
    }

    /// The first fresh entry `matches` accepts.
    pub fn find_mut(&mut self, matches: impl Fn(&Held<V>) -> bool) -> Option<&mut Held<V>> {
        let ttl = self.ttl;
        self.entries
            .values_mut()
            .find(|h| h.at.elapsed() < ttl && matches(h))
    }

    /// The first fresh entry `matches` accepts.
    pub fn find(&self, matches: impl Fn(&Held<V>) -> bool) -> Option<&Held<V>> {
        self.entries.values().find(|h| self.fresh(h) && matches(h))
    }

    /// Every stale entry, taken out.
    pub fn expired(&mut self) -> Vec<V> {
        let ttl = self.ttl;
        let stale: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, h)| h.at.elapsed() >= ttl)
            .map(|(k, _)| k.clone())
            .collect();
        stale
            .iter()
            .filter_map(|k| self.entries.remove(k))
            .map(|h| h.value)
            .collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// A fresh directory for a unit test, removed when dropped: keep it alive while it is used.
#[cfg(test)]
pub fn test_dir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("maison-")
        .tempdir()
        .expect("a temp dir")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert_eq!(hex(&[0xde, 0xad, 0x01], ":"), "de:ad:01");
        assert_eq!(hex(&[0xbe, 0xef], ""), "beef");
        assert_eq!(
            random_secret().len(),
            43,
            "32 bytes, base64url without padding"
        );
        assert!(random_secret()
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
        assert_ne!(random_secret(), random_secret());
        assert_eq!(hash_secret("a").len(), 64);
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
        assert_eq!(non_blank(Some("  x ".into())), Some("x".into()));
        assert_eq!(non_blank(Some("   ".into())), None);
        assert_eq!(non_blank(None), None);
        assert_eq!(random_id().len(), 16);
        assert!(name("  Salon ").is_ok_and(|n| n == "Salon"));
        assert!(name(" ").is_err());
        assert_eq!(clean_name("  iPhone "), Some("iPhone"));
        assert_eq!(clean_name(&"x".repeat(MAX_NAME + 1)), None);
        assert_eq!(clean_name("a\nb"), None, "no forged log lines");
    }

    #[test]
    fn a_poisoned_lock_is_taken_anyway() {
        let m = std::sync::Arc::new(Mutex::new(1));
        let poisoner = m.clone();
        let _ = std::thread::spawn(move || {
            let _held = poisoner.lock().unwrap();
            panic!("while held");
        })
        .join();
        assert!(m.is_poisoned());
        *lock(&m) += 1;
        assert_eq!(*lock(&m), 2);
    }

    #[test]
    fn addresses_group_by_household_and_by_neighbourhood() {
        let ip = |s: &str| s.parse::<IpAddr>().unwrap();
        assert_eq!(ip_group(ip("2001:db8:1:2:aaaa::1")), ip("2001:db8:1:2::"));
        assert_eq!(ip_group(ip("::ffff:192.0.2.1")), ip("192.0.2.1"));
        assert_eq!(ip_prefix(ip("192.0.2.77")), ip("192.0.2.0"));
        assert_eq!(ip_prefix(ip("2001:db8:1:2::1")), ip("2001:db8:1::"));
    }

    fn caps(total: usize, per_address: usize, per_prefix: usize) -> Caps {
        Caps {
            total,
            per_address,
            per_prefix,
        }
    }

    #[test]
    fn a_bounded_map_keeps_fresh_entries_once() {
        let ip: IpAddr = "100.64.0.1".parse().unwrap();
        let mut m = Bounded::new(Duration::from_secs(60), caps(10, 10, 10));
        assert!(m.insert("a".into(), ip, 1).is_empty());
        assert_eq!(m.get_mut("a").map(|h| h.value), Some(1));
        assert!(m.find(|h| h.value == 1).is_some());
        assert_eq!(m.take("a").map(|h| h.value), Some(1));
        assert!(m.take("a").is_none(), "once");
        let mut stale = Bounded::new(Duration::ZERO, caps(10, 10, 10));
        stale.insert("b".into(), ip, 2);
        assert!(stale.take("b").is_none(), "too late");
        stale.insert("c".into(), ip, 3);
        assert!(stale.find_mut(|_| true).is_none());
        assert_eq!(stale.expired(), vec![3]);
        assert!(stale.is_empty());
    }

    #[test]
    fn a_crowd_evicts_its_own_entries_before_anyone_elses() {
        let mut m = Bounded::new(Duration::from_secs(60), caps(3, 2, 2));
        m.insert("home".into(), "198.51.100.7".parse().unwrap(), 0);
        // one address: its own oldest goes
        let a: IpAddr = "203.0.113.1".parse().unwrap();
        m.insert("a1".into(), a, 1);
        m.insert("a2".into(), a, 2);
        assert_eq!(m.insert("a3".into(), a, 3), vec![1]);
        // one prefix (/24): its own oldest goes
        assert_eq!(
            m.insert("b".into(), "203.0.113.2".parse().unwrap(), 4),
            vec![2]
        );
        // full: the most crowded prefix pays, never the house's one pairing
        assert_eq!(
            m.insert("c".into(), "192.0.2.9".parse().unwrap(), 5),
            vec![3]
        );
        assert!(m.get_mut("home").is_some());
        assert_eq!(m.len(), 3);
    }

    #[test]
    fn casing_offences_name_every_snake_or_non_ascii_key() {
        let fine = serde_json::json!({ "peakStart": "06:00", "probabilities": { "BLUE": 1 }, "keymap": { "353": {} }, "x": [{ "a1": 1 }] });
        assert!(casing_offences(&fine).is_empty());
        let bad = serde_json::json!({ "peak_start": 1, "days": [{ "dateDébut": 1, "Name": 2 }], "ok": { "raw_dps": {} } });
        let mut offences = casing_offences(&bad);
        offences.sort();
        assert_eq!(
            offences,
            [
                ".days[0].Name",
                ".days[0].dateDébut",
                ".ok.raw_dps",
                ".peak_start"
            ]
        );
    }
}

// ------------------------------------------------- devices, time, locks --

/// `base`, doubled at each `attempt` after the first (1: `base`, 2: twice, 3: four times…),
/// never past `max`: every retry and reconnect delay.
pub fn capped_backoff(
    base: std::time::Duration,
    attempt: u32,
    max: std::time::Duration,
) -> std::time::Duration {
    base.saturating_mul(1_u32 << attempt.saturating_sub(1).min(16))
        .min(max)
}

/// `hours:minutes` on `day` at the house, as an instant (the first one, on the night the
/// clocks go back; an hour later, for a time the clocks skip).
pub fn house_time(
    day: chrono::NaiveDate,
    hours: u32,
    minutes: u32,
) -> chrono::DateTime<chrono::Utc> {
    use chrono::TimeZone;
    let mut local = day.and_hms_opt(hours, minutes, 0).unwrap_or_default();
    for _ in 0..2 {
        if let Some(at) = HOUSE_TZ.from_local_datetime(&local).earliest() {
            return at.with_timezone(&chrono::Utc);
        }
        local += chrono::Duration::hours(1);
    }
    local.and_utc()
}

/// Hex digits (either case, two per byte, nothing else) as bytes; `None` for anything else.
pub fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).ok())
        .collect()
}

/// The coded refusal `code`, with why in the log (the app words the code; the reason is for
/// the log).
pub fn refused(
    code: crate::error::Refusal,
    reason: impl std::fmt::Display,
) -> crate::error::AppError {
    tracing::info!(code = code.as_str(), %reason, "refused");
    code.into()
}

/// `HH:MM` as minutes since midnight (`00:00` to `23:59`).
pub fn minutes_of_day(value: &str) -> Option<u16> {
    let (hours, minutes) = value.split_once(':')?;
    let digits = |s: &str| s.len() == 2 && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(hours) || !digits(minutes) {
        return None;
    }
    let (hours, minutes) = (hours.parse::<u16>().ok()?, minutes.parse::<u16>().ok()?);
    (hours < 24 && minutes < 60).then_some(hours * 60 + minutes)
}

#[cfg(test)]
mod device_helper_tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn backoffs_double_then_stay_capped() {
        let (base, max) = (Duration::from_secs(2), Duration::from_secs(300));
        let delays: Vec<u64> = (0..=10)
            .map(|attempt| capped_backoff(base, attempt, max).as_secs())
            .collect();
        assert_eq!(delays, [2, 2, 4, 8, 16, 32, 64, 128, 256, 300, 300]);
        assert_eq!(capped_backoff(base, u32::MAX, max), max, "no overflow");
    }

    #[test]
    fn house_times_are_paris_times() {
        let day = |m, d| chrono::NaiveDate::from_ymd_opt(2026, m, d).unwrap();
        assert_eq!(
            house_time(day(1, 10), 6, 0).to_rfc3339(),
            "2026-01-10T05:00:00+00:00"
        );
        assert_eq!(
            house_time(day(7, 10), 0, 0).to_rfc3339(),
            "2026-07-09T22:00:00+00:00"
        );
        // 02:30 does not exist on 29 March 2026: 03:30 summer time
        assert_eq!(
            house_time(day(3, 29), 2, 30).to_rfc3339(),
            "2026-03-29T01:30:00+00:00"
        );
        // 02:30 happens twice on 25 October 2026: the first
        assert_eq!(
            house_time(day(10, 25), 2, 30).to_rfc3339(),
            "2026-10-25T00:30:00+00:00"
        );
    }

    #[test]
    fn hex_reads_back() {
        assert_eq!(unhex("00ffA0"), Some(vec![0x00, 0xff, 0xa0]));
        assert_eq!(unhex(&hex(&[1, 2, 254], "")), Some(vec![1, 2, 254]));
        assert_eq!(unhex(""), Some(vec![]));
        for bad in ["0", "0g", "00:11", "+1"] {
            assert_eq!(unhex(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_poisoned_lock_still_opens() {
        let mutex = std::sync::Arc::new(std::sync::Mutex::new(1));
        let poisoner = mutex.clone();
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.lock().unwrap();
            panic!("poisoned on purpose");
        })
        .join();
        assert!(mutex.is_poisoned());
        *lock(&mutex) += 1;
        assert_eq!(*lock(&mutex), 2);
    }

    #[test]
    fn clock_times_read_as_minutes() {
        assert_eq!(minutes_of_day("00:00"), Some(0));
        assert_eq!(minutes_of_day("07:30"), Some(450));
        assert_eq!(minutes_of_day("23:59"), Some(1439));
        for bad in [
            "24:00", "12:60", "7:30", "07:3", "0730", "aa:bb", "-1:00", "07:30 ",
        ] {
            assert_eq!(minutes_of_day(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_refusal_keeps_its_code() {
        let error = refused(crate::error::Refusal::Invalid, "why");
        assert!(matches!(
            error,
            crate::error::AppError::Coded {
                code: "invalid",
                ..
            }
        ));
    }
}
