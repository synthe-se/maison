//! Reads the Mitsubishi codes learnt into `broadlink-codes.json` back into their fields,
//! to reverse-engineer the protocol from captures: the frames are read by `mitsubishi_ir`,
//! with the tables its encoder writes with; this only prints them.
//!
//!     cargo run --bin decode_mitsubishi_ir -- [path/to/broadlink-codes.json]
//!
//! Without a path: the backend's own (`BROADLINK_CODES_JSON_PATH`, else the repository's).

use std::{env, path::PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use maison_backend::{
    broadlink::{self, BroadlinkCodeEntry},
    broadlink_ir,
    config::Config,
    mitsubishi_ir::{
        self, MitsubishiFrame, Setting, MITSUBISHI_BIT_MARK_US, MITSUBISHI_HDR_MARK_US,
        MITSUBISHI_HDR_SPACE_US, MITSUBISHI_ONE_SPACE_US, MITSUBISHI_REPEAT_GAP_US,
        MITSUBISHI_STATE_LEN, MITSUBISHI_ZERO_SPACE_US,
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
        .unwrap_or_else(|| Config::from_env().broadlink_codes_path);
    let codes =
        broadlink::read_codes(&path).map_err(|error| format!("{}: {error}", path.display()))?;

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

fn decode_packet(packet_base64: &str) -> Result<DecodedPacket, String> {
    let bytes = STANDARD
        .decode(packet_base64)
        .map_err(|error| format!("invalid base64: {error}"))?;
    let durations_us = broadlink_ir::decode(&bytes)?;
    let (frames, repeat_gaps_us) = mitsubishi_ir::decode_frames(&durations_us)?;
    Ok(DecodedPacket {
        durations_us,
        repeat_gaps_us,
        frames,
    })
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

    let bytes = &first_frame.bytes;
    let state = mitsubishi_ir::read_state(bytes);
    println!("  power: {}", on_off(state.power));
    println!(
        "  mode: {} (0b{:03b})",
        name(state.mode),
        (bytes[6] >> 3) & 0x07
    );
    println!("  temperature: {} C", state.temperature_c());
    println!("  fan: {} (byte 9 = {:02X})", name(state.fan), bytes[9]);
    println!(
        "  vane vertical: {} (bit={})",
        name(state.vane),
        on_off(bytes[9] & 0x40 != 0)
    );
    println!(
        "  vane horizontal: {} (code={})",
        name(state.wide_vane),
        bytes[8] >> 4
    );
    println!(
        "  timers: current={}, clock={}, start={}, stop={}, weekly={}",
        name(state.timer),
        format_clock_value(state.clock),
        format_clock_value(state.start_clock),
        format_clock_value(state.stop_clock),
        on_off(state.weekly_timer)
    );
    println!(
        "  extras: i-see={}, econo={}, natural-flow={}, absence={}, i-save-10c={}, direct-indirect={}, left-vane={}",
        on_off(state.i_see),
        on_off(state.econo),
        on_off(state.natural_flow),
        on_off(state.absence_detect),
        on_off(state.i_save_10c),
        direct_indirect_name(state.direct_indirect),
        name(state.left_vane)
    );
    println!();
}

/// A value's command token, as the tables say it.
fn name<T: Setting>(value: Option<T>) -> &'static str {
    value.map_or("unknown", Setting::token)
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

fn direct_indirect_name(code: u8) -> &'static str {
    match code {
        0b00 => "off",
        0b01 => "indirect",
        0b11 => "direct",
        _ => "unknown",
    }
}

fn format_clock_value(ticks: u8) -> String {
    mitsubishi_ir::clock_of(ticks).map_or_else(
        || "--:--".to_string(),
        |(hours, minutes)| format!("{hours:02}:{minutes:02}"),
    )
}
