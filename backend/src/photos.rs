//! The wall frame's photos: JPEGs the browser already resized, kept as they came in a private
//! directory (`photos/`, 0700, each file 0600 through `store.rs`), named by the SHA-256 of their
//! bytes. The directory is the only index: there is nothing to fall out of step with it. This
//! Pi never decodes a picture: what comes in is checked by its first and last bytes and its
//! size, no more. A change is said on the event stream (`subscribe`).

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use axum::body::Bytes;
use maison_api::photos::{is_id, Photo, MAX_PHOTOS, MAX_PHOTO_BYTES};
use sha2::{Digest, Sha256};
use tokio::sync::{watch, Mutex};

use crate::{
    error::{AppError, Refusal},
    store::{self, Access},
    util,
};

const EXTENSION: &str = ".jpg";
/// A JPEG's first bytes: start of image, then a marker.
const JPEG_START: [u8; 3] = [0xff, 0xd8, 0xff];
/// Its last bytes: end of image. A canvas always writes them: without them, a torn upload.
const JPEG_END: [u8; 2] = [0xff, 0xd9];

/// Whether `bytes` start and end as a JPEG does; what is between is the frame's to read.
fn is_jpeg(bytes: &[u8]) -> bool {
    bytes.starts_with(&JPEG_START) && bytes.ends_with(&JPEG_END)
}

fn file_of(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}{EXTENSION}"))
}

/// What the directory holds, the latest first (a temp file left by a power cut is no photo).
fn scan(dir: &Path) -> Result<Vec<Photo>, AppError> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let id = name.to_str().and_then(|n| n.strip_suffix(EXTENSION));
        let Some(id) = id.filter(|id| is_id(id)) else {
            continue;
        };
        // removed since the listing: gone
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let photo = Photo {
            id: id.to_string(),
            bytes: meta.len(),
        };
        found.push((meta.modified()?, photo));
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
    Ok(found.into_iter().map(|(_, photo)| photo).collect())
}

#[derive(Clone)]
pub struct PhotosManager {
    dir: Arc<PathBuf>,
    /// One change at a time: the limit is counted and the file written as one step.
    changing: Arc<Mutex<()>>,
    changed: Arc<watch::Sender<u64>>,
}

