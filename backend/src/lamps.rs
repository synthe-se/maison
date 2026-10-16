//! What the two lamp families (Hue over Bluetooth, Zigbee) share, said once: the state and
//! counts the API answers with, the rules of a brightness and a temperature and their
//! radio levels, and the files that keep their names and blacklist.
//! Outside the `bluetooth` feature gate: a build without Bluetooth still answers with them.

use std::{
    collections::HashSet,
    future::Future,
    path::{Path, PathBuf},
};

use serde::{de::DeserializeOwned, Serialize};
use tokio::sync::{Mutex, MutexGuard};

use crate::{
    error::{AppError, Refusal},
    net::address_key,
    store::{self, Access, Corrupt},
};

pub use maison_api::lamps::{HueLamp, LampColor, LampState, LampStats, ZigbeeLamp};

/// The old name, kept only for `ir.rs` until it says `HueLamp`.
pub use maison_api::lamps::HueLamp as HueLampView;

/// A lamp of either family, as what reads it needs it.
pub trait Lamp {
    fn state(&self) -> &LampState;
}

impl Lamp for HueLamp {
    fn state(&self) -> &LampState {
        &self.state
    }
}

impl Lamp for ZigbeeLamp {
    fn state(&self) -> &LampState {
        &self.state
    }
}

/// No such lamp, in either family: the coded `not_found`.
pub fn not_found() -> AppError {
    Refusal::NotFound.into()
}

/// A brightness as a lamp takes it, in both families: 1 to 100, 0 read as the dimmest.
/// Switching off is the power's job, never a brightness's.
pub fn brightness(percent: u8) -> u8 {
    percent.clamp(1, 100)
}

/// A colour temperature as a lamp takes it: 0 (warm) to 100 (cool).
pub fn temperature(percent: u8) -> u8 {
    percent.min(100)
}

/// A brightness as the radio's level (ZCL Level Control and Hue's GATT byte alike, 1–254),
/// rounded; `percent` goes through [`brightness`] first.
pub fn level_of(percent: u8) -> u8 {
    ((u16::from(brightness(percent)) * 254 + 50) / 100) as u8
}

/// A radio level (0–254) as a brightness, rounded: 0 only for 0, and back to the same
/// brightness for every level [`level_of`] gives.
pub fn brightness_of(level: u8) -> u8 {
    if level == 0 {
        return 0;
    }
    ((u16::from(level.min(254)) * 100 + 127) / 254).max(1) as u8
}

/// A lamp's light before anything was heard of it: off, at full brightness, no temperature
/// nor colour known.
pub fn unlit() -> LampState {
    LampState {
        is_on: false,
        brightness: 100,
        temperature: None,
        temperature_min: None,
        temperature_max: None,
        color: None,
    }
}

/// A lamp record kept in a lamps file.
pub trait LampRecord:
    Serialize + DeserializeOwned + Clone + PartialEq + Send + Sync + 'static
{
    /// Its stable id: the file is written in this order, so an unchanged set is an
    /// unchanged file whatever order the in-memory map gives.
    fn id(&self) -> &str;
}

/// A lamp family's two files: the lamps (names, capabilities) and the blacklisted
/// addresses. Both are state (a lost name is lost): a torn file is an error, not a reset.
///
/// Writes go through [`LampStore::save`]: the snapshot is taken *while holding the lock*,
/// so the last write is always the newest state (a rename can never be overwritten by an
/// older copy), and a file is only rewritten when its content changed (the SD card of the
/// Pi counts its writes).
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
            written: Mutex::new(Written {
                lamps: None,
                blacklist: None,
            }),
        }
    }

    /// The kept lamps; they count as written (saving them unchanged writes nothing).
    pub fn load_lamps(&self) -> Result<Vec<T>, AppError> {
        let lamps = sorted(store::read_json::<Vec<T>>(&self.lamps_path, Corrupt::Fail)?);
        self.written
            .try_lock()
            .expect("a store is loaded before it is shared")
            .lamps = Some(lamps.clone());
        Ok(lamps)
    }

    /// The blacklisted addresses, as keys ([`address_key`]): « AA:BB:… » kept by hand and
    /// « aabb… » are the same device.
    pub fn load_blacklist(&self) -> Result<HashSet<String>, AppError> {
        let blacklist = store::read_json::<Vec<String>>(&self.blacklist_path, Corrupt::Fail)?;
        let mut kept = blacklist.clone();
        kept.sort();
        self.written
            .try_lock()
            .expect("a store is loaded before it is shared")
            .blacklist = Some(kept);
        Ok(blacklist
            .iter()
            .map(|address| address_key(address))
            .collect())
    }

    /// Saves what `snapshot` says (the lamps, and the blacklist when the family keeps one
    /// that changes), the snapshot taken under the lock: `snapshot` only runs once it is
    /// held, so the newest state is always the one written.
    pub async fn save(
        &self,
        snapshot: impl Future<Output = (Vec<T>, Option<HashSet<String>>)>,
    ) -> Result<(), AppError> {
        let mut guard = self.lock().await;
        let (lamps, blacklist) = snapshot.await;
        guard.save_lamps(lamps).await?;
        if let Some(blacklist) = blacklist {
            guard.save_blacklist(&blacklist).await?;
        }
        Ok(())
    }

    /// Takes the write lock: snapshot the state after this, then save it.
    async fn lock(&self) -> LampStoreGuard<'_, T> {
        LampStoreGuard {
            store: self,
            written: self.written.lock().await,
        }
    }
}

