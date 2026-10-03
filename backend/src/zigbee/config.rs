//! The Zigbee radio's settings (`ZIGBEE_*`), read once with the rest of [`crate::config::Config`].
//! A value that does not parse is logged and its default used: a typo must not leave the
//! radio silently on another channel or PAN than the one asked for.

use std::num::NonZero;

use tracing::warn;

/// The dongle's protocol. Only Silicon Labs' EmberZNet (EZSP over ASH) is spoken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Adapter {
    #[default]
    Ember,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZigbeeConfig {
    /// `ZIGBEE_ADAPTER` (`ember`).
    pub adapter: Adapter,
    /// `ZIGBEE_SERIAL_PORT`; unset, the radio stays off.
    pub serial_port: Option<String>,
    /// `ZIGBEE_EZSP_PROTOCOL_VERSION`: 13 for EmberZNet 7.4 (the MG21 dongle). On a mismatch
    /// the driver retries once with the version the dongle announces.
    pub protocol_version: NonZero<u8>,
    /// `ZIGBEE_CHANNEL` (11–26), used when a network is formed.
    pub channel: u8,
    /// `ZIGBEE_TX_POWER`, in dBm (negative values are legitimate).
    pub tx_power: i8,
    /// `ZIGBEE_PAN_ID` (decimal or `0x…`); unset, the dongle draws one.
    pub pan_id: Option<u16>,
    /// `ZIGBEE_EXTENDED_PAN_ID` (16 hex digits, separators ignored); unset, derived from the
    /// coordinator's EUI64.
    pub extended_pan_id: Option<[u8; 8]>,
    /// `ZIGBEE_PERMIT_JOIN_SECONDS` (1–254): how long pairing stays open.
    pub permit_join_seconds: u16,
}

const DEFAULT_PROTOCOL_VERSION: NonZero<u8> = NonZero::new(13).expect("13 is not zero");

impl Default for ZigbeeConfig {
    fn default() -> Self {
        Self {
            adapter: Adapter::Ember,
            serial_port: None,
            protocol_version: DEFAULT_PROTOCOL_VERSION,
            channel: 11,
            tx_power: 8,
            pan_id: None,
            extended_pan_id: None,
            permit_join_seconds: 120,
        }
    }
}

impl ZigbeeConfig {
    /// The settings `var` gives, defaults for the rest; each refused value is logged.
    pub fn load(var: impl Fn(&str) -> Option<String>) -> Self {
        let (config, refused) = Self::parse(var);
        for warning in refused {
            warn!("{warning}");
        }
        config
    }

    /// The settings and what was refused, one line per refused value.
    fn parse(var: impl Fn(&str) -> Option<String>) -> (Self, Vec<String>) {
        let mut refused = Vec::new();
        let defaults = Self::default();
        let mut setting = |name: &str, expected: &str, parse: &dyn Fn(&str) -> Option<i64>| {
            let raw = var(name)?;
            let value = parse(&raw);
            if value.is_none() {
                refused.push(format!("{name}={raw:?} is not {expected}: the default is used"));
            }
            value
        };
        // one adapter: nothing to choose yet, only a typo to report
        setting("ZIGBEE_ADAPTER", "a supported adapter (ember)", &|raw| raw.eq_ignore_ascii_case("ember").then_some(0));
        let protocol_version = setting("ZIGBEE_EZSP_PROTOCOL_VERSION", "an EZSP version (1–255)", &|raw| {
            integer(raw).filter(|value| (1..=255).contains(value))
        });
        let channel = setting("ZIGBEE_CHANNEL", "a Zigbee channel (11–26)", &|raw| {
            integer(raw).filter(|value| (11..=26).contains(value))
        });
        let tx_power = setting("ZIGBEE_TX_POWER", "a power in dBm (-128–127)", &|raw| raw.parse::<i8>().ok().map(i64::from));
        let pan_id = setting("ZIGBEE_PAN_ID", "a PAN id (0–0xffff)", &|raw| {
            integer(raw).filter(|value| (0..=0xffff).contains(value))
        });
        let extended_pan_id = setting("ZIGBEE_EXTENDED_PAN_ID", "16 hex digits", &|raw| eui64(raw).map(i64::from_be_bytes));
        let permit_join_seconds = setting("ZIGBEE_PERMIT_JOIN_SECONDS", "a duration in seconds (1–254)", &|raw| {
            integer(raw).filter(|value| (1..=254).contains(value))
        });
        let config = Self {
            adapter: Adapter::Ember,
            serial_port: var("ZIGBEE_SERIAL_PORT"),
            protocol_version: protocol_version
                .and_then(|value| NonZero::new(value as u8))
                .unwrap_or(defaults.protocol_version),
            channel: channel.map_or(defaults.channel, |value| value as u8),
            tx_power: tx_power.map_or(defaults.tx_power, |value| value as i8),
            pan_id: pan_id.map(|value| value as u16),
            extended_pan_id: extended_pan_id.map(i64::to_be_bytes),
            permit_join_seconds: permit_join_seconds.map_or(defaults.permit_join_seconds, |value| value as u16),
        };
        (config, refused)
    }
}

