//! Mitsubishi heat-pump infrared frames (the 144-bit state protocol), built from a
//! `state-…` command and sent as Broadlink packets; and read back from learnt packets.

use crate::broadlink_ir;

pub const MITSUBISHI_HDR_MARK_US: u32 = 3400;
pub const MITSUBISHI_HDR_SPACE_US: u32 = 1750;
pub const MITSUBISHI_BIT_MARK_US: u32 = 450;
pub const MITSUBISHI_ONE_SPACE_US: u32 = 1300;
pub const MITSUBISHI_ZERO_SPACE_US: u32 = 420;
pub const MITSUBISHI_REPEAT_MARK_US: u32 = 440;
pub const MITSUBISHI_REPEAT_GAP_US: u32 = 15500;
pub const MITSUBISHI_STATE_LEN: usize = 18;
/// Header mark and space, two durations per bit, the closing mark.
pub const MITSUBISHI_FRAME_DURATIONS: usize = 2 + (MITSUBISHI_STATE_LEN * 8 * 2) + 1;
/// The Mitsubishi clock counts in 10-minute ticks; one day is 144 ticks.
const TICKS_PER_DAY: u16 = 144;

/// Temperature setpoint bounds supported by the protocol (°C).
pub const MIN_TEMPERATURE_C: u8 = 16;
pub const MAX_TEMPERATURE_C: u8 = 31;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    Auto,
    Cool,
    Dry,
    Heat,
    Fan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Fan {
    Auto,
    Level1,
    Level2,
    Level3,
    Level4,
    Silent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Vane {
    Auto,
    Highest,
    High,
    Middle,
    Low,
    Lowest,
    Swing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WideVane {
    LeftMax,
    Left,
    Center,
    Right,
    RightMax,
    Wide,
    Auto,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TimerMode {
    None,
    Stop,
    Start,
    StartStop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MitsubishiState {
    power: bool,
    mode: Mode,
    temperature_c: u8,
    fan: Fan,
    vane: Vane,
    wide_vane: WideVane,
    i_see: bool,
    ecocool: bool,
    clock: u8,
    start_clock: Option<u8>,
    stop_clock: Option<u8>,
    /// Relative stop timer ("turn off in N ticks"), resolved against the
    /// injected clock at encode time.
    stop_in_ticks: Option<u8>,
    timer_mode: TimerMode,
}

impl Default for MitsubishiState {
    fn default() -> Self {
        Self {
            power: true,
            mode: Mode::Cool,
            temperature_c: 20,
            fan: Fan::Auto,
            vane: Vane::Auto,
            wide_vane: WideVane::Center,
            i_see: false,
            ecocool: false,
            clock: 0,
            start_clock: None,
            stop_clock: None,
            stop_in_ticks: None,
            timer_mode: TimerMode::None,
        }
    }
}

/// Encodes a `state-*` command into a Broadlink IR packet.
///
/// `clock_ticks` is the AC unit's wall clock in 10-minute ticks since
/// midnight. Every Mitsubishi frame resets the unit's internal clock, and the
/// start/stop timers are absolute times against that clock — so the caller
/// must pass the current local time or the timers drift on every send.
pub fn encode_mitsubishi_command(
    command: &str,
    clock_ticks: u8,
) -> Result<Option<Vec<u8>>, String> {
    if !command.starts_with("state-") {
        return Ok(None);
    }

    let mut state = parse_state_command(command)?;
    state.clock = clock_ticks;
    if let Some(delta) = state.stop_in_ticks {
        // Resolve the relative sleep timer against the unit clock we are
        // about to transmit. Wraps past midnight (144 ticks per day).
        state.stop_clock = Some(((state.clock as u16 + delta as u16) % TICKS_PER_DAY) as u8);
        state.timer_mode = match state.timer_mode {
            TimerMode::Start | TimerMode::StartStop => TimerMode::StartStop,
            _ => TimerMode::Stop,
        };
    }
    let raw = build_state_bytes(state);
    Ok(Some(encode_broadlink_packet(&raw)))
}

/// Current wall-clock time as 10-minute ticks since midnight, the unit used
/// by the Mitsubishi clock/timer bytes.
///
/// Uses an explicit timezone rather than `chrono::Local`: on the Alpine Pi
/// deployment there is no tzdata, so `Local` silently degrades to UTC and
/// every absolute timer would fire hours off.
pub fn current_clock_ticks() -> u8 {
    use chrono::Timelike;
    let now = chrono::Utc::now().with_timezone(&chrono_tz::Europe::Paris);
    (now.hour() * 6 + now.minute() / 10) as u8
}

/// The settings encoded in a structured `state-*` command, in API-friendly
/// form. This is the single source of truth for restoring the UI form —
/// the frontend must not re-parse the command grammar.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClimateSettings {
    pub mode: String,
    pub temperature: u8,
    pub fan: String,
    pub vane: String,
    pub econo: bool,
    /// Relative sleep timer in minutes (`stopin` token), if armed.
    pub stop_in_minutes: Option<u16>,
}

/// Parses a structured command back into its settings. Returns `None` for
/// `state-off`, non-structured commands, and anything unparseable.
pub fn parse_climate_settings(command: &str) -> Option<ClimateSettings> {
    if command == "state-off" || !command.starts_with("state-") {
        return None;
    }

    let state = parse_state_command(command).ok()?;
    Some(ClimateSettings {
        mode: mode_token(state.mode).to_string(),
        temperature: state.temperature_c,
        fan: fan_token(state.fan).to_string(),
        vane: vane_token(state.vane).to_string(),
        econo: state.ecocool,
        stop_in_minutes: state.stop_in_ticks.map(|ticks| u16::from(ticks) * 10),
    })
}

fn mode_token(mode: Mode) -> &'static str {
    match mode {
        Mode::Auto => "auto",
        Mode::Cool => "cool",
        Mode::Dry => "dry",
        Mode::Heat => "heat",
        Mode::Fan => "fan",
    }
}

fn fan_token(fan: Fan) -> &'static str {
    match fan {
        Fan::Auto => "auto",
        Fan::Level1 => "1",
        Fan::Level2 => "2",
        Fan::Level3 => "3",
        Fan::Level4 => "4",
        Fan::Silent => "silent",
    }
}

fn vane_token(vane: Vane) -> &'static str {
    match vane {
        Vane::Auto => "auto",
        Vane::Highest => "highest",
        Vane::High => "high",
        Vane::Middle => "middle",
        Vane::Low => "low",
        Vane::Lowest => "lowest",
        Vane::Swing => "swing",
    }
}

