//! What the two lamp families (Hue over Bluetooth, Zigbee) share, said once: the state and
//! counts the API answers with, and the files that keep their names and blacklist.
//! Outside the `bluetooth` feature gate: a build without Bluetooth still answers with them.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::{Mutex, MutexGuard};

use crate::{
    error::AppError,
    store::{self, Access, Corrupt},
};

/// A lamp's light, as the API shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LampState {
    pub is_on: bool,
    /// 0–100.
    pub brightness: u8,
    /// 0 (warm) – 100 (cool).
    pub temperature: Option<u8>,
    pub temperature_min: Option<u8>,
    pub temperature_max: Option<u8>,
    /// Zigbee lamps only (Hue Bluetooth ones have no colour): absent, not null, for Hue.
    #[serde(flatten)]
    pub colour: Option<LampColour>,
}

/// A colour lamp's CIE 1931 point and ZCL colour mode (0 HS, 1 XY, 2 temperature).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LampColour {
    pub color_x: Option<f32>,
    pub color_y: Option<f32>,
    pub color_mode: Option<u8>,
}

/// A Hue Bluetooth lamp, as the API shows it (with or without Bluetooth built in).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HueLampView {
    pub id: String,
    pub name: String,
    pub address: String,
    pub model: Option<String>,
    pub manufacturer: String,
    pub firmware: Option<String>,
    pub connected: bool,
    pub connecting: bool,
    pub reachable: bool,
    pub state: LampState,
    pub last_seen: Option<String>,
}

/// How many lamps there are, and whether the family works at all.
#[derive(Debug, Clone, Serialize)]
pub struct LampStats {
    pub total: usize,
    pub connected: usize,
    pub reachable: usize,
    pub disabled: bool,
    pub message: Option<String>,
}

/// A lamp record kept in a lamps file.
pub trait LampRecord: Serialize + DeserializeOwned + Clone + PartialEq + Send + Sync + 'static {
    /// Its stable id: the file is written in this order, so an unchanged set is an
    /// unchanged file whatever order the in-memory map gives.
    fn id(&self) -> &str;
}

/// A lamp family's two files: the lamps (names, capabilities) and the blacklisted
/// addresses. Both are state (a lost name is lost): a torn file is an error, not a reset.
///
/// Writes go through [`LampStore::lock`]: the caller takes its snapshot *while holding the
/// lock*, so the last write is always the newest state (a rename can never be overwritten
/// by an older copy), and a file is only rewritten when its content changed (the SD card
/// of the Pi counts its writes).
pub struct LampStore<T> {
    lamps_path: PathBuf,
    blacklist_path: PathBuf,
    written: Mutex<Written<T>>,
}

struct Written<T> {
    lamps: Option<Vec<T>>,
    blacklist: Option<Vec<String>>,
}

/// The store, locked for one snapshot-and-write.
pub struct LampStoreGuard<'a, T> {
    store: &'a LampStore<T>,
    written: MutexGuard<'a, Written<T>>,
}

impl<T: LampRecord> LampStore<T> {
    pub fn new(lamps_path: &Path, blacklist_path: &Path) -> Self {
        Self {
            lamps_path: lamps_path.to_path_buf(),
            blacklist_path: blacklist_path.to_path_buf(),
            written: Mutex::new(Written { lamps: None, blacklist: None }),
        }
    }

    /// The kept lamps; they count as written (saving them unchanged writes nothing).
    pub fn load_lamps(&self) -> Result<Vec<T>, AppError> {
        let lamps = sorted(store::read_json::<Vec<T>>(&self.lamps_path, Corrupt::Fail)?);
        self.written.try_lock().expect("a store is loaded before it is shared").lamps = Some(lamps.clone());
        Ok(lamps)
    }

    /// The blacklisted addresses, as kept.
    pub fn load_blacklist(&self) -> Result<HashSet<String>, AppError> {
        let blacklist = store::read_json::<Vec<String>>(&self.blacklist_path, Corrupt::Fail)?;
        let mut kept = blacklist.clone();
        kept.sort();
        self.written.try_lock().expect("a store is loaded before it is shared").blacklist = Some(kept);
        Ok(blacklist.into_iter().collect())
    }

    /// Takes the write lock: snapshot the state after this, then save it.
    pub async fn lock(&self) -> LampStoreGuard<'_, T> {
        LampStoreGuard { store: self, written: self.written.lock().await }
    }
}

