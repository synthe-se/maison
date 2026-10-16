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

/// `base` doubled per `attempt` (from 0), `max` at most: a reconnect's wait.
pub fn capped_backoff(
    base: std::time::Duration,
    attempt: u32,
    max: std::time::Duration,
) -> std::time::Duration {
    base.saturating_mul(2u32.saturating_pow(attempt)).min(max)
}

/// A std mutex's guard, even after a panic while it was held: what it guards stays usable
/// (a poisoned lock would otherwise panic every caller after the first).
pub fn lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The coded refusal `code`, with why in the log (the app words the code, the reason is the
/// owner's).
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

/// Hex digits as bytes (either case, two per byte); `None` for anything else.
pub fn unhex(value: &str) -> Option<Vec<u8>> {
    if !value.len().is_multiple_of(2) || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).ok())
        .collect()
}

#[cfg(test)]
mod appended_tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn backoff_doubles_up_to_its_cap() {
        let (base, max) = (Duration::from_secs(1), Duration::from_secs(30));
        let waits: Vec<u64> = (0..7)
            .map(|n| capped_backoff(base, n, max).as_secs())
            .collect();
        assert_eq!(waits, [1, 2, 4, 8, 16, 30, 30]);
        assert_eq!(capped_backoff(base, u32::MAX, max), max, "no overflow");
    }

    #[test]
    fn a_poisoned_lock_still_opens() {
        let mutex = std::sync::Arc::new(std::sync::Mutex::new(1));
        let held = mutex.clone();
        let _ = std::thread::spawn(move || {
            let _guard = held.lock().unwrap();
            panic!("poisons");
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
    fn hex_reads_back() {
        assert_eq!(unhex("0aFf10"), Some(vec![0x0a, 0xff, 0x10]));
        assert_eq!(unhex(&hex(&[1, 2, 254], "")), Some(vec![1, 2, 254]));
        assert_eq!(unhex(""), Some(Vec::new()));
        for bad in ["abc", "zz", "+1", "0x", "é1"] {
            assert_eq!(unhex(bad), None, "{bad}");
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