fn parse_state_command(command: &str) -> Result<MitsubishiState, String> {
    if command == "state-off" {
        return Ok(MitsubishiState {
            power: false,
            mode: Mode::Cool,
            temperature_c: 16,
            fan: Fan::Level4,
            vane: Vane::Swing,
            wide_vane: WideVane::Center,
            ..MitsubishiState::default()
        });
    }

    let tokens = command.split('-').collect::<Vec<_>>();
    if tokens.len() < 7 || tokens[0] != "state" {
        return Err(format!("unsupported Mitsubishi state command '{command}'"));
    }

    let mut index = 1;
    let mode = parse_mode(tokens[index])?;
    index += 1;

    let temperature_c = parse_temperature(tokens.get(index).copied())?;
    index += 1;

    expect_token(&tokens, index, "fan")?;
    index += 1;
    let fan = parse_fan(tokens.get(index).copied())?;
    index += 1;

    expect_token(&tokens, index, "vane")?;
    index += 1;
    let vane = parse_vane(tokens.get(index).copied())?;
    index += 1;

    let mut state = MitsubishiState {
        mode,
        temperature_c,
        fan,
        vane,
        ..MitsubishiState::default()
    };

    while index < tokens.len() {
        match tokens[index] {
            // `left-max` / `right-max` hold a dash themselves: two tokens once split
            "wide" => {
                index += 1;
                let side = tokens.get(index).copied();
                let max = matches!(side, Some("left" | "right"))
                    && tokens.get(index + 1).copied() == Some("max");
                state.wide_vane = if max {
                    index += 1;
                    parse_wide_vane(side.map(|side| if side == "left" { "left-max" } else { "right-max" }))?
                } else {
                    parse_wide_vane(side)?
                };
                index += 1;
            }
            "econo" => {
                index += 1;
                state.ecocool = parse_toggle(tokens.get(index).copied(), "econo")?;
                index += 1;
            }
            "isee" => {
                index += 1;
                state.i_see = parse_toggle(tokens.get(index).copied(), "isee")?;
                index += 1;
            }
            // `timer-off` cancels any programmed start/stop timer on the unit.
            "timer" => {
                index += 1;
                let timer_enabled = parse_toggle(tokens.get(index).copied(), "timer")?;
                index += 1;
                if !timer_enabled {
                    state.timer_mode = TimerMode::None;
                    state.start_clock = None;
                    state.stop_clock = None;
                    state.stop_in_ticks = None;
                }
            }
            "start" => {
                index += 1;
                state.start_clock = Some(parse_clock(&tokens, &mut index)?);
            }
            "stop" => {
                index += 1;
                state.stop_clock = Some(parse_clock(&tokens, &mut index)?);
            }
            // Relative sleep timer: `stopin-<minutes>`, like the remote's
            // "turn off in 1h/3h" buttons. Resolved at encode time.
            "stopin" => {
                index += 1;
                state.stop_in_ticks = Some(parse_stop_in_minutes(tokens.get(index).copied())?);
                index += 1;
            }
            token => {
                return Err(format!(
                    "unsupported Mitsubishi state token '{token}' in '{command}'"
                ));
            }
        }
    }

    state.timer_mode = match (state.start_clock.is_some(), state.stop_clock.is_some()) {
        (false, false) => state.timer_mode,
        (true, false) => TimerMode::Start,
        (false, true) => TimerMode::Stop,
        (true, true) => TimerMode::StartStop,
    };

    if state.mode != Mode::Cool {
        state.ecocool = false;
    }

    Ok(state)
}

