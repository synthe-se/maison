//! The Mitsubishi air conditioner, driven in infrared through the Broadlink blaster: what its
//! clients share with the backend (`backend/src/mitsubishi_ir.rs`, which writes and encodes the
//! commands).

/// The setpoints the Mitsubishi protocol takes (°C).
pub const MIN_TEMPERATURE_C: u8 = 16;
pub const MAX_TEMPERATURE_C: u8 = 31;

/// The command that switches the unit off.
pub const OFF_COMMAND: &str = "state-off";

/// What « Allumer » sends when no order was ever kept: the web form's first settings (cool,
/// 20 °C, fan and vane automatic, the living room's horizontal vane centred).
pub const FIRST_ON_COMMAND: &str = "state-cool-20-fan-auto-vane-auto-wide-center";

/// The living room's unit: the backend looks its learnt codes up first (`send_mitsubishi`).
pub const MODEL: &str = "msz-hj5va";

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
}
