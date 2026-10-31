//! On the build machine: `keygen <key file> <public file>` makes the release key, `sign <key file>
//! <manifest>` writes `<manifest's dir>/manifest.sig`, and `describe <version> <dir>` writes
//! `<dir>/manifest.json` from the files there. The key file stays off the repository.

use maison_frame_tools::{File, Manifest, SLOT_FILES, hex, sha256_of};
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use std::path::Path;
use std::{env, fs, process};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let done = match args.as_slice() {
        ["keygen", key, public] => keygen(Path::new(key), Path::new(public)),
        ["sign", key, manifest] => sign(Path::new(key), Path::new(manifest)),
        ["describe", version, dir] => describe(version, Path::new(dir)),
        _ => Err("usage: maison-sign keygen <key> <public> | sign <key> <manifest> | describe <version> <dir>".to_string()),
    };
    if let Err(why) = done {
        eprintln!("{why}");
        process::exit(1);
    }
}

fn keygen(key: &Path, public: &Path) -> Result<(), String> {
    if key.exists() {
        return Err(format!(
            "{}: there already, a new key would orphan every frame",
            key.display()
        ));
    }
    let doc = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).map_err(|e| e.to_string())?;
    let pair = Ed25519KeyPair::from_pkcs8(doc.as_ref()).map_err(|e| e.to_string())?;
    fs::write(key, doc.as_ref()).map_err(|e| e.to_string())?;
    fs::write(public, hex(pair.public_key().as_ref())).map_err(|e| e.to_string())
}

fn sign(key: &Path, manifest: &Path) -> Result<(), String> {
    let pair = Ed25519KeyPair::from_pkcs8(&fs::read(key).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let signature = pair.sign(&fs::read(manifest).map_err(|e| e.to_string())?);
    fs::write(manifest.with_file_name("manifest.sig"), signature.as_ref())
        .map_err(|e| e.to_string())
}

fn describe(version: &str, dir: &Path) -> Result<(), String> {
    let version = version
        .parse()
        .map_err(|_| format!("{version}: not a number"))?;
    let mut files = Vec::new();
    for name in SLOT_FILES {
        let path = dir.join(name);
        if path.exists() {
            let size = fs::metadata(&path).map_err(|e| e.to_string())?.len();
            files.push(File {
                name: name.into(),
                sha256: sha256_of(&path).map_err(|e| e.to_string())?,
                size,
            });
        }
    }
    let manifest = Manifest { version, files };
    fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
