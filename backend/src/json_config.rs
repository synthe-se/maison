//! A device's settings file, kept in memory and on disk together: the TV, the Android box,
//! the rabbit and the learnt IR codes each hold one.

use std::path::{Path, PathBuf};

use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::RwLock;

use crate::{
    error::AppError,
    store::{self, Access, Corrupt},
};

/// What a settings file may hold, checked where it comes in (loaded or set), once: the
/// code that uses a value never checks it again. Nothing to check is the default.
pub trait Checked: Sized {
    fn checked(self) -> Result<Self, AppError> {
        Ok(self)
    }
}

pub struct JsonConfig<T> {
    path: PathBuf,
    value: RwLock<T>,
}

impl<T: Checked + Clone + Default + Serialize + DeserializeOwned> JsonConfig<T> {
    /// A missing file is the unconfigured default (the shelf offers to fill it in); a torn
    /// one, or one holding what `set` would refuse, is an error, not a silently forgotten
    /// device.
    pub fn load(path: &Path) -> Result<Self, AppError> {
        let value = store::read_json::<T>(path, Corrupt::Fail)?.checked()?;
        Ok(Self { path: path.to_path_buf(), value: RwLock::new(value) })
    }

    pub async fn get(&self) -> T {
        self.value.read().await.clone()
    }

    /// A look at the value without copying it.
    pub async fn read<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&*self.value.read().await)
    }

    /// Checked, saved, then kept: a failed write leaves memory as it is on disk.
    pub async fn set(&self, value: T) -> Result<T, AppError> {
        self.update(|current| {
            *current = value;
            Ok(())
        })
        .await?;
        Ok(self.get().await)
    }

    /// A change made on a copy, checked and saved, then swapped in, all under one lock:
    /// two changes at once are saved in the order they are kept, and a failed write (or a
    /// refused change) leaves memory as it is on disk.
    pub async fn update<R>(&self, change: impl FnOnce(&mut T) -> Result<R, AppError>) -> Result<R, AppError> {
        let mut current = self.value.write().await;
        let mut next = current.clone();
        let answer = change(&mut next)?;
        let next = next.checked()?;
        store::write_json_async(&self.path, &next, Access::Shared).await?;
        *current = next;
        Ok(answer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl Checked for Vec<u8> {
        fn checked(self) -> Result<Self, AppError> {
            if self.contains(&0) { Err(AppError::bad_request("no zero")) } else { Ok(self) }
        }
    }

    #[tokio::test]
    async fn missing_is_default_set_persists_and_a_failed_save_changes_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("x.json");
        let config = JsonConfig::<Vec<u8>>::load(&path).expect("loads");
        assert!(config.get().await.is_empty());
        config.set(vec![1]).await.expect("saved");
        assert_eq!(JsonConfig::<Vec<u8>>::load(&path).expect("reloads").get().await, vec![1]);

        // a directory where the file should be: the write fails
        let blocked_path = dir.path().join("blocked.json");
        let blocked = JsonConfig::<Vec<u8>>::load(&blocked_path).expect("missing is fine");
        std::fs::create_dir(&blocked_path).expect("dir");
        assert!(blocked.set(vec![2]).await.is_err());
        assert!(blocked.get().await.is_empty(), "memory follows the disk");
        assert!(blocked.update(|v| { v.push(3); Ok(()) }).await.is_err());
        assert!(blocked.get().await.is_empty(), "memory follows the disk");
    }

    #[tokio::test]
    async fn the_check_runs_on_set_update_and_load() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("x.json");
        let config = JsonConfig::<Vec<u8>>::load(&path).expect("loads");
        assert!(config.set(vec![0]).await.is_err());
        assert!(config.update(|v| { v.push(0); Ok(()) }).await.is_err());
        assert!(config.get().await.is_empty());
        assert!(!path.exists(), "nothing refused reaches the disk");
        std::fs::write(&path, "[0]").expect("written");
        assert!(JsonConfig::<Vec<u8>>::load(&path).is_err(), "a hand-edited file is checked too");
    }

    /// Concurrent changes all land, and the file holds the last one kept.
    #[tokio::test]
    async fn concurrent_updates_are_never_lost() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("x.json");
        let config = std::sync::Arc::new(JsonConfig::<Vec<u8>>::load(&path).expect("loads"));
        let tasks: Vec<_> = (1..=20u8)
            .map(|n| {
                let config = config.clone();
                tokio::spawn(async move { config.update(|v| { v.push(n); Ok(()) }).await })
            })
            .collect();
        for task in tasks {
            task.await.expect("joined").expect("saved");
        }
        let mut kept = config.get().await;
        let on_disk = JsonConfig::<Vec<u8>>::load(&path).expect("reloads").get().await;
        assert_eq!(on_disk, kept, "the disk holds what memory holds");
        kept.sort();
        assert_eq!(kept, (1..=20).collect::<Vec<_>>());
    }

    #[test]
    fn a_torn_file_is_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("x.json");
        std::fs::write(&path, "{").expect("written");
        assert!(JsonConfig::<Vec<u8>>::load(&path).is_err());
    }
}
