//! The lamps, Hue over Bluetooth (`/api/hue-lamps`) and Zigbee (`/api/zigbee/lamps`): each
//! family answers the same routes with its own lamp record and one shared state.

use serde::{Deserialize, Serialize};

/// A lamp's light.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LampState {
    pub is_on: bool,
    /// 0–100.
    pub brightness: u8,
    /// 0 (warm) – 100 (cool).
    pub temperature: Option<u8>,
    pub temperature_min: Option<u8>,
    pub temperature_max: Option<u8>,
    /// Zigbee lamps only (Hue Bluetooth ones have no colour): absent, not null, for Hue. Read
    /// back, an absent colour reads as one whose fields are all `None`.
    #[serde(flatten)]
    pub colour: Option<LampColour>,
}

/// A colour lamp's CIE 1931 point and ZCL colour mode (0 HS, 1 XY, 2 temperature).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LampColour {
    pub color_x: Option<f32>,
    pub color_y: Option<f32>,
    pub color_mode: Option<u8>,
}

/// A Hue Bluetooth lamp.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HueLamp {
    pub id: String,
    pub name: String,
    pub address: String,
    pub model: Option<String>,
    pub manufacturer: String,
    pub firmware: Option<String>,
    /// The Bluetooth link is up: the server can drive it now.
    pub connected: bool,
    pub connecting: bool,
    pub reachable: bool,
    pub state: LampState,
    pub last_seen: Option<String>,
}

/// A Zigbee lamp on the server's coordinator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZigbeeLamp {
    pub id: String,
    pub name: String,
    pub address: String,
    pub friendly_name: String,
    pub interview_completed: bool,
    pub model: Option<String>,
    pub manufacturer: String,
    pub connected: bool,
    /// It answered recently.
    pub reachable: bool,
    pub supports_brightness: bool,
    pub supports_temperature: bool,
    pub supports_color: bool,
    pub state: LampState,
    pub last_seen: Option<String>,
}

/// A family's lamps and their counts (`GET /`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LampList<L> {
    pub lamps: Vec<L>,
    pub total: usize,
    pub connected: usize,
    pub reachable: usize,
    pub message: String,
}

/// How many lamps there are, and whether the family works at all (`GET /stats`):
/// `disabled` when the server runs without that radio.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LampStats {
    pub total: usize,
    pub connected: usize,
    pub reachable: usize,
    pub disabled: bool,
    pub message: Option<String>,
}

/// `POST /{id}/power`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PowerRequest {
    pub enabled: bool,
}

/// `POST /{id}/brightness` (0–100).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrightnessRequest {
    pub brightness: u8,
}

/// `POST /api/{hue-lamps,zigbee/lamps}/{id}/temperature`: 0 (warm) to 100 (cool).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemperatureRequest {
    pub temperature: u8,
}

/// A rename, of a lamp or a shutter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameRequest {
    pub name: String,
}

/// `POST /api/zigbee/lamps/{id}/color`: a CIE 1931 point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorRequest {
    pub x: f32,
    pub y: f32,
}

/// `POST /api/zigbee/lamps/{id}/effect`: the effect by its name (`candle`, `blink`…).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectRequest {
    pub effect: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Answer;

    #[test]
    fn a_hue_list_reads_as_the_backend_writes_it() {
        let json = r#"{"success":true,"lamps":[{"id":"a","name":"Salon","address":"AA:BB","model":null,"manufacturer":"Signify","firmware":"1.0","connected":true,"connecting":false,"reachable":true,"state":{"isOn":true,"brightness":80,"temperature":null,"temperatureMin":null,"temperatureMax":null},"lastSeen":null}],"total":1,"connected":1,"reachable":1,"message":"Hue lamps list retrieved"}"#;
        let l: Answer<LampList<HueLamp>> = serde_json::from_str(json).unwrap();
        let lamp = &l.body.lamps[0];
        assert!(lamp.connected && lamp.state.is_on);
        assert_eq!(lamp.state.brightness, 80);
    }

    #[test]
    fn a_zigbee_list_round_trips() {
        let json = r#"{"success":true,"lamps":[{"id":"z","name":"Bureau","address":"0x1","friendlyName":"Bureau","interviewCompleted":true,"model":"LCT","manufacturer":"Philips","connected":true,"reachable":false,"supportsBrightness":true,"supportsTemperature":true,"supportsColor":true,"state":{"isOn":false,"brightness":10,"temperature":50,"temperatureMin":0,"temperatureMax":100,"colorX":0.3,"colorY":0.3,"colorMode":1},"lastSeen":"2026-10-16T08:00:00Z"}],"total":1,"connected":1,"reachable":0,"message":"Zigbee lamps list retrieved"}"#;
        let l: Answer<LampList<ZigbeeLamp>> = serde_json::from_str(json).unwrap();
        assert_eq!(
            l.body.lamps[0].state.colour.as_ref().unwrap().color_mode,
            Some(1)
        );
        assert_eq!(serde_json::to_string(&l).unwrap(), json);
    }

    #[test]
    fn stats_and_bodies() {
        let s: Answer<LampStats> = serde_json::from_str(
            r#"{"success":true,"total":0,"connected":0,"reachable":0,"disabled":true,"message":"no Bluetooth"}"#,
        )
        .unwrap();
        assert!(s.body.disabled);
        assert_eq!(
            serde_json::to_string(&PowerRequest { enabled: true }).unwrap(),
            r#"{"enabled":true}"#
        );
        assert_eq!(
            serde_json::to_string(&BrightnessRequest { brightness: 5 }).unwrap(),
            r#"{"brightness":5}"#
        );
    }
}
