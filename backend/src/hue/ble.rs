//! What a Hue lamp says over Bluetooth: its GATT characteristics, how it advertises, and
//! the byte encodings of its light.

use std::future::Future;

use btleplug::{
    api::{Characteristic, Peripheral as _, PeripheralProperties},
    platform::Peripheral,
};
use tokio::time::{Duration, timeout};
use uuid::Uuid;

use crate::{error::AppError, net};

pub(super) const LIGHT_CONTROL_SERVICE: &str = "932c32bd-0000-47a2-835a-a8d455b859dd";
pub(super) const POWER_UUID: &str = "932c32bd-0002-47a2-835a-a8d455b859dd";
pub(super) const BRIGHTNESS_UUID: &str = "932c32bd-0003-47a2-835a-a8d455b859dd";
pub(super) const TEMPERATURE_UUID: &str = "932c32bd-0004-47a2-835a-a8d455b859dd";
pub(super) const CONTROL_UUID: &str = "932c32bd-0007-47a2-835a-a8d455b859dd";
const MODEL_UUID: &str = "00002a24-0000-1000-8000-00805f9b34fb";
const FIRMWARE_UUID: &str = "00002a28-0000-1000-8000-00805f9b34fb";
const MANUFACTURER_UUID: &str = "00002a29-0000-1000-8000-00805f9b34fb";
const DEVICE_NAME_UUID: &str = "97fe6561-0003-4f62-86e9-b71ee2da3d22";
const CONFIG_SERVICE_UUID: &str = "0000fe0f-0000-1000-8000-00805f9b34fb";

const PHILIPS_MANUFACTURER_ID: u16 = 0x0075;
const SIGNIFY_MANUFACTURER_ID: u16 = 0x0105;

/// The lamp's characteristics this backend uses, found after service discovery.
#[derive(Clone, Default)]
pub(super) struct HueCharacteristics {
    pub power: Option<Characteristic>,
    pub brightness: Option<Characteristic>,
    pub temperature: Option<Characteristic>,
    pub control: Option<Characteristic>,
    pub model: Option<Characteristic>,
    pub firmware: Option<Characteristic>,
    pub manufacturer: Option<Characteristic>,
    pub device_name: Option<Characteristic>,
}

impl HueCharacteristics {
    pub fn from_peripheral(peripheral: &Peripheral) -> Self {
        let mut found = Self::default();
        for characteristic in peripheral.characteristics() {
            let slot = match uuid_key(characteristic.uuid) {
                uuid if uuid == uuid_str_key(POWER_UUID) => &mut found.power,
                uuid if uuid == uuid_str_key(BRIGHTNESS_UUID) => &mut found.brightness,
                uuid if uuid == uuid_str_key(TEMPERATURE_UUID) => &mut found.temperature,
                uuid if uuid == uuid_str_key(CONTROL_UUID) => &mut found.control,
                uuid if uuid == uuid_str_key(MODEL_UUID) => &mut found.model,
                uuid if uuid == uuid_str_key(FIRMWARE_UUID) => &mut found.firmware,
                uuid if uuid == uuid_str_key(MANUFACTURER_UUID) => &mut found.manufacturer,
                uuid if uuid == uuid_str_key(DEVICE_NAME_UUID) => &mut found.device_name,
                _ => continue,
            };
            *slot = Some(characteristic);
        }
        found
    }
}

/// One Bluetooth operation, bounded by `limit`: a timeout or a Bluetooth error is the
/// lamp being unavailable (the stack's own error stays in the log).
pub(super) async fn ble<T>(
    op: impl Future<Output = btleplug::Result<T>>,
    what: &str,
    limit: Duration,
) -> Result<T, AppError> {
    timeout(limit, op)
        .await
        .map_err(|_| AppError::service_unavailable(format!("Hue lamp {what} timed out")))?
        .map_err(|error| net::unreachable("Hue lamp", format!("{what}: {error}")))
}

/// A Hue lamp by its services, its maker's id, or failing both its name.
pub(super) fn is_hue_lamp(properties: &PeripheralProperties) -> bool {
    if properties.services.iter().any(|uuid| {
        let uuid = uuid_key(*uuid);
        uuid == uuid_str_key(LIGHT_CONTROL_SERVICE) || uuid == uuid_str_key(CONFIG_SERVICE_UUID)
    }) {
        return true;
    }

    if properties
        .manufacturer_data
        .keys()
        .any(|id| *id == PHILIPS_MANUFACTURER_ID || *id == SIGNIFY_MANUFACTURER_ID)
    {
        return true;
    }

    let name = properties
        .local_name
        .as_deref()
        .or(properties.advertisement_name.as_deref())
        .unwrap_or_default()
        .to_ascii_lowercase();

    ["hue", "philips", "signify", "lwa", "lwv", "ltg", "lct", "lwb", "lca"]
        .iter()
        .any(|needle| name.contains(needle))
}