fn parse_mode(token: &str) -> Result<Mode, String> {
    match token {
        "auto" => Ok(Mode::Auto),
        "cool" => Ok(Mode::Cool),
        "dry" => Ok(Mode::Dry),
        "heat" => Ok(Mode::Heat),
        "fan" => Ok(Mode::Fan),
        _ => Err(format!("unsupported mode '{token}'")),
    }
}

fn parse_temperature(token: Option<&str>) -> Result<u8, String> {
    let value = token.ok_or_else(|| "missing temperature token".to_string())?;
    let temperature = value
        .parse::<u8>()
        .map_err(|_| format!("invalid temperature '{value}'"))?;
    if !(MIN_TEMPERATURE_C..=MAX_TEMPERATURE_C).contains(&temperature) {
        return Err(format!("temperature out of range '{value}'"));
    }
    Ok(temperature)
}

fn parse_fan(token: Option<&str>) -> Result<Fan, String> {
    match token.ok_or_else(|| "missing fan token".to_string())? {
        "auto" => Ok(Fan::Auto),
        "1" => Ok(Fan::Level1),
        "2" => Ok(Fan::Level2),
        "3" => Ok(Fan::Level3),
        "4" => Ok(Fan::Level4),
        "silent" => Ok(Fan::Silent),
        other => Err(format!("unsupported fan '{other}'")),
    }
}

fn parse_vane(token: Option<&str>) -> Result<Vane, String> {
    match token.ok_or_else(|| "missing vane token".to_string())? {
        "auto" => Ok(Vane::Auto),
        "highest" => Ok(Vane::Highest),
        "high" => Ok(Vane::High),
        "middle" => Ok(Vane::Middle),
        "low" => Ok(Vane::Low),
        "lowest" => Ok(Vane::Lowest),
        "swing" => Ok(Vane::Swing),
        other => Err(format!("unsupported vane '{other}'")),
    }
}