impl<T: LampRecord> LampStoreGuard<'_, T> {
    /// Writes the lamps when they differ from the last write; says whether it wrote.
    pub async fn save_lamps(&mut self, lamps: Vec<T>) -> Result<bool, AppError> {
        let lamps = sorted(lamps);
        if self.written.lamps.as_ref() == Some(&lamps) {
            return Ok(false);
        }
        store::write_json_async(&self.store.lamps_path, &lamps, Access::Shared).await?;
        self.written.lamps = Some(lamps);
        Ok(true)
    }

    /// Writes the blacklist when it differs from the last write; says whether it wrote.
    pub async fn save_blacklist(&mut self, blacklist: &HashSet<String>) -> Result<bool, AppError> {
        let mut blacklist = blacklist.iter().cloned().collect::<Vec<_>>();
        blacklist.sort();
        if self.written.blacklist.as_ref() == Some(&blacklist) {
            return Ok(false);
        }
        store::write_json_async(&self.store.blacklist_path, &blacklist, Access::Shared).await?;
        self.written.blacklist = Some(blacklist);
        Ok(true)
    }
}

fn sorted<T: LampRecord>(mut lamps: Vec<T>) -> Vec<T> {
    lamps.sort_by(|a, b| a.id().cmp(b.id()));
    lamps
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Lamp {
        id: String,
        name: String,
    }

    impl LampRecord for Lamp {
        fn id(&self) -> &str {
            &self.id
        }
    }

    fn lamp(id: &str, name: &str) -> Lamp {
        Lamp { id: id.into(), name: name.into() }
    }

    fn paths() -> (PathBuf, PathBuf, tempfile::TempDir) {
        let dir = crate::util::test_dir();
        (dir.path().join("lamps.json"), dir.path().join("blacklist.json"), dir)
    }

    #[tokio::test]
    async fn only_a_change_is_written_whatever_the_order() {
        let (lamps_path, blacklist_path, _dir) = paths();
        let store = LampStore::<Lamp>::new(&lamps_path, &blacklist_path);
        assert!(store.load_lamps().unwrap().is_empty(), "a missing file is no lamp");
        assert!(store.load_blacklist().unwrap().is_empty());

        let mut guard = store.lock().await;
        assert!(guard.save_lamps(vec![lamp("b", "B"), lamp("a", "A")]).await.unwrap());
        assert!(!guard.save_lamps(vec![lamp("a", "A"), lamp("b", "B")]).await.unwrap(), "same set, other order");
        assert!(guard.save_lamps(vec![lamp("a", "A2"), lamp("b", "B")]).await.unwrap());
        let blacklist = HashSet::from(["y".to_string(), "x".to_string()]);
        assert!(guard.save_blacklist(&blacklist).await.unwrap());
        assert!(!guard.save_blacklist(&blacklist).await.unwrap());
        drop(guard);

        let again = LampStore::<Lamp>::new(&lamps_path, &blacklist_path);
        assert_eq!(again.load_lamps().unwrap(), vec![lamp("a", "A2"), lamp("b", "B")]);
        assert_eq!(again.load_blacklist().unwrap(), blacklist);
        let mut guard = again.lock().await;
        assert!(!guard.save_lamps(vec![lamp("b", "B"), lamp("a", "A2")]).await.unwrap(), "loaded counts as written");
    }

    #[test]
    fn a_torn_lamps_file_is_an_error_not_an_empty_house() {
        let (lamps_path, blacklist_path, _dir) = paths();
        std::fs::create_dir_all(lamps_path.parent().unwrap()).unwrap();
        std::fs::write(&lamps_path, "[{\"id\": \"a\"").unwrap();
        std::fs::write(&blacklist_path, "").unwrap();
        let store = LampStore::<Lamp>::new(&lamps_path, &blacklist_path);
        assert!(store.load_lamps().is_err());
        assert!(store.load_blacklist().is_err());
        assert!(lamps_path.exists(), "the names are left for a human to rescue");
    }

    #[test]
    fn hue_state_has_no_colour_fields_and_zigbee_state_has_them() {
        let mut state = LampState {
            is_on: true,
            brightness: 40,
            temperature: None,
            temperature_min: Some(0),
            temperature_max: Some(100),
            colour: None,
        };
        let hue = serde_json::to_value(&state).unwrap();
        assert_eq!(
            hue,
            serde_json::json!({"isOn": true, "brightness": 40, "temperature": null, "temperatureMin": 0, "temperatureMax": 100})
        );
        state.colour = Some(LampColour { color_x: Some(0.5), color_y: None, color_mode: Some(1) });
        let zigbee = serde_json::to_value(&state).unwrap();
        assert_eq!(zigbee["colorX"], 0.5);
        assert!(zigbee["colorY"].is_null() && zigbee.as_object().unwrap().contains_key("colorY"));
        assert_eq!(zigbee["colorMode"], 1);
    }
}
