//! The Mitsubishi air conditioner, driven in infrared through the Broadlink blaster: what its
//! clients share with the backend (`backend/src/mitsubishi_ir.rs`, which writes and encodes the
//! commands).

use serde::{Deserialize, Serialize};

/// The setpoints the Mitsubishi protocol takes (°C).
pub const MIN_TEMPERATURE_C: u8 = 16;
pub const MAX_TEMPERATURE_C: u8 = 31;

/// The settings' tokens as the grammar spells them, in the order a person reads them
/// (`backend/src/mitsubishi_ir.rs`'s tables hold each one's frame code; a test there keeps them
/// in step).
pub const MODES: [&str; 5] = ["cool", "heat", "dry", "fan", "auto"];
pub const FANS: [&str; 6] = ["auto", "1", "2", "3", "4", "silent"];
pub const VANES: [&str; 7] = [
    "auto", "highest", "high", "middle", "low", "lowest", "swing",
];

/// The sleep timer's presets, in minutes, like the remote's 1 h / 3 h buttons (multiples of the
/// unit's 10-minute ticks; the web form's `STOP_AFTER_CHOICES`).
pub const STOP_CHOICES_MIN: [u32; 7] = [30, 60, 120, 180, 300, 480, 720];

/// The command that switches the unit off.
pub const OFF_COMMAND: &str = "state-off";

/// What « Allumer » sends when no order was ever kept: the web form's first settings (cool,
/// 20 °C, fan and vane automatic, the living room's horizontal vane centred).
pub const FIRST_ON_COMMAND: &str = "state-cool-20-fan-auto-vane-auto-wide-center";

/// The living room's unit: the backend looks its learnt codes up first (`send_mitsubishi`).
pub const MODEL: &str = "msz-hj5va";

/// `POST /api/broadlink/mitsubishi/send`: a structured command as text (`state-cool-21-…`) or as
/// settings (`{mode, temperature, …}`, written into a command by the backend), one of the two.
/// `S`: the settings, the backend's typed `ClimateSettings`, as clients found them otherwise.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendRequest<S> {
    pub host: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<S>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// A setpoint `delta` degrees away from `from`, within the protocol's bounds.
pub fn step_temperature(from: u8, delta: i8) -> u8 {
    (i16::from(from) + i16::from(delta))
        .clamp(i16::from(MIN_TEMPERATURE_C), i16::from(MAX_TEMPERATURE_C)) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_setpoint_steps_within_the_bounds() {
        assert_eq!(step_temperature(20, 1), 21);
        assert_eq!(step_temperature(16, -1), 16);
        assert_eq!(step_temperature(31, 1), 31);
    }

    #[test]
    fn a_send_says_one_of_the_two_and_nothing_empty() {
        let off = SendRequest::<()> {
            host: "192.168.1.60".into(),
            local_ip: None,
            command: Some(OFF_COMMAND.into()),
            settings: None,
            model: Some(MODEL.into()),
        };
        assert_eq!(
            serde_json::to_value(&off).unwrap(),
            serde_json::json!({
                "host": "192.168.1.60",
                "command": "state-off",
                "model": "msz-hj5va"
            })
        );
    }
}
