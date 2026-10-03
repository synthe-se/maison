//! State files, said once: every JSON file the backend keeps is read and written here.
//!
//! - Reading: a missing file is the default; an empty or unparsable one is an error (a
//!   torn write must not silently become « nothing », e.g. nobody able to sign in). A pure
//!   cache may ask for [`Corrupt::Reset`]: the bad file is kept aside as `.corrupt` and
//!   the default is used, loudly.
//! - Writing: a unique temp file created 0600 (or 0644), written, `fsync`ed, renamed over
//!   the old one, then the directory `fsync`ed: a power cut on the Pi's SD card leaves
//!   either the old file or the new one. The temp file goes on error. The replaced file's
//!   owner is kept (or the directory's, for a first one): root's CLI writes files the
//!   service must still be able to write.
//! - Several processes (the service and `maison-backend invite`) may edit one file:
//!   [`locked`] holds an OS lock on `<file>.lock` across the read-modify-write.

use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Serialize, de::DeserializeOwned};

use crate::error::AppError;

/// What a corrupt file means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corrupt {
    /// An error: the file holds state that cannot be rebuilt (people, names, codes).
    Fail,
    /// A cache: kept aside as `<file>.corrupt`, logged, and the default used.
    Reset,
}

/// Who may read the file besides its owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// 0600: secrets and personal data.
    Private,
    /// 0644: everything else.
    Shared,
}

/// The file's value; missing is `T::default()`, empty or unparsable follows `corrupt`.
pub fn read_json<T: DeserializeOwned + Default>(path: &Path, corrupt: Corrupt) -> Result<T, AppError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(T::default()),
        Err(error) => return Err(error.into()),
    };
    let parsed = if text.trim().is_empty() {
        Err(format!("{} is empty", path.display()))
    } else {
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    };
    match (parsed, corrupt) {
        (Ok(value), _) => Ok(value),
        (Err(why), Corrupt::Fail) => Err(AppError::http(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("unreadable state file: {why}"),
        )),
        (Err(why), Corrupt::Reset) => {
            let aside = sibling(path, "corrupt");
            tracing::warn!(%why, aside = %aside.display(), "corrupt cache file: kept aside, starting empty");
            let _ = fs::rename(path, &aside);
            Ok(T::default())
        }
    }
}

/// Writes `value` as pretty JSON with a final newline, atomically (see the module notes).
pub fn write_json<T: Serialize + ?Sized>(path: &Path, value: &T, access: Access) -> Result<(), AppError> {
    let text = format!("{}\n", serde_json::to_string_pretty(value)?);
    write_bytes(path, text.as_bytes(), access)
}

