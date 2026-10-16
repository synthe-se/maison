//! A Zigbee device, said once: what it is ([`ZigbeeDeviceInfo`], kept in `zigbee-lamps.json`),
//! its light ([`Light`]), the driver's live record of it ([`DiscoveredDevice`]) and the
//! snapshot the driver shares with the manager ([`ZigbeeDevice`]).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use tokio::time::Instant;

use super::zcl::COLOR_CONTROL_CLUSTER_ID;
use crate::lamps::LampColor;

/// A 16-bit id as the logs say it (`0x2e34`): node ids, clusters, profiles.
#[derive(Debug, Clone, Copy)]
pub struct Hex(pub u16);

impl std::fmt::Display for Hex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{:04x}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ZigbeeDeviceType {
    /// A controllable light (On/Off, Level Control or Color Control input clusters).
    Lamp,
    /// A remote or dimmer switch (sends On/Off and Level Control through its output clusters).
    Remote,
    /// Not interviewed yet, or nothing we drive.
    #[default]
    Unknown,
}

/// What a device is: address, endpoint, clusters, names and capabilities. The driver learns
/// it; the lamps file keeps it (so a lamp is driven at once after a restart).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZigbeeDeviceInfo {
    /// Read from `ieeeAddress` too (the zigbee2mqtt-era name).
    #[serde(alias = "ieeeAddress")]
    pub eui64: String,
    pub endpoint: Option<u8>,
    #[serde(default)]
    pub input_clusters: Vec<u16>,
    #[serde(default)]
    pub output_clusters: Vec<u16>,
    pub model: Option<String>,
    pub manufacturer: Option<String>,
    #[serde(default)]
    pub supports_brightness: bool,
    #[serde(default)]
    pub supports_temperature: bool,
    /// CIE XY colour (bit 3 of colorCapabilities): a white-ambiance lamp has the Color
    /// Control cluster for its temperature only.
    #[serde(default)]
    pub supports_color: bool,
    /// `lamp`, `remote` or `unknown`; a file from before says `isRemote: true/false`.
    #[serde(default, alias = "isRemote", deserialize_with = "device_type_or_flag")]
    pub device_type: ZigbeeDeviceType,
}

/// A device type as written now (`"remote"`), or as `isRemote` was (`true`: a remote, `false`:
/// a lamp).
fn device_type_or_flag<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<ZigbeeDeviceType, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Kept {
        Flag(bool),
        Type(ZigbeeDeviceType),
    }
    Ok(match Kept::deserialize(deserializer)? {
        Kept::Flag(true) => ZigbeeDeviceType::Remote,
        Kept::Flag(false) => ZigbeeDeviceType::Lamp,
        Kept::Type(device_type) => device_type,
    })
}

impl ZigbeeDeviceInfo {
    pub fn has_input(&self, cluster_id: u16) -> bool {
        self.input_clusters.contains(&cluster_id)
    }

    /// Reads the Color Control cluster (temperature, XY, capabilities).
    pub fn has_color_control_cluster(&self) -> bool {
        self.has_input(COLOR_CONTROL_CLUSTER_ID)
    }

    /// `newer`, keeping the model and manufacturer known here when it has none (a device
    /// re-interviewed has not answered its Basic cluster yet).
    pub fn updated(&self, newer: &ZigbeeDeviceInfo) -> ZigbeeDeviceInfo {
        ZigbeeDeviceInfo {
            model: newer.model.clone().or_else(|| self.model.clone()),
            manufacturer: newer
                .manufacturer
                .clone()
                .or_else(|| self.manufacturer.clone()),
            ..newer.clone()
        }
    }
}

/// A lamp's light as last heard (or as last set).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Light {
    pub is_on: bool,
    /// 0–100.
    pub brightness: u8,
    /// 0 (warm) – 100 (cool).
    pub temperature: Option<u8>,
    pub color: LampColor,
}

/// Whether a lamp reports its light by itself (ZCL attribute reporting, asked at its
/// interview): one that does says it is alive every few minutes, and needs no ping.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Reporting {
    /// Not asked (yet).
    #[default]
    Unknown,
    /// Asked; its answer has not come.
    Asked,
    /// It said yes for its On/Off: a report at least every [`super::zcl::REPORT_MAX_INTERVAL_S`].
    Configured,
    /// It said no (or could not be asked): pinged as before.
    Refused,
}