impl PhotosManager {
    /// The directory is made by the first photo: a house without any has none.
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: Arc::new(dir.to_path_buf()),
            changing: Arc::default(),
            changed: Arc::new(watch::channel(0).0),
        }
    }

    /// A change of the list, for the event stream.
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.changed.subscribe()
    }

    fn said(&self) {
        self.changed.send_modify(|n| *n += 1);
    }

    pub async fn list(&self) -> Result<Vec<Photo>, AppError> {
        let dir = self.dir.clone();
        tokio::task::spawn_blocking(move || scan(&dir)).await?
    }

    /// A photo's bytes; an id that is none is simply not found.
    pub async fn read(&self, id: &str) -> Result<Vec<u8>, AppError> {
        if !is_id(id) {
            return Err(Refusal::NotFound.into());
        }
        let file = file_of(&self.dir, id);
        match tokio::task::spawn_blocking(move || fs::read(file)).await? {
            Ok(bytes) => Ok(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Err(Refusal::NotFound.into())
            }
            Err(error) => Err(error.into()),
        }
    }

    /// Keeps `bytes`; the same bytes again are the photo already kept. Hashing and the fsyncs
    /// run off the runtime: the Pi has one core.
    pub async fn add(&self, bytes: Bytes) -> Result<Photo, AppError> {
        if bytes.len() > MAX_PHOTO_BYTES {
            return Err(Refusal::TooLarge.into());
        }
        if !is_jpeg(&bytes) {
            return Err(Refusal::NotAPhoto.into());
        }
        let _one = self.changing.lock().await;
        let dir = self.dir.clone();
        let (photo, new) = tokio::task::spawn_blocking(move || {
            let photo = Photo {
                id: util::hex(&Sha256::digest(&bytes), ""),
                bytes: bytes.len() as u64,
            };
            let file = file_of(&dir, &photo.id);
            if file.exists() {
                return Ok((photo, false));
            }
            if scan(&dir)?.len() >= MAX_PHOTOS {
                return Err(AppError::from(Refusal::TooManyPhotos));
            }
            store::private_dir(&dir)?;
            store::write_bytes(&file, &bytes, Access::Private)?;
            Ok((photo, true))
        })
        .await??;
        if new {
            self.said();
        }
        Ok(photo)
    }

    /// Removes a photo; false when there is none.
    pub async fn remove(&self, id: &str) -> Result<bool, AppError> {
        if !is_id(id) {
            return Ok(false);
        }
        let _one = self.changing.lock().await;
        let file = file_of(&self.dir, id);
        match tokio::task::spawn_blocking(move || fs::remove_file(file)).await? {
            Ok(()) => {
                self.said();
                Ok(true)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::test_dir;

    const JPEG: &[u8] = &[0xff, 0xd8, 0xff, 0xe0, 1, 2, 3, 0xff, 0xd9];

    #[test]
    fn a_jpeg_is_told_by_its_ends() {
        assert!(is_jpeg(JPEG));
        assert!(!is_jpeg(b"\x89PNG\r\n\x1a\n"), "another format");
        assert!(!is_jpeg(&JPEG[..6]), "torn");
        assert!(!is_jpeg(&[]));
    }

    #[tokio::test]
    async fn the_same_bytes_are_one_photo_and_a_change_is_said() {
        let dir = test_dir();
        let photos = PhotosManager::new(&dir.path().join("photos"));
        let mut changes = photos.subscribe();
        assert!(
            photos.list().await.unwrap().is_empty(),
            "no directory yet: none"
        );
        let first = photos.add(Bytes::from_static(JPEG)).await.unwrap();
        assert!(is_id(&first.id));
        assert!(changes.has_changed().unwrap());
        changes.borrow_and_update();
        let again = photos.add(Bytes::from_static(JPEG)).await.unwrap();
        assert_eq!(again, first);
        assert!(!changes.has_changed().unwrap(), "nothing changed");
        assert_eq!(photos.list().await.unwrap(), std::slice::from_ref(&first));
        assert_eq!(photos.read(&first.id).await.unwrap(), JPEG);
        assert!(photos.remove(&first.id).await.unwrap());
        assert!(changes.has_changed().unwrap());
        assert!(!photos.remove(&first.id).await.unwrap());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_familys_photos_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = test_dir();
        let folder = dir.path().join("photos");
        let photos = PhotosManager::new(&folder);
        let photo = photos.add(Bytes::from_static(JPEG)).await.unwrap();
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&folder), 0o700);
        assert_eq!(mode(&file_of(&folder, &photo.id)), 0o600);
    }

    #[tokio::test]
    async fn only_named_photos_are_listed() {
        let dir = test_dir();
        let folder = dir.path().join("photos");
        let photos = PhotosManager::new(&folder);
        let photo = photos.add(Bytes::from_static(JPEG)).await.unwrap();
        fs::write(folder.join(format!("{}.jpg.abc.tmp", photo.id)), b"torn").unwrap();
        fs::write(folder.join("notes.jpg"), b"not ours").unwrap();
        assert_eq!(photos.list().await.unwrap(), [photo]);
    }

    #[tokio::test]
    async fn a_name_that_is_no_id_reaches_no_file() {
        let dir = test_dir();
        fs::write(dir.path().join("secret"), b"x").unwrap();
        let photos = PhotosManager::new(&dir.path().join("photos"));
        assert!(photos.read("../secret").await.is_err());
        assert!(!photos.remove("../secret").await.unwrap());
        assert!(dir.path().join("secret").exists());
    }
}
