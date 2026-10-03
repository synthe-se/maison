//! The Hue lamps as kept (`hue-lamps.json`) and how a lamp is recognised from one scan to
//! the next: by its Bluetooth address, or by the platform's peripheral id where the stack
//! hides the address (macOS reports 00:00:00:00:00:00).

use std::collections::HashMap;

use btleplug::{
    api::{Peripheral as _, PeripheralProperties},
    platform::Peripheral,
};
use serde::{Deserialize, Serialize};

use crate::{lamps::LampRecord, net::address_key};

const ZERO_BLE_ADDRESS: &str = "00:00:00:00:00:00";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StoredLampConfig {
    pub id: String,
    pub name: String,
    pub address: String,
    pub model: Option<String>,
    #[serde(default)]
    pub has_connected_once: bool,
    pub temperature_min: Option<u8>,
    pub temperature_max: Option<u8>,
    pub last_temperature: Option<u8>,
}

impl LampRecord for StoredLampConfig {
    fn id(&self) -> &str {
        &self.id
    }
}

fn is_zero_address(address: &str) -> bool {
    address_key(address) == address_key(ZERO_BLE_ADDRESS)
}

/// The key a scanned lamp is kept under: its address, else the peripheral id.
pub(super) fn stable_lamp_id(peripheral: &Peripheral, properties: &PeripheralProperties) -> String {
    address_key(&lamp_display_address(peripheral, properties))
}

/// The address shown for a scanned lamp: its own, else the peripheral id.
pub(super) fn lamp_display_address(peripheral: &Peripheral, properties: &PeripheralProperties) -> String {
    let address = properties.address.to_string().trim().to_ascii_lowercase();
    if is_zero_address(&address) { peripheral.id().to_string() } else { address }
}

/// Whether a kept lamp is the one a scan found.
pub(super) fn lamp_matches_scan(
    lamp: &StoredLampConfig,
    stable_id: &str,
    display_address: &str,
    discovered_name: &str,
) -> bool {
    let lamp_id = address_key(&lamp.id);
    let lamp_address = address_key(&lamp.address);
    lamp_id == stable_id
        || lamp_address == stable_id
        || lamp_address == address_key(display_address)
        || ((is_zero_address(&lamp.id) || is_zero_address(&lamp.address)) && lamp.name == discovered_name)
}

/// The kept lamps, one per device: older files may hold a lamp twice (once by address,
/// once by peripheral id); the most complete record wins.
pub(super) fn dedupe_stored_lamps(configs: Vec<StoredLampConfig>) -> Vec<StoredLampConfig> {
    let mut deduped: HashMap<String, StoredLampConfig> = HashMap::new();
    for config in configs {
        let canonical = canonical_stored_identity(&config);
        let normalized = normalize_stored_config(config, &canonical);
        match deduped.get(&canonical) {
            Some(existing) if stored_config_score(&normalized) <= stored_config_score(existing) => {}
            _ => {
                deduped.insert(canonical, normalized);
            }
        }
    }
    deduped.into_values().collect()
}

fn canonical_stored_identity(config: &StoredLampConfig) -> String {
    let id = address_key(&config.id);
    if !id.is_empty() && !is_zero_address(&config.id) { id } else { address_key(&config.address) }
}

fn normalize_stored_config(mut config: StoredLampConfig, canonical: &str) -> StoredLampConfig {
    config.id = canonical.to_string();
    if address_key(&config.address).is_empty() || is_zero_address(&config.address) {
        config.address = canonical.to_string();
    }
    config
}

fn stored_config_score(config: &StoredLampConfig) -> usize {
    usize::from(config.has_connected_once) * 10
        + usize::from(config.model.is_some()) * 5
        + usize::from(config.temperature_min.is_some())
        + usize::from(config.temperature_max.is_some())
        + usize::from(config.last_temperature.is_some())
}

/// A name for a lamp that advertises none: « Hue Lamp » and the end of its address.
pub(super) fn fallback_lamp_name(address: &str) -> String {
    let suffix = address.chars().rev().take(5).collect::<String>();
    format!("Hue Lamp {}", suffix.chars().rev().collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lamp(id: &str, address: &str) -> StoredLampConfig {
        StoredLampConfig { id: id.into(), name: "Lamp".into(), address: address.into(), ..Default::default() }
    }

    #[test]
    fn a_lamp_kept_twice_is_kept_once_the_most_complete() {
        let bare = lamp("AA:BB:CC:DD:EE:FF", "aa:bb:cc:dd:ee:ff");
        let known = StoredLampConfig { has_connected_once: true, ..lamp("aabbccddeeff", "aa:bb:cc:dd:ee:ff") };
        let kept = dedupe_stored_lamps(vec![bare.clone(), known.clone(), bare]);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].id, "aabbccddeeff");
        assert!(kept[0].has_connected_once);
    }

    #[test]
    fn a_hidden_address_falls_back_to_the_peripheral_id() {
        let kept = dedupe_stored_lamps(vec![lamp(ZERO_BLE_ADDRESS, "1234-ABCD")]);
        assert_eq!(kept[0].id, "1234abcd");
        let kept = dedupe_stored_lamps(vec![lamp("1234-abcd", ZERO_BLE_ADDRESS)]);
        assert_eq!(kept[0].address, "1234abcd");
    }

    #[test]
    fn a_scan_matches_by_key_address_or_hidden_name() {
        let kept = lamp("aabbccddeeff", "aa:bb:cc:dd:ee:ff");
        assert!(lamp_matches_scan(&kept, "aabbccddeeff", "x", "Other"));
        assert!(lamp_matches_scan(&kept, "zz", "AA-BB-CC-DD-EE-FF", "Other"));
        assert!(!lamp_matches_scan(&kept, "zz", "zz", "Lamp"));
        let hidden = lamp(ZERO_BLE_ADDRESS, ZERO_BLE_ADDRESS);
        assert!(lamp_matches_scan(&hidden, "zz", "zz", "Lamp"));
        assert!(!lamp_matches_scan(&hidden, "zz", "zz", "Other"));
    }

    #[test]
    fn a_nameless_lamp_is_named_after_its_address() {
        assert_eq!(fallback_lamp_name("aa:bb:cc:dd:ee:ff"), "Hue Lamp ee:ff");
    }
}
