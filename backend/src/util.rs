//! Small helpers every module shares, said once.

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

/// 256 random bits, in hex: tokens, ceremony and record ids.
pub fn random_secret() -> String {
    hex(&rand::random::<[u8; 32]>(), "")
}

/// What is kept of a secret (an invitation or refresh token): SHA-256 with a domain prefix.
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

/// A name as kept (a person's, a passkey's, a lamp's, a shutter's, a scene's): trimmed, not
/// empty, 60 characters at most, no control characters; else the coded `bad_name` refusal.
pub fn name(value: &str) -> Result<&str, crate::error::AppError> {
    crate::people::clean_name(value).ok_or_else(|| crate::passkey::Refusal::BadName.into())
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
        assert_eq!(random_secret().len(), 64);
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

/// A std mutex's guard, a poisoned one included: a thread that panicked while holding it
/// must not make every later caller panic too (what it guards is plain data).
pub fn lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
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
}