fn parse_wide_vane(token: Option<&str>) -> Result<WideVane, String> {
    match token.ok_or_else(|| "missing wide vane token".to_string())? {
        "left-max" => Ok(WideVane::LeftMax),
        "left" => Ok(WideVane::Left),
        "center" => Ok(WideVane::Center),
        "right" => Ok(WideVane::Right),
        "right-max" => Ok(WideVane::RightMax),
        "wide" => Ok(WideVane::Wide),
        "auto" => Ok(WideVane::Auto),
        other => Err(format!("unsupported wide vane '{other}'")),
    }
}

fn parse_toggle(token: Option<&str>, label: &str) -> Result<bool, String> {
    match token.ok_or_else(|| format!("missing {label} toggle"))? {
        "on" => Ok(true),
        "off" => Ok(false),
        other => Err(format!("unsupported {label} toggle '{other}'")),
    }
}

fn parse_clock(tokens: &[&str], index: &mut usize) -> Result<u8, String> {
    let hours = tokens
        .get(*index)
        .ok_or_else(|| "missing clock hour".to_string())?
        .parse::<u8>()
        .map_err(|_| "invalid clock hour".to_string())?;
    *index += 1;
    let minutes = tokens
        .get(*index)
        .ok_or_else(|| "missing clock minute".to_string())?
        .parse::<u8>()
        .map_err(|_| "invalid clock minute".to_string())?;
    *index += 1;

    if hours > 23 || minutes > 59 || minutes % 10 != 0 {
        return Err(format!(
            "unsupported clock value {:02}:{:02}",
            hours, minutes
        ));
    }

    Ok(hours * 6 + minutes / 10)
}

fn parse_stop_in_minutes(token: Option<&str>) -> Result<u8, String> {
    let value = token.ok_or_else(|| "missing stopin minutes".to_string())?;
    let minutes = value
        .parse::<u16>()
        .map_err(|_| format!("invalid stopin minutes '{value}'"))?;
    if minutes == 0 || minutes % 10 != 0 || minutes >= 24 * 60 {
        return Err(format!(
            "stopin minutes must be a multiple of 10 below 1440, got '{value}'"
        ));
    }
    Ok((minutes / 10) as u8)
}

fn expect_token(tokens: &[&str], index: usize, expected: &str) -> Result<(), String> {
    match tokens.get(index).copied() {
        Some(token) if token == expected => Ok(()),
        Some(token) => Err(format!("expected '{expected}', got '{token}'")),
        None => Err(format!("missing token '{expected}'")),
    }
}

fn build_state_bytes(state: MitsubishiState) -> [u8; MITSUBISHI_STATE_LEN] {
    let mut bytes = [0_u8; MITSUBISHI_STATE_LEN];
    bytes[..5].copy_from_slice(&[0x23, 0xCB, 0x26, 0x01, 0x00]);
    bytes[5] = if state.power { 0x20 } else { 0x00 };
    bytes[6] = mode_byte(state.mode) | if state.i_see { 0x40 } else { 0x00 };
    bytes[7] = state.temperature_c.saturating_sub(16);
    bytes[8] = (wide_vane_byte(state.wide_vane) << 4) | mode_nibble(state.mode);
    bytes[9] = fan_byte(state.fan) | vane_byte(state.vane);
    bytes[10] = state.clock;
    bytes[11] = state.stop_clock.unwrap_or(0);
    bytes[12] = state.start_clock.unwrap_or(0);
    bytes[13] = timer_mode_byte(state.timer_mode);
    bytes[14] = if state.ecocool { 0x20 } else { 0x00 };
    bytes[17] = checksum(&bytes);
    bytes
}