pub(super) fn parse_brightness(raw_value: u8) -> u8 {
    let clamped = raw_value.clamp(1, 254);
    ((u16::from(clamped) * 100) / 254).max(1) as u8
}

pub(super) fn to_brightness(percentage: u8) -> u8 {
    let clamped = percentage.clamp(1, 100);
    (((u16::from(clamped) * 254) + 50) / 100) as u8
}

pub(super) fn parse_temperature(raw_value: u8) -> u8 {
    let clamped = raw_value.clamp(1, 244);
    (((244_u16.saturating_sub(u16::from(clamped))) * 100) / 243) as u8
}

pub(super) fn to_temperature(percentage: u8) -> u8 {
    let clamped = percentage.clamp(0, 100);
    (244_u16.saturating_sub((u16::from(clamped) * 243) / 100)) as u8
}

/// The combined control characteristic's command: one segment per field set.
pub(super) fn build_control_command(power: Option<bool>, brightness: Option<u8>, temperature: Option<u8>) -> Vec<u8> {
    let mut commands = Vec::new();
    if let Some(power) = power {
        commands.extend_from_slice(&[0x01, 0x01, u8::from(power)]);
    }
    if let Some(brightness) = brightness {
        commands.extend_from_slice(&[0x02, 0x01, brightness.clamp(1, 254)]);
    }
    if let Some(temperature) = temperature {
        commands.extend_from_slice(&[0x03, 0x02, temperature.clamp(1, 244), 0x01]);
    }
    commands
}

pub(super) fn uuid_key(uuid: Uuid) -> String {
    uuid.simple().to_string()
}

pub(super) fn uuid_str_key(uuid: &str) -> String {
    uuid.replace('-', "").to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_conversion_round_trips_reasonably() {
        assert_eq!(parse_brightness(to_brightness(1)), 1);
        assert!(parse_brightness(to_brightness(50)) >= 49);
        assert_eq!(parse_brightness(to_brightness(100)), 100);
    }

    #[test]
    fn temperature_conversion_round_trips_reasonably() {
        assert_eq!(parse_temperature(to_temperature(0)), 0);
        assert!(parse_temperature(to_temperature(50)) <= 50);
        assert!(parse_temperature(to_temperature(100)) >= 99);
    }

    #[test]
    fn combined_control_command_contains_expected_segments() {
        assert_eq!(
            build_control_command(Some(true), Some(10), Some(20)),
            vec![0x01, 0x01, 0x01, 0x02, 0x01, 10, 0x03, 0x02, 20, 0x01]
        );
    }

    #[test]
    fn uuids_compare_whatever_their_spelling() {
        let uuid = Uuid::parse_str(POWER_UUID).unwrap();
        assert_eq!(uuid_key(uuid), uuid_str_key(&POWER_UUID.to_ascii_uppercase()));
    }

    #[test]
    fn hue_lamps_are_told_by_service_maker_or_name() {
        let mut properties = PeripheralProperties::default();
        assert!(!is_hue_lamp(&properties));
        properties.local_name = Some("Hue ambiance lamp".into());
        assert!(is_hue_lamp(&properties));
        let mut by_maker = PeripheralProperties::default();
        by_maker.manufacturer_data.insert(SIGNIFY_MANUFACTURER_ID, vec![]);
        assert!(is_hue_lamp(&by_maker));
        let mut by_service = PeripheralProperties::default();
        by_service.services.push(Uuid::parse_str(LIGHT_CONTROL_SERVICE).unwrap());
        assert!(is_hue_lamp(&by_service));
    }

    /// The client hears that the lamp is unavailable, never the stack's message.
    #[tokio::test]
    async fn a_bluetooth_error_says_nothing_of_the_stack() {
        let failing = async { Err::<(), _>(btleplug::Error::Other("org.bluez.Error.Failed: hci0 busy".into())) };
        let error = ble(failing, "write", Duration::from_secs(1)).await.unwrap_err();
        assert_eq!(error.to_string(), "Hue lamp unreachable");
        let slow = async {
            tokio::time::sleep(Duration::from_secs(5)).await;
            Ok(())
        };
        let error = ble(slow, "write", Duration::from_millis(10)).await.unwrap_err();
        assert_eq!(error.to_string(), "Hue lamp write timed out");
    }
}
