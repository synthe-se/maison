//! The Meross plugs (`/api/meross`): the list, a plug's live electricity, turning one on or off.

use serde::{Deserialize, Serialize};

use crate::DeviceRef;

/// A plug as the list says it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plug {
    pub id: String,
    pub name: String,
    pub ip: String,
    pub is_online: bool,
    pub is_on: bool,
    /// When the plug last answered (ms since the epoch); 0: never.
    pub last_ping: i64,
}

/// `GET /api/meross`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlugList {
    pub devices: Vec<Plug>,
    pub total: usize,
    pub message: String,
}

/// What the plug reports: tenths of a volt, milliamperes, milliwatts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElectricityRaw {
    pub channel: i32,
    pub current: i32,
    pub voltage: i32,
    pub power: i32,
    pub config: Option<ElectricityConfig>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElectricityConfig {
    pub voltage_ratio: i32,
    pub electricity_ratio: i32,
}

/// The reading in words with its unit (« 42W »), and the raw one numbers come from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Electricity {
    pub voltage: String,
    pub current: String,
    pub power: String,
    pub raw: ElectricityRaw,
}

impl ElectricityRaw {
    /// The voltage in volts.
    pub fn volts(&self) -> f64 {
        f64::from(self.voltage) / 10.0
    }

    /// The current in amperes.
    pub fn amperes(&self) -> f64 {
        f64::from(self.current) / 1000.0
    }

    /// The power in watts.
    pub fn watts(&self) -> f64 {
        f64::from(self.power) / 1000.0
    }

    /// The reading in words with its units (« 230.1V », « 12W »).
    pub fn said(self) -> Electricity {
        /// A whole number without its « .0 ».
        fn unit(value: f64, suffix: &str) -> String {
            if value.fract() == 0.0 {
                format!("{}{suffix}", value.trunc() as i64)
            } else {
                format!("{value}{suffix}")
            }
        }
        Electricity {
            voltage: unit(self.volts(), "V"),
            current: unit(self.amperes(), "A"),
            power: unit(self.watts(), "W"),
            raw: self,
        }
    }
}

/// `GET /api/meross/{id}/electricity`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElectricityAnswer {
    pub device: DeviceRef,
    pub message: String,
    pub electricity: Electricity,
}

/// `POST /api/meross/{id}/toggle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToggleRequest {
    pub on: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Answer;

    #[test]
    fn the_list_and_a_reading_round_trip() {
        let list = r#"{"success":true,"devices":[{"id":"p1","name":"Bureau","ip":"192.168.1.20","isOnline":true,"isOn":false,"lastPing":1760000000000}],"total":1,"message":"Meross devices list retrieved"}"#;
        let l: Answer<PlugList> = serde_json::from_str(list).unwrap();
        assert!(l.body.devices[0].is_online);
        assert_eq!(serde_json::to_string(&l).unwrap(), list);
        let reading = r#"{"success":true,"device":{"id":"p1","name":"Bureau"},"message":"Electricity data retrieved","electricity":{"voltage":"230.1V","current":"0.183A","power":"42W","raw":{"channel":0,"current":183,"voltage":2301,"power":42000,"config":{"voltageRatio":188,"electricityRatio":100}}}}"#;
        let e: Answer<ElectricityAnswer> = serde_json::from_str(reading).unwrap();
        assert_eq!(e.body.electricity.raw.watts(), 42.0);
        assert_eq!(serde_json::to_string(&e).unwrap(), reading);
        let said = e.body.electricity.raw.clone().said();
        assert_eq!(said, e.body.electricity, "the words are the raw reading's");
        let whole = ElectricityRaw {
            channel: 0,
            current: 1500,
            voltage: 2300,
            power: 12000,
            config: None,
        };
        assert_eq!(
            (whole.volts(), whole.amperes(), whole.watts()),
            (230.0, 1.5, 12.0)
        );
        let said = whole.said();
        assert_eq!(
            (
                said.voltage.as_str(),
                said.current.as_str(),
                said.power.as_str()
            ),
            ("230V", "1.5A", "12W")
        );
        assert_eq!(
            serde_json::to_string(&ToggleRequest { on: true }).unwrap(),
            r#"{"on":true}"#
        );
    }
}