/// Writes `bytes` atomically (see the module notes).
pub fn write_bytes(path: &Path, bytes: &[u8], access: Access) -> Result<(), AppError> {
    let dir = parent(path);
    fs::create_dir_all(dir)?;
    let tmp = sibling(path, &format!("{}.tmp", crate::util::random_secret()[..12].to_owned()));
    let result = (|| -> std::io::Result<()> {
        let mut file = create_new(&tmp, access)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        keep_owner(&tmp, path, dir)?;
        fs::rename(&tmp, path)?;
        // the rename itself must survive a power cut
        if let Ok(d) = File::open(dir) {
            let _ = d.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    Ok(result?)
}

/// A private directory (0700), created if missing.
pub fn private_dir(path: &Path) -> Result<(), AppError> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// A secret kept in a file (a private key): read it, or make it with `generate` and keep it
/// (0600). A file that `decode` refuses is logged and replaced, not a dead end forever.
pub fn load_or_create_secret<T>(
    path: &Path,
    decode: impl Fn(&[u8]) -> Option<T>,
    generate: impl FnOnce() -> Result<(T, Vec<u8>), AppError>,
) -> Result<T, AppError> {
    match fs::read(path) {
        Ok(bytes) if !bytes.is_empty() => match decode(&bytes) {
            Some(value) => return Ok(value),
            None => tracing::warn!(path = %path.display(), "unreadable secret file: making a new one"),
        },
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let (value, bytes) = generate()?;
    write_bytes(path, &bytes, Access::Private)?;
    Ok(value)
}

/// Runs `f` holding an exclusive OS lock on `<path>.lock` (other processes wait), for a
/// read-modify-write several processes share.
pub fn locked<T>(path: &Path, f: impl FnOnce() -> Result<T, AppError>) -> Result<T, AppError> {
    fs::create_dir_all(parent(path))?;
    let lock_path = sibling(path, "lock");
    let lock = OpenOptions::new().create(true).truncate(false).write(true).open(&lock_path)?;
    keep_owner(&lock_path, path, parent(path))?;
    lock.lock()?;
    let out = f();
    let _ = lock.unlock();
    out
}

fn parent(path: &Path) -> &Path {
    path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."))
}

/// `dir/name.json` → `dir/name.json.<suffix>`.
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(format!(".{suffix}"));
    path.with_file_name(name)
}

fn create_new(path: &Path, access: Access) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(match access {
            Access::Private => 0o600,
            Access::Shared => 0o644,
        });
    }
    #[cfg(not(unix))]
    let _ = access;
    options.open(path)
}

/// Gives `file` the owner of `like` (the file it replaces), or of `dir` when there is none.
/// Only root can change an owner; for anyone else it is already theirs, and this is a no-op.
fn keep_owner(file: &Path, like: &Path, dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let Ok(owner) = fs::metadata(like).or_else(|_| fs::metadata(dir)) else { return Ok(()) };
        let mine = fs::metadata(file)?;
        if (mine.uid(), mine.gid()) != (owner.uid(), owner.gid()) {
            std::os::unix::fs::chown(file, Some(owner.uid()), Some(owner.gid()))?;
        }
    }
    #[cfg(not(unix))]
    let _ = (file, like, dir);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("maison-store-{}", crate::util::random_secret()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn missing_is_default_and_a_write_reads_back() {
        let d = dir();
        let path = d.join("sub").join("x.json");
        assert_eq!(read_json::<Vec<u8>>(&path, Corrupt::Fail).unwrap(), Vec::<u8>::new());
        write_json(&path, &vec![1u8, 2], Access::Shared).unwrap();
        assert_eq!(read_json::<Vec<u8>>(&path, Corrupt::Fail).unwrap(), vec![1, 2]);
        assert!(fs::read_to_string(&path).unwrap().ends_with('\n'));
        let leftovers: Vec<_> = fs::read_dir(path.parent().unwrap()).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name()).collect();
        assert_eq!(leftovers.len(), 1, "no temp file left: {leftovers:?}");
    }

    #[test]
    fn an_empty_or_torn_file_is_an_error_unless_it_is_a_cache() {
        let d = dir();
        let path = d.join("x.json");
        fs::write(&path, "").unwrap();
        assert!(read_json::<Vec<u8>>(&path, Corrupt::Fail).is_err(), "empty is not « nothing »");
        fs::write(&path, "[1, 2").unwrap();
        assert!(read_json::<Vec<u8>>(&path, Corrupt::Fail).is_err());
        assert!(path.exists(), "a state file is left alone");
        assert_eq!(read_json::<Vec<u8>>(&path, Corrupt::Reset).unwrap(), Vec::<u8>::new());
        assert!(!path.exists() && d.join("x.json.corrupt").exists(), "a cache is kept aside");
    }

    #[cfg(unix)]
    #[test]
    fn private_files_are_0600_from_the_start() {
        use std::os::unix::fs::PermissionsExt;
        let path = dir().join("secret.json");
        write_json(&path, &"s", Access::Private).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        write_json(&path, &"t", Access::Private).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn a_stale_temp_file_does_not_block_writes() {
        let d = dir();
        let path = d.join("x.json");
        fs::write(d.join("x.json.tmp"), "left by a crash").unwrap();
        write_json(&path, &1, Access::Shared).unwrap();
        assert_eq!(read_json::<i32>(&path, Corrupt::Fail).unwrap(), 1);
    }

    #[test]
    fn a_bad_secret_is_replaced_and_a_good_one_kept() {
        let path = dir().join("key");
        let decode = |b: &[u8]| (b == b"good").then_some(1);
        fs::write(&path, b"torn").unwrap();
        assert_eq!(load_or_create_secret(&path, decode, || Ok((1, b"good".to_vec()))).unwrap(), 1);
        assert_eq!(fs::read(&path).unwrap(), b"good");
        let never = || -> Result<(i32, Vec<u8>), AppError> { panic!("kept, not regenerated") };
        assert_eq!(load_or_create_secret(&path, decode, never).unwrap(), 1);
    }

    #[test]
    fn locked_edits_from_two_threads_lose_nothing() {
        let path = dir().join("n.json");
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let path = path.clone();
                std::thread::spawn(move || {
                    for _ in 0..10 {
                        locked(&path, || {
                            let n: i32 = read_json(&path, Corrupt::Fail)?;
                            write_json(&path, &(n + 1), Access::Shared)
                        })
                        .unwrap();
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        assert_eq!(read_json::<i32>(&path, Corrupt::Fail).unwrap(), 80);
    }
}
