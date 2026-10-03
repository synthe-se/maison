//! A device's settings file, kept in memory and on disk together: the TV, the Android box
//! and the rabbit each hold one.

use std::path::{Path, PathBuf};

use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::RwLock;

use crate::{
    error::AppError,
    store::{self, Access, Corrupt},
};

pub struct JsonConfig<T> {
    path: PathBuf,
    value: RwLock<T>,
}

impl<T: Clone + Default + Serialize + DeserializeOwned> JsonConfig<T> {
    /// A missing file is the unconfigured default (the shelf offers to fill it in); a torn
    /// one is an error, not a silently forgotten device.
    pub fn load(path: &Path) -> Result<Self, AppError> {
        Ok(Self { path: path.to_path_buf(), value: RwLock::new(store::read_json(path, Corrupt::Fail)?) })
    }

    pub async fn get(&self) -> T {
        self.value.read().await.clone()
    }

    /// Saved first, then kept: a failed write leaves memory as it is on disk.
    pub async fn set(&self, value: T) -> Result<T, AppError> {
        let mut current = self.value.write().await;
        store::write_json(&self.path, &value, Access::Shared)?;
        *current = value.clone();
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    #[test]
    fn a_torn_file_is_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("x.json");
        std::fs::write(&path, "{").expect("written");
        assert!(JsonConfig::<Vec<u8>>::load(&path).is_err());
    }
}
