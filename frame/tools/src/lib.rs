//! What the frame's updates share: the signed manifest, what it may say, and how a release is
//! checked. A release is `manifest.json`, its detached Ed25519 signature `manifest.sig`, and the
//! files it names, under `<version>/`. The private key never leaves the build machine.

use ring::digest::{Context, SHA256};
use ring::signature::{ED25519, UnparsedPublicKey};
use serde::{Deserialize, Serialize};

/// The release key: `maison-sign keygen` writes it here; the frame trusts nothing else.
pub const RELEASE_KEY_HEX: &str = include_str!("../release.pub");

/// What a slot may hold, and nothing else: a name comes from the network and becomes a path.
pub const SLOT_FILES: [&str; 4] = [
    "rootfs.squashfs",
    "zImage",
    "board.dtb",
    "initramfs.cpio.gz",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub version: u64,
    pub files: Vec<File>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct File {
    pub name: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, PartialEq)]
pub enum Refused {
    BadKey,
    BadSignature,
    BadManifest(String),
}

pub fn unhex(text: &str) -> Option<Vec<u8>> {
    let text = text.trim();
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| {
            text.get(i..i + 2)
                .and_then(|b| u8::from_str_radix(b, 16).ok())
        })
        .collect()
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The manifest, only when `key` signed exactly these bytes and what it says can be trusted to
/// become paths on the frame.
pub fn verified(bytes: &[u8], signature: &[u8], key_hex: &str) -> Result<Manifest, Refused> {
    let key = unhex(key_hex)
        .filter(|k| k.len() == 32)
        .ok_or(Refused::BadKey)?;
    UnparsedPublicKey::new(&ED25519, key)
        .verify(bytes, signature)
        .map_err(|_| Refused::BadSignature)?;
    let manifest: Manifest =
        serde_json::from_slice(bytes).map_err(|e| Refused::BadManifest(e.to_string()))?;
    if !manifest.files.iter().any(|f| f.name == "rootfs.squashfs") {
        return Err(Refused::BadManifest("no system".into()));
    }
    for f in &manifest.files {
        if !SLOT_FILES.contains(&f.name.as_str()) {
            return Err(Refused::BadManifest(format!(
                "{}: not a slot's file",
                f.name
            )));
        }
        if unhex(&f.sha256).is_none_or(|h| h.len() != 32) {
            return Err(Refused::BadManifest(format!("{}: not a SHA-256", f.name)));
        }
    }
    Ok(manifest)
}

/// Whether a release may replace what runs: strictly newer, and not one that already failed here.
pub fn wanted(version: u64, running: u64, refused: &[u64]) -> bool {
    version > running && !refused.contains(&version)
}

pub fn sha256_of(path: &std::path::Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut context = Context::new(&SHA256);
    let mut buffer = vec![0; 1 << 16];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            return Ok(hex(context.finish().as_ref()));
        }
        context.update(&buffer[..read]);
    }
}

/// `sha256sum -c`'s format, what the initramfs checks at every boot.
pub fn sums(files: &[File]) -> String {
    files
        .iter()
        .map(|f| format!("{}  {}\n", f.sha256, f.name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    fn pair() -> Ed25519KeyPair {
        let doc = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        Ed25519KeyPair::from_pkcs8(doc.as_ref()).unwrap()
    }

    fn manifest(name: &str, sha: &str) -> Vec<u8> {
        serde_json::to_vec(&Manifest {
            version: 7,
            files: vec![File {
                name: name.into(),
                sha256: sha.into(),
                size: 1,
            }],
        })
        .unwrap()
    }

    #[test]
    fn a_signed_manifest_is_read_and_any_other_refused() {
        let key = pair();
        let public = hex(key.public_key().as_ref());
        let bytes = manifest("rootfs.squashfs", &"ab".repeat(32));
        let signature = key.sign(&bytes);
        assert_eq!(
            verified(&bytes, signature.as_ref(), &public)
                .unwrap()
                .version,
            7
        );

        let mut changed = bytes.clone();
        changed[12] ^= 1;
        assert_eq!(
            verified(&changed, signature.as_ref(), &public),
            Err(Refused::BadSignature)
        );
        let stranger = hex(pair().public_key().as_ref());
        assert_eq!(
            verified(&bytes, signature.as_ref(), &stranger),
            Err(Refused::BadSignature)
        );
        assert_eq!(
            verified(&bytes, signature.as_ref(), "abcd"),
            Err(Refused::BadKey)
        );
    }

    #[test]
    fn a_name_from_the_network_never_becomes_any_path() {
        let key = pair();
        let public = hex(key.public_key().as_ref());
        for name in [
            "../../etc/passwd",
            "sums",
            "/etc/shadow",
            "rootfs.squashfs/x",
        ] {
            let bytes = manifest(name, &"ab".repeat(32));
            let signature = key.sign(&bytes);
            assert!(
                matches!(
                    verified(&bytes, signature.as_ref(), &public),
                    Err(Refused::BadManifest(_))
                ),
                "{name}"
            );
        }
        let bytes = manifest("rootfs.squashfs", "short");
        let signature = key.sign(&bytes);
        assert!(matches!(
            verified(&bytes, signature.as_ref(), &public),
            Err(Refused::BadManifest(_))
        ));
    }

    #[test]
    fn a_release_must_be_newer_and_not_one_that_failed() {
        assert!(wanted(8, 7, &[]));
        assert!(!wanted(7, 7, &[]));
        assert!(!wanted(6, 7, &[]));
        assert!(!wanted(8, 7, &[8]));
    }

    #[test]
    fn sums_read_as_sha256sum_writes_them() {
        let files = [File {
            name: "zImage".into(),
            sha256: "cd".repeat(32),
            size: 2,
        }];
        assert_eq!(sums(&files), format!("{}  zImage\n", "cd".repeat(32)));
    }
}
