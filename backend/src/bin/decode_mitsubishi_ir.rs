//! Reads the Mitsubishi codes learnt into `broadlink-codes.json` back into their fields,
//! to reverse-engineer the protocol from captures.
//!
//!     cargo run --bin decode_mitsubishi_ir -- [path/to/broadlink-codes.json]

use std::{env, path::PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use maison_backend::{
    broadlink::{self, BroadlinkCodeEntry},
    broadlink_ir,
    mitsubishi_ir::{
        self, MitsubishiFrame, MITSUBISHI_BIT_MARK_US, MITSUBISHI_HDR_MARK_US, MITSUBISHI_HDR_SPACE_US,
        MITSUBISHI_ONE_SPACE_US, MITSUBISHI_REPEAT_GAP_US, MITSUBISHI_STATE_LEN, MITSUBISHI_ZERO_SPACE_US,
    },
    util,
};

#[derive(Debug)]
struct DecodedPacket {
    durations_us: Vec<u32>,
    repeat_gaps_us: Vec<u32>,
    frames: Vec<MitsubishiFrame>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(default_codes_path);
    let codes = broadlink::read_codes(&path).map_err(|error| format!("{}: {error}", path.display()))?;

    println!("File: {}", path.display());
    println!("Codes: {}", codes.len());
    println!();

    for code in &codes {
        match decode_packet(&code.packet_base64) {
            Ok(packet) => print_code_report(code, &packet),
            Err(error) => {
                println!("{} ({})", code.name, code.command);
                println!("  decode error: {error}");
                println!();
            }
        }
    }

    Ok(())
}

fn default_codes_path() -> PathBuf {
    ["broadlink-codes.json", "../broadlink-codes.json"]
        .into_iter()
        .map(PathBuf::from)
        .find(|candidate| candidate.exists())
        .unwrap_or_else(|| PathBuf::from("broadlink-codes.json"))
}

fn decode_packet(packet_base64: &str) -> Result<DecodedPacket, String> {
    let bytes = STANDARD
        .decode(packet_base64)
        .map_err(|error| format!("invalid base64: {error}"))?;
    let durations_us = broadlink_ir::decode(&bytes)?;
    let (frames, repeat_gaps_us) = mitsubishi_ir::decode_frames(&durations_us)?;
    Ok(DecodedPacket { durations_us, repeat_gaps_us, frames })
}

fn print_code_report(code: &BroadlinkCodeEntry, packet: &DecodedPacket) {
    println!("{} ({})", code.name, code.command);
    println!(
        "  packet: {} bytes, {} durations, {} frame(s)",
        code.packet_length,
        packet.durations_us.len(),
        packet.frames.len()
    );

    if !packet.repeat_gaps_us.is_empty() {
        println!("  repeat gaps: {}", join_u32(&packet.repeat_gaps_us));
    }

    let first_frame = &packet.frames[0];
    let repeated = packet
        .frames
        .iter()
        .skip(1)
        .all(|frame| frame.bytes == first_frame.bytes);

    println!(
        "  timings: header={} / {} (expected {} / {}), bit mark~{}, zero~{}, one~{}, footer~{}, repeat={} ; repeated={}",
        first_frame.header_mark_us,
        first_frame.header_space_us,
        MITSUBISHI_HDR_MARK_US,
        MITSUBISHI_HDR_SPACE_US,
        MITSUBISHI_BIT_MARK_US,
        MITSUBISHI_ZERO_SPACE_US,
        MITSUBISHI_ONE_SPACE_US,
        first_frame.footer_mark_us,
        MITSUBISHI_REPEAT_GAP_US,
        if repeated { "yes" } else { "no" }
    );
    println!("  raw: {}", util::hex(&first_frame.bytes, " "));
    println!(
        "  checksum: {:02X} ({})",
        first_frame.bytes[MITSUBISHI_STATE_LEN - 1],
        if mitsubishi_ir::checksum_valid(&first_frame.bytes) {
            "valid"
        } else {
            "invalid"
        }
    );

    let state = interpret_state(&first_frame.bytes);
    println!("  power: {}", on_off(state.power));
    println!("  mode: {} (0b{:03b})", mode_name(state.mode), state.mode);
    println!(
        "  temperature: {} C",
        format_temperature(state.temperature_half_degrees)
    );
    println!(
        "  fan: {} (code={}, auto={})",
        fan_name(state.fan_code, state.fan_auto),
        state.fan_code,
        on_off(state.fan_auto)
    );
    println!(
        "  vane vertical: {} (code={}, bit={})",
        vertical_vane_name(state.vertical_vane_code),
        state.vertical_vane_code,
        on_off(state.vertical_vane_enabled)
    );
    println!(
        "  vane horizontal: {} (code={})",
        horizontal_vane_name(state.horizontal_vane_code),
        state.horizontal_vane_code
    );
    println!(
        "  timers: current={}, clock={}, start={}, stop={}, weekly={}",
        timer_name(state.timer_mode),
        format_clock_value(state.clock),
        format_clock_value(state.start_clock),
        format_clock_value(state.stop_clock),
        on_off(state.weekly_timer)
    );
    println!(
        "  extras: i-see={}, econo={}, natural-flow={}, absence={}, i-save-10c={}, direct-indirect={}, left-vane={}",
        on_off(state.i_see),
        on_off(state.ecocool),
        on_off(state.natural_flow),
        on_off(state.absence_detect),
        on_off(state.i_save_10c),
        direct_indirect_name(state.direct_indirect),
        vertical_vane_name(state.left_vane_code)
    );
    println!();
}

#[derive(Debug)]
struct InterpretedState {
    power: bool,
    mode: u8,
    i_see: bool,
    temperature_half_degrees: u8,
    horizontal_vane_code: u8,
    fan_code: u8,
    fan_auto: bool,
    vertical_vane_code: u8,
    vertical_vane_enabled: bool,
    clock: u8,
    stop_clock: u8,
    start_clock: u8,
    timer_mode: u8,
    weekly_timer: bool,
    ecocool: bool,
    direct_indirect: u8,
    absence_detect: bool,
    i_save_10c: bool,
    natural_flow: bool,
    left_vane_code: u8,
}

fn interpret_state(bytes: &[u8; MITSUBISHI_STATE_LEN]) -> InterpretedState {
    InterpretedState {
        power: bytes[5] & 0x20 != 0,
        mode: (bytes[6] >> 3) & 0x07,
        i_see: bytes[6] & 0x40 != 0,
        temperature_half_degrees: ((bytes[7] & 0x0F) * 2)
            + if bytes[7] & 0x10 != 0 { 1 } else { 0 },
        horizontal_vane_code: bytes[8] >> 4,
        fan_code: bytes[9] & 0x07,
        vertical_vane_code: (bytes[9] >> 3) & 0x07,
        vertical_vane_enabled: bytes[9] & 0x40 != 0,
        fan_auto: bytes[9] & 0x80 != 0,
        clock: bytes[10],
        stop_clock: bytes[11],
        start_clock: bytes[12],
        timer_mode: bytes[13] & 0x07,
        weekly_timer: bytes[13] & 0x08 != 0,
        ecocool: bytes[14] & 0x20 != 0,
        direct_indirect: bytes[15] & 0x03,
        absence_detect: bytes[15] & 0x04 != 0,
        i_save_10c: bytes[15] & 0x20 != 0,
        natural_flow: bytes[16] & 0x02 != 0,
        left_vane_code: (bytes[16] >> 3) & 0x07,
    }
}

fn join_u32(values: &[u32]) -> String {
    values
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn on_off(enabled: bool) -> &'static str {
    if enabled {
        "on"
    } else {
        "off"
    }
}

fn mode_name(mode: u8) -> &'static str {
    match mode {
        0b100 => "auto",
        0b011 => "cool",
        0b010 => "dry",
        0b001 => "heat",
        0b111 => "fan",
        _ => "unknown",
    }
}

