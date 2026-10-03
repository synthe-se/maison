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

/// A setting's values, said once: its command token and its code in the frame. The parser,
/// the encoder, the serde form (the API, `climate-state.json`) and the decoder all read the
/// table, so a token and its code cannot drift apart.
pub trait Setting: Copy + Eq + 'static {
    /// What the setting is called in error messages.
    const NAME: &'static str;
    /// `(value, command token, code in the frame)`.
    const TABLE: &'static [(Self, &'static str, u8)];

    fn token(self) -> &'static str {
        Self::TABLE.iter().find(|(value, ..)| *value == self).map_or("", |(_, token, _)| token)
    }

    fn code(self) -> u8 {
        Self::TABLE.iter().find(|(value, ..)| *value == self).map_or(0, |(.., code)| *code)
    }

    fn from_token(token: &str) -> Option<Self> {
        Self::TABLE.iter().find(|(_, t, _)| *t == token).map(|(value, ..)| *value)
    }

    fn from_code(code: u8) -> Option<Self> {
        Self::TABLE.iter().find(|(.., c)| *c == code).map(|(value, ..)| *value)
    }

    /// The value of a command token, or why not.
    fn parse(token: Option<&str>) -> Result<Self, String> {
        let token = token.ok_or_else(|| format!("missing {} token", Self::NAME))?;
        Self::from_token(token).ok_or_else(|| format!("unsupported {} '{token}'", Self::NAME))
    }
}

/// Serialised as its command token, both ways (the strings the API has always used).
macro_rules! setting_serde {
    ($($setting:ty),*) => {$(
        impl serde::Serialize for $setting {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.token())
            }
        }

        impl<'de> serde::Deserialize<'de> for $setting {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let token = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
                <$setting>::parse(Some(&token)).map_err(serde::de::Error::custom)
            }
        }
    )*};
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Auto,
    Cool,
    Dry,
    Heat,
    Fan,
}

/// The code is the 3-bit mode of byte 6 (bits 3 to 5).
impl Setting for Mode {
    const NAME: &'static str = "mode";
    const TABLE: &'static [(Self, &'static str, u8)] = &[
        (Mode::Heat, "heat", 0b001),
        (Mode::Dry, "dry", 0b010),
        (Mode::Cool, "cool", 0b011),
        (Mode::Auto, "auto", 0b100),
        (Mode::Fan, "fan", 0b111),
    ];
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Fan {
    Auto,
    Level1,
    Level2,
    Level3,
    Level4,
    Silent,
}

/// The code is byte 9's fan bits: the speed (0 to 2), or the « auto » bit 7.
impl Setting for Fan {
    const NAME: &'static str = "fan";
    const TABLE: &'static [(Self, &'static str, u8)] = &[
        (Fan::Auto, "auto", 0x80),
        (Fan::Level1, "1", 1),
        (Fan::Level2, "2", 2),
        (Fan::Level3, "3", 3),
        (Fan::Level4, "4", 4),
        (Fan::Silent, "silent", 5),
    ];
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Vane {
    Auto,
    Highest,
    High,
    Middle,
    Low,
    Lowest,
    Swing,
}

/// The code is byte 9's bits 3 to 5 (bit 6 set alongside: « vane set »).
impl Setting for Vane {
    const NAME: &'static str = "vane";
    const TABLE: &'static [(Self, &'static str, u8)] = &[
        (Vane::Auto, "auto", 0b000),
        (Vane::Highest, "highest", 0b001),
        (Vane::High, "high", 0b010),
        (Vane::Middle, "middle", 0b011),
        (Vane::Low, "low", 0b100),
        (Vane::Lowest, "lowest", 0b101),
        (Vane::Swing, "swing", 0b111),
    ];
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WideVane {
    LeftMax,
    Left,
    Center,
    Right,
    RightMax,
    Wide,
    Auto,
}