fn mode_byte(mode: Mode) -> u8 {
    match mode {
        Mode::Heat => 0x08,
        Mode::Dry => 0x10,
        Mode::Cool => 0x18,
        Mode::Auto => 0x20,
        Mode::Fan => 0x38,
    }
}

fn mode_nibble(mode: Mode) -> u8 {
    match mode {
        Mode::Auto => 0x00,
        Mode::Cool => 0x06,
        Mode::Dry => 0x02,
        Mode::Heat => 0x00,
        Mode::Fan => 0x07,
    }
}

fn wide_vane_byte(wide_vane: WideVane) -> u8 {
    match wide_vane {
        WideVane::LeftMax => 0x1,
        WideVane::Left => 0x2,
        WideVane::Center => 0x3,
        WideVane::Right => 0x4,
        WideVane::RightMax => 0x5,
        WideVane::Wide => 0x6,
        WideVane::Auto => 0x8,
    }
}

fn fan_byte(fan: Fan) -> u8 {
    match fan {
        Fan::Auto => 0x80,
        Fan::Level1 => 0x01,
        Fan::Level2 => 0x02,
        Fan::Level3 => 0x03,
        Fan::Level4 => 0x04,
        Fan::Silent => 0x05,
    }
}

fn vane_byte(vane: Vane) -> u8 {
    let code = match vane {
        Vane::Auto => 0x00,
        Vane::Highest => 0x01,
        Vane::High => 0x02,
        Vane::Middle => 0x03,
        Vane::Low => 0x04,
        Vane::Lowest => 0x05,
        Vane::Swing => 0x07,
    };
    0x40 | (code << 3)
}

fn timer_mode_byte(timer_mode: TimerMode) -> u8 {
    match timer_mode {
        TimerMode::None => 0x00,
        TimerMode::Stop => 0x03,
        TimerMode::Start => 0x05,
        TimerMode::StartStop => 0x07,
    }
}

fn checksum(bytes: &[u8; MITSUBISHI_STATE_LEN]) -> u8 {
    bytes[..MITSUBISHI_STATE_LEN - 1]
        .iter()
        .fold(0_u8, |sum, byte| sum.wrapping_add(*byte))
}

fn encode_broadlink_packet(state: &[u8; MITSUBISHI_STATE_LEN]) -> Vec<u8> {
    let mut pulses = Vec::with_capacity(MITSUBISHI_FRAME_DURATIONS * 2 + 1);
    append_frame(&mut pulses, state);
    pulses.push(MITSUBISHI_REPEAT_GAP_US);
    append_frame(&mut pulses, state);
    broadlink_ir::encode(pulses)
}

/// One frame read back from the durations of a packet.
#[derive(Debug)]
pub struct MitsubishiFrame {
    pub header_mark_us: u32,
    pub header_space_us: u32,
    pub footer_mark_us: u32,
    pub bytes: [u8; MITSUBISHI_STATE_LEN],
}

/// The frames of a packet's durations and the gaps between them.
pub fn decode_frames(durations_us: &[u32]) -> Result<(Vec<MitsubishiFrame>, Vec<u32>), String> {
    let mut frames = Vec::new();
    let mut repeat_gaps_us = Vec::new();
    let mut index = 0;
    while let Some(frame) = durations_us.get(index..index + MITSUBISHI_FRAME_DURATIONS) {
        frames.push(decode_frame(frame));
        index += MITSUBISHI_FRAME_DURATIONS;
        match durations_us.get(index) {
            Some(gap) if *gap > 5000 => {
                repeat_gaps_us.push(*gap);
                index += 1;
            }
            _ => break,
        }
    }
    if frames.is_empty() {
        return Err("no Mitsubishi 144-bit frame found".to_string());
    }
    Ok((frames, repeat_gaps_us))
}