fn fan_name(fan_code: u8, fan_auto: bool) -> &'static str {
    if fan_auto {
        return "auto";
    }

    match fan_code {
        1 => "level-1",
        2 => "level-2",
        3 => "level-3",
        4 => "max",
        5 => "silent",
        _ => "unknown",
    }
}

fn vertical_vane_name(code: u8) -> &'static str {
    match code {
        0b000 => "auto",
        0b001 => "highest",
        0b010 => "high",
        0b011 => "middle",
        0b100 => "low",
        0b101 => "lowest",
        0b111 => "swing",
        _ => "unknown",
    }
}

fn horizontal_vane_name(code: u8) -> &'static str {
    match code {
        0b0001 => "far-left",
        0b0010 => "left",
        0b0011 => "center",
        0b0100 => "right",
        0b0101 => "far-right",
        0b0110 => "wide",
        0b1000 => "auto",
        _ => "unknown",
    }
}

fn timer_name(code: u8) -> &'static str {
    match code {
        0 => "none",
        3 => "stop",
        5 => "start",
        7 => "start+stop",
        _ => "unknown",
    }
}

fn direct_indirect_name(code: u8) -> &'static str {
    match code {
        0b00 => "off",
        0b01 => "indirect",
        0b11 => "direct",
        _ => "unknown",
    }
}

fn format_clock_value(raw_value: u8) -> String {
    if raw_value == 0 {
        return "--:--".to_string();
    }

    let total_minutes = (raw_value as u16) * 10;
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    format!("{hours:02}:{minutes:02}")
}

fn format_temperature(half_degrees: u8) -> String {
    let whole = 16 + (half_degrees / 2);
    if half_degrees.is_multiple_of(2) {
        whole.to_string()
    } else {
        format!("{whole}.5")
    }
}