/// The code is byte 8's high nibble.
impl Setting for WideVane {
    const NAME: &'static str = "wide vane";
    const TABLE: &'static [(Self, &'static str, u8)] = &[
        (WideVane::LeftMax, "left-max", 0x1),
        (WideVane::Left, "left", 0x2),
        (WideVane::Center, "center", 0x3),
        (WideVane::Right, "right", 0x4),
        (WideVane::RightMax, "right-max", 0x5),
        (WideVane::Wide, "wide", 0x6),
        (WideVane::Auto, "auto", 0x8),
    ];
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimerMode {
    None,
    Stop,
    Start,
    StartStop,
}

/// The code is byte 13's low 3 bits. Not a command token: the timers follow from
/// `start-…`/`stop-…`/`stopin-…`; the names are for the decoder.
impl Setting for TimerMode {
    const NAME: &'static str = "timer";
    const TABLE: &'static [(Self, &'static str, u8)] = &[
        (TimerMode::None, "none", 0),
        (TimerMode::Stop, "stop", 3),
        (TimerMode::Start, "start", 5),
        (TimerMode::StartStop, "start-stop", 7),
    ];
}

setting_serde!(Mode, Fan, Vane, WideVane);

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
/// In the house's time zone (`util::HOUSE_TZ`), never `chrono::Local` (UTC on the Pi).
pub fn current_clock_ticks() -> u8 {
    use chrono::Timelike;
    let now = chrono::Utc::now().with_timezone(&crate::util::HOUSE_TZ);
    (now.hour() * 6 + now.minute() / 10) as u8
}

/// The settings encoded in a structured `state-*` command, in API-friendly form. This is
/// the single source of truth for restoring the UI form: the frontend reads these instead of
/// re-parsing the command grammar, and may send them instead of a command
/// ([`ClimateSettings::command`] writes it).
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClimateSettings {
    pub mode: Mode,
    pub temperature: u8,
    pub fan: Fan,
    pub vane: Vane,
    /// The horizontal vane; absent, the unit's default (centre). Absent from files written
    /// before it was kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wide: Option<WideVane>,
    #[serde(default)]
    pub econo: bool,
    /// Relative sleep timer in minutes (`stopin` token), if armed.
    #[serde(default)]
    pub stop_in_minutes: Option<u16>,
}

impl ClimateSettings {
    /// The `state-…` command for these settings (checked: an out-of-range temperature or
    /// timer is refused as a hand-written command would be).
    pub fn command(&self) -> Result<String, String> {
        let mut parts = vec![
            "state".to_string(),
            self.mode.token().to_string(),
            self.temperature.to_string(),
            "fan".to_string(),
            self.fan.token().to_string(),
            "vane".to_string(),
            self.vane.token().to_string(),
        ];
        if let Some(wide) = self.wide {
            parts.extend(["wide".to_string(), wide.token().to_string()]);
        }
        if self.econo {
            parts.extend(["econo".to_string(), "on".to_string()]);
        }
        if let Some(minutes) = self.stop_in_minutes {
            parts.extend(["stopin".to_string(), minutes.to_string()]);
        }
        let command = parts.join("-");
        parse_state_command(&command)?;
        Ok(command)
    }
}

/// The command that switches the unit off (it carries no settings).
pub const OFF_COMMAND: &str = "state-off";

/// Parses a structured command back into its settings. Returns `None` for
/// `state-off`, non-structured commands, and anything unparseable.
pub fn parse_climate_settings(command: &str) -> Option<ClimateSettings> {
    if command == OFF_COMMAND || !command.starts_with("state-") {
        return None;
    }

    let state = parse_state_command(command).ok()?;
    Some(ClimateSettings {
        mode: state.mode,
        temperature: state.temperature_c,
        fan: state.fan,
        vane: state.vane,
        wide: Some(state.wide_vane),
        econo: state.ecocool,
        stop_in_minutes: state.stop_in_ticks.map(|ticks| u16::from(ticks) * 10),
    })
}

