//! The Broadlink IR packet, said once: what the RM blasters send and what learning returns.
//!
//! A packet is a 4-byte header (the IR token, a repeat count, the little-endian length of
//! what follows), the pulse durations in ~32.84 µs ticks — one byte each, or `0x00` then
//! two big-endian bytes for 256 ticks and above — and the `0x00 0x0D` terminator.
//! Marks and spaces alternate from a mark, so only the durations are carried.

/// One Broadlink tick, in microseconds.
pub const TICK_US: f32 = 32.84;
/// First byte of an infrared packet (RF ones use other tokens).
pub const IR_TOKEN: u8 = 0x26;
/// Token, repeat count, length.
pub const HEADER_LEN: usize = 4;
const TERMINATOR: [u8; 2] = [0x00, 0x0D];

/// The packet for alternating mark/space `durations_us`, starting on a mark.
pub fn encode(durations_us: impl IntoIterator<Item = u32>) -> Vec<u8> {
    let mut packet = vec![IR_TOKEN, 0x00, 0x00, 0x00];
    for micros in durations_us {
        let ticks = ((micros as f32) / TICK_US).round() as u16;
        if ticks >= 256 {
            packet.push(0x00);
            packet.extend_from_slice(&ticks.to_be_bytes());
        } else {
            packet.push(ticks as u8);
        }
    }
    packet.extend_from_slice(&TERMINATOR);
    // the declared length covers the durations and the terminator, plus one: the
    // blaster truncates the frame otherwise
    let encoded_len = (packet.len() - HEADER_LEN + 1) as u16;
    packet[2..4].copy_from_slice(&encoded_len.to_le_bytes());
    packet
}

/// The mark/space durations of an infrared packet, in microseconds.
pub fn decode(packet: &[u8]) -> Result<Vec<u32>, String> {
    if packet.len() <= HEADER_LEN {
        return Err("packet too short".to_string());
    }
    if packet[0] != IR_TOKEN {
        return Err(format!("unsupported Broadlink packet token 0x{:02X}", packet[0]));
    }
    let encoded = &packet[HEADER_LEN..];
    let mut durations_us = Vec::new();
    let mut index = 0;
    while index < encoded.len() {
        match encoded[index] {
            0 if encoded[index..].starts_with(&TERMINATOR) && encoded.len() - index == 2 => break,
            0 => {
                let pair = encoded
                    .get(index + 1..index + 3)
                    .ok_or_else(|| "truncated extended Broadlink duration".to_string())?;
                durations_us.push(ticks_to_micros(u16::from_be_bytes([pair[0], pair[1]]).into()));
                index += 3;
            }
            ticks => {
                durations_us.push(ticks_to_micros(ticks.into()));
                index += 1;
            }
        }
    }
    Ok(durations_us)
}

fn ticks_to_micros(ticks: u32) -> u32 {
    ((ticks as f32) * TICK_US).round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_round_trip_to_the_tick() {
        let durations = [889, 1778, 3400, 15500, 420];
        let packet = encode(durations);
        assert_eq!(packet[0], IR_TOKEN);
        assert_eq!(&packet[packet.len() - 2..], &TERMINATOR);
        let declared = usize::from(u16::from_le_bytes([packet[2], packet[3]]));
        assert_eq!(declared, packet.len() - HEADER_LEN + 1);
        let decoded = decode(&packet).expect("decodes");
        assert_eq!(decoded.len(), durations.len());
        for (sent, back) in durations.iter().zip(&decoded) {
            assert!(sent.abs_diff(*back) <= 17, "{sent} came back as {back}");
        }
    }

    #[test]
    fn long_pulses_take_three_bytes() {
        // 15500 µs = 472 ticks: 0x00 then 0x01D8, big-endian
        assert_eq!(&encode([15500])[4..7], &[0x00, 0x01, 0xD8]);
    }

    #[test]
    fn foreign_or_torn_packets_are_refused() {
        assert!(decode(&[0xB2, 0, 0, 0, 1]).is_err());
        assert!(decode(&[IR_TOKEN, 0, 0]).is_err());
        assert!(decode(&[IR_TOKEN, 0, 0, 0, 0x00, 0x01]).is_err());
    }
}