fn decode_frame(durations_us: &[u32]) -> MitsubishiFrame {
    let mut bytes = [0_u8; MITSUBISHI_STATE_LEN];
    let bit_pairs = &durations_us[2..MITSUBISHI_FRAME_DURATIONS - 1];
    for (bit_index, [_, space]) in bit_pairs.as_chunks::<2>().0.iter().enumerate() {
        let one = *space > (MITSUBISHI_ONE_SPACE_US + MITSUBISHI_ZERO_SPACE_US) / 2;
        bytes[bit_index / 8] |= u8::from(one) << (bit_index % 8);
    }
    MitsubishiFrame {
        header_mark_us: durations_us[0],
        header_space_us: durations_us[1],
        footer_mark_us: durations_us[MITSUBISHI_FRAME_DURATIONS - 1],
        bytes,
    }
}

/// Whether the last byte is the sum of the others.
pub fn checksum_valid(bytes: &[u8; MITSUBISHI_STATE_LEN]) -> bool {
    checksum(bytes) == bytes[MITSUBISHI_STATE_LEN - 1]
}

fn append_frame(pulses: &mut Vec<u32>, state: &[u8; MITSUBISHI_STATE_LEN]) {
    pulses.push(MITSUBISHI_HDR_MARK_US);
    pulses.push(MITSUBISHI_HDR_SPACE_US);
    for byte in state {
        for bit in 0..8 {
            pulses.push(MITSUBISHI_BIT_MARK_US);
            let is_one = (byte >> bit) & 1 == 1;
            pulses.push(if is_one {
                MITSUBISHI_ONE_SPACE_US
            } else {
                MITSUBISHI_ZERO_SPACE_US
            });
        }
    }
    pulses.push(MITSUBISHI_REPEAT_MARK_US);
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    #[test]
    fn parses_state_command() {
        let packet = encode_mitsubishi_command(
            "state-cool-22-fan-2-vane-low-wide-center-econo-on-start-06-00-stop-11-00",
            0,
        )
        .expect("command should parse")
        .expect("state command should generate");

        let decoded = STANDARD.encode(packet);
        assert!(!decoded.is_empty());
    }

    #[test]
    fn clock_ticks_are_injected_into_the_frame() {
        // 14:30 -> 14 * 6 + 3 = 87 ticks.
        let mut state = parse_state_command("state-cool-20-fan-auto-vane-auto-wide-center-stop-23-50")
            .expect("state command should parse");
        state.clock = 87;
        let bytes = build_state_bytes(state);

        assert_eq!(bytes[10], 87);
        assert_eq!(bytes[11], 23 * 6 + 5); // stop timer 23:50
        assert_eq!(bytes[13], 0x03); // TimerMode::Stop
        assert_eq!(bytes[17], checksum(&bytes));
    }

    #[test]
    fn midnight_stop_timer_is_representable() {
        let state = parse_state_command("state-cool-20-fan-auto-vane-auto-wide-center-stop-00-00")
            .expect("state command should parse");
        assert_eq!(state.stop_clock, Some(0));
        assert_eq!(state.timer_mode, TimerMode::Stop);

        let bytes = build_state_bytes(state);
        assert_eq!(bytes[11], 0);
        assert_eq!(bytes[13], 0x03); // the stop timer must still be armed
    }

    #[test]
    fn relative_stop_timer_resolves_against_injected_clock() {
        // Clock 23:00 (138 ticks) + stop in 3h (18 ticks) wraps to 02:00 (12).
        let packet = encode_mitsubishi_command(
            "state-cool-20-fan-auto-vane-auto-wide-center-stopin-180",
            138,
        )
        .expect("command should parse")
        .expect("state command should generate");
        assert!(!packet.is_empty());

        let mut state = parse_state_command("state-cool-20-fan-auto-vane-auto-wide-center-stopin-180")
            .expect("state command should parse");
        assert_eq!(state.stop_in_ticks, Some(18));
        state.clock = 138;
        state.stop_clock = Some(((state.clock as u16 + 18) % TICKS_PER_DAY) as u8);
        state.timer_mode = TimerMode::Stop;
        let bytes = build_state_bytes(state);
        assert_eq!(bytes[10], 138);
        assert_eq!(bytes[11], 12);
        assert_eq!(bytes[13], 0x03);
    }

    #[test]
    fn stopin_rejects_invalid_durations() {
        for command in [
            "state-cool-20-fan-auto-vane-auto-wide-center-stopin-0",
            "state-cool-20-fan-auto-vane-auto-wide-center-stopin-15",
            "state-cool-20-fan-auto-vane-auto-wide-center-stopin-1440",
        ] {
            assert!(parse_state_command(command).is_err(), "{command} should be rejected");
        }
    }

    #[test]
    fn parse_climate_settings_roundtrips_a_structured_command() {
        let settings = parse_climate_settings(
            "state-heat-24-fan-2-vane-middle-wide-center-econo-off-stopin-90",
        )
        .expect("settings should parse");
        assert_eq!(settings.mode, "heat");
        assert_eq!(settings.temperature, 24);
        assert_eq!(settings.fan, "2");
        assert_eq!(settings.vane, "middle");
        assert!(!settings.econo);
        assert_eq!(settings.stop_in_minutes, Some(90));

        assert_eq!(parse_climate_settings("state-off"), None);
        assert_eq!(parse_climate_settings("cool_22_auto"), None);
    }

    #[test]
    fn timer_off_cancels_programmed_timers() {
        let state = parse_state_command(
            "state-cool-20-fan-auto-vane-auto-wide-center-stop-11-00-timer-off",
        )
        .expect("state command should parse");
        assert_eq!(state.timer_mode, TimerMode::None);
        assert_eq!(state.stop_clock, None);
    }

    #[test]
    fn builds_expected_bytes_for_manual_state() {
        let state = parse_state_command("state-cool-20-fan-3-vane-swing-wide-center")
            .expect("state command should parse");
        let bytes = build_state_bytes(state);

        assert_eq!(&bytes[..5], &[0x23, 0xCB, 0x26, 0x01, 0x00]);
        assert_eq!(bytes[6], 0x18);
        assert_eq!(bytes[7], 0x04);
        assert_eq!(bytes[8], 0x36);
        assert_eq!(bytes[9], 0x7B);
        assert_eq!(bytes[17], checksum(&bytes));
    }

    /// Encoded, sent as a Broadlink packet, read back: every wide vane position survives,
    /// the two with a dash in their name included.
    #[test]
    fn every_wide_vane_round_trips_through_a_packet() {
        for (token, code) in [
            ("left-max", 0x1),
            ("left", 0x2),
            ("center", 0x3),
            ("right", 0x4),
            ("right-max", 0x5),
            ("wide", 0x6),
            ("auto", 0x8),
        ] {
            let command = format!("state-cool-20-fan-auto-vane-auto-wide-{token}-econo-on");
            let packet = encode_mitsubishi_command(&command, 0)
                .unwrap_or_else(|error| panic!("{command}: {error}"))
                .expect("a state command");
            let durations = broadlink_ir::decode(&packet).expect("decodes");
            let (frames, gaps) = decode_frames(&durations).expect("frames");
            assert_eq!(frames.len(), 2, "sent twice");
            assert_eq!(gaps.len(), 1);
            let bytes = frames[0].bytes;
            assert_eq!(bytes[8] >> 4, code, "{token}");
            assert_eq!(bytes[14], 0x20, "econo after the wide vane still parsed ({token})");
            assert!(checksum_valid(&bytes));
        }
        assert!(parse_state_command("state-cool-20-fan-auto-vane-auto-wide-max").is_err());
    }

    #[test]
    fn off_command_matches_known_state_shape() {
        let state = parse_state_command("state-off").expect("off command should parse");
        let bytes = build_state_bytes(state);

        assert_eq!(bytes[5], 0x00);
        assert_eq!(bytes[6], 0x18);
        assert_eq!(bytes[7], 0x00);
        assert_eq!(bytes[8], 0x36);
        assert_eq!(bytes[9], 0x7C);
    }
}