impl<T: LampRecord> LampStoreGuard<'_, T> {
    /// Writes the lamps when they differ from the last write; says whether it wrote.
    async fn save_lamps(&mut self, lamps: Vec<T>) -> Result<bool, AppError> {
        let lamps = sorted(lamps);
        if self.written.lamps.as_ref() == Some(&lamps) {
            return Ok(false);
        }
        store::write_json_async(&self.store.lamps_path, &lamps, Access::Shared).await?;
        self.written.lamps = Some(lamps);
        Ok(true)
    }

    /// Writes the blacklist when it differs from the last write; says whether it wrote.
    async fn save_blacklist(&mut self, blacklist: &HashSet<String>) -> Result<bool, AppError> {
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
        Lamp {
            id: id.into(),
            name: name.into(),
        }
    }

    fn paths() -> (PathBuf, PathBuf, tempfile::TempDir) {
        let dir = crate::util::test_dir();
        (
            dir.path().join("lamps.json"),
            dir.path().join("blacklist.json"),
            dir,
        )
    }

    #[tokio::test]
    async fn only_a_change_is_written_whatever_the_order() {
        let (lamps_path, blacklist_path, _dir) = paths();
        let store = LampStore::<Lamp>::new(&lamps_path, &blacklist_path);
        assert!(
            store.load_lamps().unwrap().is_empty(),
            "a missing file is no lamp"
        );
        assert!(store.load_blacklist().unwrap().is_empty());

        let mut guard = store.lock().await;
        assert!(guard
            .save_lamps(vec![lamp("b", "B"), lamp("a", "A")])
            .await
            .unwrap());
        assert!(
            !guard
                .save_lamps(vec![lamp("a", "A"), lamp("b", "B")])
                .await
                .unwrap(),
            "same set, other order"
        );
        assert!(guard
            .save_lamps(vec![lamp("a", "A2"), lamp("b", "B")])
            .await
            .unwrap());
        let blacklist = HashSet::from(["y".to_string(), "x".to_string()]);
        assert!(guard.save_blacklist(&blacklist).await.unwrap());
        assert!(!guard.save_blacklist(&blacklist).await.unwrap());
        drop(guard);

        let again = LampStore::<Lamp>::new(&lamps_path, &blacklist_path);
        assert_eq!(
            again.load_lamps().unwrap(),
            vec![lamp("a", "A2"), lamp("b", "B")]
        );
        assert_eq!(again.load_blacklist().unwrap(), blacklist);
        let mut guard = again.lock().await;
        assert!(
            !guard
                .save_lamps(vec![lamp("b", "B"), lamp("a", "A2")])
                .await
                .unwrap(),
            "loaded counts as written"
        );
        drop(guard);
        // the snapshot is taken under the lock: what it says when it runs is what is written
        let renamed = std::sync::Mutex::new(vec![lamp("a", "A3")]);
        again
            .save(async { (renamed.lock().unwrap().clone(), None) })
            .await
            .unwrap();
        assert_eq!(again.load_lamps().unwrap(), vec![lamp("a", "A3")]);
    }

    /// A blacklist written by hand, with separators and capitals, keeps the same devices out.
    #[test]
    fn a_kept_blacklist_is_read_as_keys() {
        let (lamps_path, blacklist_path, _dir) = paths();
        store::write_json(
            &blacklist_path,
            &["AA:BB:CC:DD:EE:FF", "00-17-88-01"],
            Access::Shared,
        )
        .unwrap();
        let store = LampStore::<Lamp>::new(&lamps_path, &blacklist_path);
        assert_eq!(
            store.load_blacklist().unwrap(),
            HashSet::from(["aabbccddeeff".to_string(), "00178801".to_string()])
        );
    }

    #[test]
    fn brightness_and_levels_round_trip_with_one_rule() {
        assert_eq!(
            (brightness(0), brightness(50), brightness(200)),
            (1, 50, 100)
        );
        assert_eq!((temperature(0), temperature(101)), (0, 100));
        assert_eq!(
            (level_of(0), level_of(1), level_of(50), level_of(100)),
            (3, 3, 127, 254)
        );
        for percent in 1..=100 {
            assert_eq!(brightness_of(level_of(percent)), percent, "{percent}");
        }
        assert_eq!(
            (brightness_of(0), brightness_of(1), brightness_of(255)),
            (0, 1, 100)
        );
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
        assert!(
            lamps_path.exists(),
            "the names are left for a human to rescue"
        );
    }

    #[test]
    fn hue_state_has_no_colour_fields_and_zigbee_state_has_them() {
        let mut state = LampState {
            is_on: true,
            brightness: 40,
            temperature_min: Some(0),
            temperature_max: Some(100),
            ..unlit()
        };
        let hue = serde_json::to_value(&state).unwrap();
        assert_eq!(
            hue,
            serde_json::json!({"isOn": true, "brightness": 40, "temperature": null, "temperatureMin": 0, "temperatureMax": 100})
        );
        state.color = Some(LampColor {
            color_x: Some(0.5),
            color_y: None,
            color_mode: Some(1),
        });
        let zigbee = serde_json::to_value(&state).unwrap();
        assert_eq!(zigbee["colorX"], 0.5);
        assert!(zigbee["colorY"].is_null() && zigbee.as_object().unwrap().contains_key("colorY"));
        assert_eq!(zigbee["colorMode"], 1);
    }
}