/// A device as the driver shares it with the manager.
#[derive(Debug, Clone, PartialEq)]
pub struct ZigbeeDevice {
    /// The EUI64's hex digits: the lamp's id in the API.
    pub id: String,
    pub node_id: u16,
    pub info: ZigbeeDeviceInfo,
    pub light: Light,
    pub connected: bool,
    pub reachable: bool,
    /// RFC 3339, to the second: an unchanged device is an unchanged snapshot.
    pub last_seen: Option<String>,
}

/// What the user last asked of a lamp, sent again when it comes back after a power cut.
#[derive(Debug, Clone, Default)]
pub struct Desired {
    pub brightness: Option<u8>,
    pub temperature: Option<u8>,
    pub color: Option<(f32, f32)>,
    /// False once the lamp went out of reach; true again when the restore went through.
    /// Only the unreachable → reachable transition restores, not every check.
    pub applied: bool,
}

impl Desired {
    pub fn is_empty(&self) -> bool {
        self.brightness.is_none() && self.temperature.is_none() && self.color.is_none()
    }
}

/// A device on the network, as the driver tracks it.
#[derive(Debug, Clone)]
pub struct DiscoveredDevice {
    pub node_id: u16,
    pub info: ZigbeeDeviceInfo,
    pub light: Light,
    pub connected: bool,
    pub reachable: bool,
    pub interview_completed: bool,
    pub interview_attempts: u32,
    pub last_seen: Option<Instant>,
    pub desired: Desired,
    /// The last discovery asked of it: a chatty uninterviewed device must not flood the
    /// dongle with discovery round-trips.
    pub last_discovery_at: Option<Instant>,
    /// Failed availability ping cycles in a row: drives the check's backoff; reset by any
    /// answer.
    pub failed_ping_cycles: u32,
    /// Whether it reports by itself (then it is only pinged when its reports stop).
    pub reporting: Reporting,
}

impl DiscoveredDevice {
    /// A device kept from an earlier run: known, not heard yet.
    pub fn known(node_id: u16, info: ZigbeeDeviceInfo) -> Self {
        Self {
            node_id,
            interview_completed: info.endpoint.is_some(),
            info,
            light: Light::default(),
            connected: false,
            reachable: false,
            interview_attempts: 0,
            last_seen: None,
            desired: Desired {
                applied: true,
                ..Desired::default()
            },
            last_discovery_at: None,
            failed_ping_cycles: 0,
            reporting: Reporting::Unknown,
        }
    }

    /// A device that just joined: heard now, to be interviewed. A joining lamp is on.
    pub fn joined(node_id: u16, eui64: String) -> Self {
        Self {
            light: Light {
                is_on: true,
                brightness: 100,
                ..Light::default()
            },
            connected: true,
            reachable: true,
            last_seen: Some(Instant::now()),
            ..Self::known(
                node_id,
                ZigbeeDeviceInfo {
                    eui64,
                    ..ZigbeeDeviceInfo::default()
                },
            )
        }
    }

    /// It answered: it is there.
    pub fn heard(&mut self) {
        self.connected = true;
        self.reachable = true;
        self.last_seen = Some(Instant::now());
        self.failed_ping_cycles = 0;
    }

    /// It stopped answering; says whether that is news (it was reachable until now).
    pub fn lost(&mut self) -> bool {
        let news = self.reachable;
        if news {
            self.reachable = false;
            self.desired.applied = false;
        }
        news
    }
}

#[cfg(test)]
impl DiscoveredDevice {
    /// An interviewed Hue lamp on endpoint 11 (On/Off, Level), on and reachable.
    pub fn test_lamp() -> Self {
        let mut device = Self::known(
            0x2e34,
            ZigbeeDeviceInfo {
                eui64: "4b:8e:c6:08:01:88:17:00".to_string(),
                endpoint: Some(11),
                input_clusters: vec![0, 3, 4, 5, 6, 8],
                output_clusters: vec![25],
                model: Some("LTG002".to_string()),
                manufacturer: Some("Signify Netherlands B.V.".to_string()),
                supports_brightness: true,
                device_type: ZigbeeDeviceType::Lamp,
                ..ZigbeeDeviceInfo::default()
            },
        );
        device.connected = true;
        device.reachable = true;
        device.light = Light {
            is_on: true,
            brightness: 100,
            ..Light::default()
        };
        device
    }
}

impl From<&DiscoveredDevice> for ZigbeeDevice {
    fn from(device: &DiscoveredDevice) -> Self {
        ZigbeeDevice {
            id: normalize_device_id(&device.info.eui64, device.node_id),
            node_id: device.node_id,
            info: device.info.clone(),
            light: device.light.clone(),
            connected: device.connected,
            reachable: device.reachable,
            last_seen: device.last_seen.map(wall_clock),
        }
    }
}

