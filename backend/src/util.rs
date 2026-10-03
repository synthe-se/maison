//! Small helpers every module shares, said once.

use sha2::{Digest, Sha256};

/// Bytes in lowercase hex, `sep` between them (« "" », « ":" » for a MAC, « " " » for a dump).
pub fn hex(bytes: &[u8], sep: &str) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(sep)
}

/// 256 random bits, in hex: tokens, ceremony and record ids.
pub fn random_secret() -> String {
    hex(&rand::random::<[u8; 32]>(), "")
}

/// What is kept of a secret (an invitation or refresh token): SHA-256 with a domain prefix.
pub fn hash_secret(secret: &str) -> String {
    hex(&Sha256::new().chain_update(b"maison/secret/v1\0").chain_update(secret.as_bytes()).finalize(), "")
}

/// Byte-wise comparison without a data-dependent early exit; a length mismatch only says
/// the length.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// A text field as kept: trimmed, and blank means absent.
pub fn non_blank(value: Option<String>) -> Option<String> {
    value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
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
    }
}