fn parse_state_command(command: &str) -> Result<MitsubishiState, String> {
    if command == OFF_COMMAND {
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
    let mode = Mode::parse(Some(tokens[index]))?;
    index += 1;

    let temperature_c = parse_temperature(tokens.get(index).copied())?;
    index += 1;

    expect_token(&tokens, index, "fan")?;
    index += 1;
    let fan = Fan::parse(tokens.get(index).copied())?;
    index += 1;

    expect_token(&tokens, index, "vane")?;
    index += 1;
    let vane = Vane::parse(tokens.get(index).copied())?;
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
                    WideVane::parse(side.map(|side| if side == "left" { "left-max" } else { "right-max" }))?
                } else {
                    WideVane::parse(side)?
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
    bytes[6] = (state.mode.code() << 3) | if state.i_see { 0x40 } else { 0x00 };
    bytes[7] = state.temperature_c.saturating_sub(16);
    bytes[8] = (state.wide_vane.code() << 4) | mode_nibble(state.mode);
    // bit 6: the vane is set
    bytes[9] = state.fan.code() | 0x40 | (state.vane.code() << 3);
    bytes[10] = state.clock;
    bytes[11] = state.stop_clock.unwrap_or(0);
    bytes[12] = state.start_clock.unwrap_or(0);
    bytes[13] = state.timer_mode.code();
    bytes[14] = if state.ecocool { 0x20 } else { 0x00 };
    bytes[17] = checksum(&bytes);
    bytes
}

/// Byte 8's low nibble, which each mode sets its own way (not a table code: auto and heat
/// share theirs).
fn mode_nibble(mode: Mode) -> u8 {
    match mode {
        Mode::Auto | Mode::Heat => 0x00,
        Mode::Cool => 0x06,
        Mode::Dry => 0x02,
        Mode::Fan => 0x07,
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

/// What a frame's state bytes say, read with the same tables the encoder writes with;
/// `None` for a code no table knows (a capture from a mode this module never sends).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadState {
    pub power: bool,
    pub mode: Option<Mode>,
    pub i_see: bool,
    /// Above 16 °C, in half degrees.
    pub temperature_half_degrees: u8,
    pub fan: Option<Fan>,
    pub vane: Option<Vane>,
    pub wide_vane: Option<WideVane>,
    /// The clock bytes, in 10-minute ticks (0: unset).
    pub clock: u8,
    pub stop_clock: u8,
    pub start_clock: u8,
    pub timer: Option<TimerMode>,
    pub econo: bool,
}

pub fn read_state(bytes: &[u8; MITSUBISHI_STATE_LEN]) -> ReadState {
    ReadState {
        power: bytes[5] & 0x20 != 0,
        mode: Mode::from_code((bytes[6] >> 3) & 0x07),
        i_see: bytes[6] & 0x40 != 0,
        temperature_half_degrees: ((bytes[7] & 0x0F) * 2) + u8::from(bytes[7] & 0x10 != 0),
        fan: Fan::from_code(if bytes[9] & 0x80 != 0 { 0x80 } else { bytes[9] & 0x07 }),
        vane: Vane::from_code((bytes[9] >> 3) & 0x07),
        wide_vane: WideVane::from_code(bytes[8] >> 4),
        clock: bytes[10],
        stop_clock: bytes[11],
        start_clock: bytes[12],
        timer: TimerMode::from_code(bytes[13] & 0x07),
        econo: bytes[14] & 0x20 != 0,
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
        assert_eq!(settings.mode, Mode::Heat);
        assert_eq!(settings.temperature, 24);
        assert_eq!(settings.fan, Fan::Level2);
        assert_eq!(settings.vane, Vane::Middle);
        assert_eq!(settings.wide, Some(WideVane::Center));
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

    /// Every value of every table: its token parses, encodes into a packet, and reads back
    /// from the packet's bytes as itself.
    #[test]
    fn every_setting_round_trips_command_packet_and_back() {
        fn read(command: &str) -> ReadState {
            let packet = encode_mitsubishi_command(command, 0).expect(command).expect("a state command");
            let durations = broadlink_ir::decode(&packet).expect("decodes");
            let (frames, _) = decode_frames(&durations).expect("frames");
            assert!(checksum_valid(&frames[0].bytes));
            read_state(&frames[0].bytes)
        }
        for (mode, token, _) in Mode::TABLE {
            let state = read(&format!("state-{token}-20-fan-auto-vane-auto"));
            assert_eq!(state.mode, Some(*mode), "{token}");
            assert!(state.power);
            assert_eq!(state.temperature_half_degrees, 8, "20 °C");
        }
        for (fan, token, _) in Fan::TABLE {
            assert_eq!(read(&format!("state-cool-20-fan-{token}-vane-auto")).fan, Some(*fan), "{token}");
        }
        for (vane, token, _) in Vane::TABLE {
            assert_eq!(read(&format!("state-cool-20-fan-auto-vane-{token}")).vane, Some(*vane), "{token}");
        }
        for (wide, token, _) in WideVane::TABLE {
            let state = read(&format!("state-cool-20-fan-auto-vane-auto-wide-{token}"));
            assert_eq!(state.wide_vane, Some(*wide), "{token}");
        }
        let timed = read("state-cool-20-fan-auto-vane-auto-start-06-00-stop-11-00");
        assert_eq!(timed.timer, Some(TimerMode::StartStop));
        assert_eq!((timed.start_clock, timed.stop_clock), (36, 66));
    }

    /// Tables say each token and each code once.
    #[test]
    fn tables_hold_no_duplicate() {
        fn unique<T: Setting>() {
            for (i, (_, token, code)) in T::TABLE.iter().enumerate() {
                for (_, other_token, other_code) in &T::TABLE[i + 1..] {
                    assert_ne!(token, other_token, "{}", T::NAME);
                    assert_ne!(code, other_code, "{}", T::NAME);
                }
            }
        }
        unique::<Mode>();
        unique::<Fan>();
        unique::<Vane>();
        unique::<WideVane>();
        unique::<TimerMode>();
    }

    /// The JSON the API and `climate-state.json` have always held still reads, and writes
    /// the same strings back.
    #[test]
    fn settings_json_is_unchanged() {
        let old = serde_json::json!({
            "mode": "cool", "temperature": 21, "fan": "auto", "vane": "swing",
            "econo": false, "stopInMinutes": 180
        });
        let settings: ClimateSettings = serde_json::from_value(old.clone()).expect("old JSON reads");
        assert_eq!((settings.mode, settings.fan, settings.vane, settings.wide), (Mode::Cool, Fan::Auto, Vane::Swing, None));
        assert_eq!(serde_json::to_value(&settings).unwrap(), old);
        let level: ClimateSettings =
            serde_json::from_value(serde_json::json!({"mode": "heat", "temperature": 22, "fan": "3", "vane": "low", "wide": "left-max"}))
                .expect("reads");
        assert_eq!((level.fan, level.wide, level.econo, level.stop_in_minutes), (Fan::Level3, Some(WideVane::LeftMax), false, None));
        assert!(serde_json::from_value::<ClimateSettings>(serde_json::json!({"mode": "turbo", "temperature": 20, "fan": "1", "vane": "low"})).is_err());
    }

    #[test]
    fn settings_write_the_command_they_were_read_from() {
        for command in [
            "state-cool-21-fan-auto-vane-swing-wide-center",
            "state-heat-24-fan-2-vane-middle-wide-left-max-stopin-90",
            "state-cool-16-fan-4-vane-swing-wide-center-econo-on",
        ] {
            let settings = parse_climate_settings(command).expect(command);
            assert_eq!(settings.command().expect("valid"), command);
        }
        let mut bad = parse_climate_settings("state-cool-21-fan-auto-vane-swing").unwrap();
        bad.temperature = 40;
        assert!(bad.command().is_err());
        bad.temperature = 21;
        bad.stop_in_minutes = Some(15);
        assert!(bad.command().is_err());
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