/// When `instant` was, by the wall clock, to the second. Read from the clock each time: the
/// Pi has no RTC and NTP steps its clock after boot.
fn wall_clock(instant: Instant) -> String {
    let at = Utc::now() - chrono::Duration::from_std(instant.elapsed()).unwrap_or_default();
    DateTime::from_timestamp(at.timestamp(), 0)
        .unwrap_or(at)
        .to_rfc3339()
}

/// The id of a device in the API: its EUI64's hex digits, lowercase; the node id when the
/// EUI64 is not known.
pub fn normalize_device_id(eui64: &str, node_id: u16) -> String {
    let normalized = crate::net::address_key(eui64);
    if normalized.is_empty() {
        format!("{node_id:016x}")
    } else {
        normalized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hue_lamp() -> ZigbeeDeviceInfo {
        ZigbeeDeviceInfo {
            eui64: "4b:8e:c6:08:01:88:17:00".into(),
            endpoint: Some(11),
            input_clusters: vec![0, 6, 8, 0x0300],
            output_clusters: vec![25],
            model: Some("LCT015".into()),
            manufacturer: None,
            supports_brightness: true,
            supports_temperature: true,
            supports_color: true,
            device_type: ZigbeeDeviceType::Lamp,
        }
    }

    #[test]
    fn the_file_is_written_camel_case_and_old_files_still_read() {
        let json = serde_json::to_value(hue_lamp()).unwrap();
        assert_eq!(json["eui64"], "4b:8e:c6:08:01:88:17:00");
        assert_eq!(json["inputClusters"], serde_json::json!([0, 6, 8, 768]));
        assert_eq!(json["deviceType"], "lamp");
        assert_eq!(json["supportsColor"], true);
        assert!(crate::util::casing_offences(&json).is_empty(), "{json}");
        let kept: ZigbeeDeviceInfo = serde_json::from_value(json).unwrap();
        assert_eq!(kept, hue_lamp());

        // a record from before: its old names, and no colour field
        let old = serde_json::json!({"ieeeAddress": "aa", "endpoint": 1, "model": null, "manufacturer": null,
            "supportsBrightness": true, "supportsTemperature": false, "colorTempMin": null, "isRemote": true});
        let old: ZigbeeDeviceInfo = serde_json::from_value(old).unwrap();
        assert_eq!(
            (old.eui64.as_str(), old.device_type, old.supports_color),
            ("aa", ZigbeeDeviceType::Remote, false)
        );
        let lamp: ZigbeeDeviceInfo =
            serde_json::from_value(serde_json::json!({"ieeeAddress": "bb", "isRemote": false}))
                .unwrap();
        assert_eq!(lamp.device_type, ZigbeeDeviceType::Lamp);
    }

    #[test]
    fn ids_are_said_in_hex() {
        assert_eq!(Hex(0x2e34).to_string(), "0x2e34");
        assert_eq!(Hex(6).to_string(), "0x0006");
    }

    #[test]
    fn a_reinterview_keeps_the_names_it_has_not_read_yet() {
        let kept = ZigbeeDeviceInfo {
            manufacturer: Some("Signify".into()),
            ..hue_lamp()
        };
        let fresh = ZigbeeDeviceInfo {
            model: None,
            manufacturer: None,
            endpoint: Some(12),
            ..hue_lamp()
        };
        let updated = kept.updated(&fresh);
        assert_eq!(updated.model.as_deref(), Some("LCT015"));
        assert_eq!(updated.manufacturer.as_deref(), Some("Signify"));
        assert_eq!(updated.endpoint, Some(12));
    }

    #[test]
    fn the_snapshot_of_an_unchanged_device_does_not_change() {
        let mut device = DiscoveredDevice::known(0x2e34, hue_lamp());
        device.heard();
        let first = ZigbeeDevice::from(&device);
        assert_eq!(first.id, "4b8ec60801881700");
        assert!(
            first
                .last_seen
                .as_deref()
                .is_some_and(|at| !at.contains('.')),
            "to the second"
        );
        assert_eq!(ZigbeeDevice::from(&device), first);
    }

    #[test]
    fn a_joining_device_is_on_and_awaits_its_interview() {
        let device = DiscoveredDevice::joined(0x1234, "00:17:88:01:08:0c:00:0b".into());
        assert!(device.light.is_on && device.reachable && !device.interview_completed);
        assert_eq!(
            (device.light.brightness, device.info.device_type),
            (100, ZigbeeDeviceType::Unknown)
        );
        assert!(device.desired.applied);
        assert_eq!(normalize_device_id("", 0x1234), "0000000000001234");
    }
}
