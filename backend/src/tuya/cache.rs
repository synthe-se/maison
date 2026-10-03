//! The values a device only reports now and then (the feeder's meal plan, the litter
//! level), kept across restarts in `device-cache.json`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{TuyaDeviceType, TuyaManager, dps};
use crate::store::{self, Access};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct DeviceCache(HashMap<String, HashMap<String, Value>>);

impl DeviceCache {
    /// A device's cached values, as data points.
    pub(super) fn dps(&self, device_id: &str) -> Map<String, Value> {
        self.0
            .get(device_id)
            .map(|values| values.iter().map(|(key, value)| (key.clone(), value.clone())).collect())
            .unwrap_or_default()
    }
}

/// The values worth keeping across restarts: the ones a device only reports now and then.
pub(super) fn cacheable_dps_keys(device_type: TuyaDeviceType) -> &'static [&'static str] {
    match device_type {
        TuyaDeviceType::Feeder => &[dps::feeder::MEAL_PLAN],
        TuyaDeviceType::LitterBox => &[dps::litter::LITTER_LEVEL],
        TuyaDeviceType::Fountain | TuyaDeviceType::Unknown => &[],
    }
}

/// Puts `updates` into `entry`; true when one of them changed it.
fn merge_cache_updates(entry: &mut HashMap<String, Value>, updates: Vec<(String, Value)>) -> bool {
    let mut changed = false;
    for (key, value) in updates {
        if entry.get(&key) != Some(&value) {
            entry.insert(key, value);
            changed = true;
        }
    }
    changed
}

impl TuyaManager {
    /// The cached values `merged` lacks (a status read that missed them).
    pub(super) async fn merge_cached_dps(&self, merged: &mut Map<String, Value>, device_type: TuyaDeviceType, device_id: &str) {
        let cache = self.cache.read().await;
        let Some(cached) = cache.0.get(device_id) else {
            return;
        };
        for dps_id in cacheable_dps_keys(device_type) {
            if let Some(value) = cached.get(*dps_id) {
                merged.entry((*dps_id).to_string()).or_insert_with(|| value.clone());
            }
        }
    }

    pub(super) async fn store_cacheable_dps(&self, device_id: &str, device_type: TuyaDeviceType, dps: &Map<String, Value>) {
        let updates = cacheable_dps_keys(device_type)
            .iter()
            .filter_map(|key| dps.get(*key).cloned().map(|value| ((*key).to_string(), value)))
            .collect::<Vec<_>>();
        self.apply_cache_updates(device_id, updates).await;
    }

    /// Puts `updates` in the cache and rewrites the file only when they changed it: the
    /// devices push their values all day long, the SD card need not hear each one. The
    /// write lock is held across the write, so writes land in order. A failed write is
    /// logged: it is a cache, the devices say it all again.
    pub(super) async fn apply_cache_updates(&self, device_id: &str, updates: Vec<(String, Value)>) {
        if updates.is_empty() {
            return;
        }
        let mut cache = self.cache.write().await;
        let mut next = cache.clone();
        if !merge_cache_updates(next.0.entry(device_id.to_string()).or_default(), updates) {
            return;
        }
        if let Err(error) = store::write_json_async(&self.cache_path, &next, Access::Shared).await {
            tracing::warn!(%error, "tuya device cache not saved");
        }
        *cache = next;
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::tuya::tests::{config, make};

    #[test]
    fn the_cache_changes_only_on_new_values() {
        let mut entry = HashMap::new();
        assert!(merge_cache_updates(&mut entry, vec![("1".into(), json!("plan"))]));
        assert!(!merge_cache_updates(&mut entry, vec![("1".into(), json!("plan"))]));
        assert!(merge_cache_updates(&mut entry, vec![("1".into(), json!("plan")), ("112".into(), json!("full"))]));
        assert_eq!(entry.len(), 2);
    }

    #[test]
    fn cached_keys_are_per_device_type() {
        assert_eq!(cacheable_dps_keys(TuyaDeviceType::Feeder), &["1"]);
        assert_eq!(cacheable_dps_keys(TuyaDeviceType::LitterBox), &["112"]);
        assert!(cacheable_dps_keys(TuyaDeviceType::Fountain).is_empty());
    }

    #[tokio::test]
    async fn the_cache_file_is_written_only_when_it_changes() {
        let (manager, root) = make(json!([config("f", "Feeder", "")]), None);
        let path = root.path().join("device-cache.json");
        manager.apply_cache_updates("f", vec![("1".into(), json!("plan"))]).await;
        let written: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written, json!({ "f": { "1": "plan" } }));

        // The same value again: the file is left alone.
        std::fs::remove_file(&path).unwrap();
        manager.apply_cache_updates("f", vec![("1".into(), json!("plan"))]).await;
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn the_cache_seeds_the_listed_values_and_a_corrupt_one_is_reset() {
        let (manager, _dir) = make(json!([config("f", "Feeder", "")]), Some(r#"{"f":{"1":"BQgeAgE="}}"#));
        let listed = manager.list_devices().await;
        assert_eq!(listed[0].last_data.dps.get("1"), Some(&json!("BQgeAgE=")));
        assert_eq!(listed[0].device_type, TuyaDeviceType::Feeder);

        let (manager, root) = make(json!([config("f", "Feeder", "")]), Some("{torn"));
        assert!(manager.list_devices().await[0].last_data.dps.is_empty());
        assert!(root.path().join("device-cache.json.corrupt").exists());
    }
}