/// A decimal or `0x` hexadecimal integer.
fn integer(raw: &str) -> Option<i64> {
    match raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        Some(hex) => i64::from_str_radix(hex, 16).ok(),
        None => raw.parse().ok(),
    }
}

/// Eight bytes written as 16 hex digits, whatever separates them.
fn eui64(raw: &str) -> Option<[u8; 8]> {
    let digits = raw.chars().filter(char::is_ascii_hexdigit).collect::<Vec<_>>();
    let only_separators = raw.chars().all(|c| c.is_ascii_hexdigit() || matches!(c, ':' | '-' | ' '));
    if digits.len() != 16 || !only_separators {
        return None;
    }
    let mut bytes = [0_u8; 8];
    for (byte, pair) in bytes.iter_mut().zip(digits.chunks(2)) {
        *byte = u8::from_str_radix(&pair.iter().collect::<String>(), 16).ok()?;
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(vars: &[(&str, &str)]) -> (ZigbeeConfig, Vec<String>) {
        let vars = vars.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<Vec<_>>();
        ZigbeeConfig::parse(|name| vars.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone()))
    }

    #[test]
    fn nothing_set_is_every_default_and_no_warning() {
        let (config, refused) = parse(&[]);
        assert_eq!(config, ZigbeeConfig::default());
        assert!(refused.is_empty());
        assert_eq!((config.protocol_version.get(), config.channel, config.tx_power), (13, 11, 8));
    }

    #[test]
    fn every_setting_is_read() {
        let (config, refused) = parse(&[
            ("ZIGBEE_ADAPTER", "EMBER"),
            ("ZIGBEE_SERIAL_PORT", "/dev/ttyUSB0"),
            ("ZIGBEE_EZSP_PROTOCOL_VERSION", "14"),
            ("ZIGBEE_CHANNEL", "0x19"),
            ("ZIGBEE_TX_POWER", "-3"),
            ("ZIGBEE_PAN_ID", "0x1A62"),
            ("ZIGBEE_EXTENDED_PAN_ID", "dd:dd:dd:dd:dd:dd:dd:01"),
        ]);
        assert!(refused.is_empty(), "{refused:?}");
        assert_eq!(config.adapter, Adapter::Ember);
        assert_eq!(config.serial_port.as_deref(), Some("/dev/ttyUSB0"));
        assert_eq!(config.protocol_version.get(), 14);
        assert_eq!(config.channel, 25);
        assert_eq!(config.tx_power, -3);
        assert_eq!(config.pan_id, Some(0x1a62));
        assert_eq!(config.extended_pan_id, Some([0xdd, 0xdd, 0xdd, 0xdd, 0xdd, 0xdd, 0xdd, 0x01]));
    }

    #[test]
    fn a_value_that_does_not_parse_is_said_and_defaulted() {
        let (config, refused) = parse(&[
            ("ZIGBEE_ADAPTER", "zstack"),
            ("ZIGBEE_EZSP_PROTOCOL_VERSION", "0"),
            ("ZIGBEE_CHANNEL", "27"),
            ("ZIGBEE_TX_POWER", "200"),
            ("ZIGBEE_PAN_ID", "0x10000"),
            ("ZIGBEE_EXTENDED_PAN_ID", "dd:dd:dd"),
        ]);
        let defaults = ZigbeeConfig::default();
        assert_eq!(config, defaults);
        assert_eq!(refused.len(), 6, "{refused:?}");
        for name in ["ZIGBEE_ADAPTER", "ZIGBEE_EZSP_PROTOCOL_VERSION", "ZIGBEE_CHANNEL", "ZIGBEE_TX_POWER", "ZIGBEE_PAN_ID", "ZIGBEE_EXTENDED_PAN_ID"] {
            assert!(refused.iter().any(|line| line.starts_with(name)), "{name} not said: {refused:?}");
        }
    }

    #[test]
    fn an_extended_pan_id_is_16_hex_digits() {
        assert_eq!(eui64("00124b0012345678"), Some([0x00, 0x12, 0x4b, 0x00, 0x12, 0x34, 0x56, 0x78]));
        assert_eq!(eui64("00-12-4b-00-12-34-56-78"), Some([0x00, 0x12, 0x4b, 0x00, 0x12, 0x34, 0x56, 0x78]));
        assert_eq!(eui64("00124b001234567"), None, "15 digits");
        assert_eq!(eui64("zz124b0012345678zz"), None, "not hex");
    }
}
