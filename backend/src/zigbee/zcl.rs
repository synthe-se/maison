//! The Zigbee Cluster Library and ZDO as far as the lamps and the Hue dimmer need them: the
//! identifiers, the frames sent, and the frames read. Pure functions: no EZSP here.

use std::str::FromStr;

use tracing::debug;

use super::device::{DiscoveredDevice, ZigbeeDeviceType};
use crate::error::AppError;

pub const ZDO_PROFILE_ID: u16 = 0x0000;
pub const HOME_AUTOMATION_PROFILE_ID: u16 = 0x0104;
/// Our own endpoint on the coordinator.
pub const DEFAULT_SOURCE_ENDPOINT: u8 = 1;

pub const BASIC_CLUSTER_ID: u16 = 0x0000;
pub const IDENTIFY_CLUSTER_ID: u16 = 0x0003;
pub const ON_OFF_CLUSTER_ID: u16 = 0x0006;
pub const LEVEL_CONTROL_CLUSTER_ID: u16 = 0x0008;
pub const COLOR_CONTROL_CLUSTER_ID: u16 = 0x0300;
/// Philips manufacturer-specific cluster used by the Hue remotes for their button
/// notifications (`hueNotification`).
pub const PHILIPS_SPECIFIC_CLUSTER_ID: u16 = 0xFC00;
/// Philips manufacturer-specific cluster for the Hue effects (candle, fireplace…).
const PHILIPS_EFFECTS_CLUSTER_ID: u16 = 0xFC03;
const PHILIPS_MANUFACTURER_CODE: u16 = 0x100B;
const PHILIPS_MULTI_COLOR_COMMAND_ID: u8 = 0x00;

pub const SIMPLE_DESC_REQ_CLUSTER_ID: u16 = 0x0004;
pub const ACTIVE_EP_REQ_CLUSTER_ID: u16 = 0x0005;
pub const DEVICE_ANNCE_CLUSTER_ID: u16 = 0x0013;
pub const BIND_REQ_CLUSTER_ID: u16 = 0x0021;
pub const SIMPLE_DESC_RSP_CLUSTER_ID: u16 = 0x8004;
pub const ACTIVE_EP_RSP_CLUSTER_ID: u16 = 0x8005;

pub const ZCL_GLOBAL_FRAME_CONTROL: u8 = 0x00;
pub const ZCL_CLUSTER_COMMAND_FRAME_CONTROL: u8 = 0x11;
/// Manufacturer-specific cluster command, client → server.
const ZCL_MANU_CLUSTER_COMMAND_FRAME_CONTROL: u8 = 0x05;
/// Frame control bit 2: a manufacturer code follows the frame control byte.
const ZCL_MANUFACTURER_SPECIFIC_FLAG: u8 = 0x04;
pub const ZCL_READ_ATTRIBUTES_COMMAND_ID: u8 = 0x00;
const ZCL_READ_ATTRIBUTES_RESPONSE_COMMAND_ID: u8 = 0x01;
const ZCL_REPORT_ATTRIBUTES_COMMAND_ID: u8 = 0x0a;

pub const ZCL_ON_OFF_COMMAND_OFF: u8 = 0x00;
pub const ZCL_ON_OFF_COMMAND_ON: u8 = 0x01;
const ZCL_ON_OFF_COMMAND_TOGGLE: u8 = 0x02;
pub const ZCL_LEVEL_CONTROL_COMMAND_MOVE: u8 = 0x01;
pub const ZCL_LEVEL_CONTROL_COMMAND_STEP: u8 = 0x02;
pub const ZCL_LEVEL_CONTROL_COMMAND_STOP: u8 = 0x03;
/// Move to Level (with On/Off): the level also switches the lamp on or off.
pub const ZCL_LEVEL_CONTROL_COMMAND_MOVE_TO_LEVEL: u8 = 0x04;
pub const ZCL_LEVEL_CONTROL_COMMAND_MOVE_WITH_ON_OFF: u8 = 0x05;
pub const ZCL_LEVEL_CONTROL_COMMAND_STEP_WITH_ON_OFF: u8 = 0x06;
pub const ZCL_LEVEL_CONTROL_COMMAND_STOP_WITH_ON_OFF: u8 = 0x07;
const ZCL_COLOR_CONTROL_COMMAND_MOVE_TO_COLOR: u8 = 0x07;
pub const ZCL_COLOR_CONTROL_COMMAND_MOVE_TO_COLOR_TEMPERATURE: u8 = 0x0a;
const ZCL_IDENTIFY_TRIGGER_EFFECT_COMMAND_ID: u8 = 0x40;

/// ZCL colorMode values.
pub const COLOR_MODE_XY: u8 = 1;
pub const COLOR_MODE_TEMPERATURE: u8 = 2;

/// Colour temperature range of the API's 0–100 scale, in mireds: 0 % is the warmest
/// (500 mireds, 2000 K), 100 % the coolest (153 mireds, 6500 K). The web mirrors them
/// (web/src/lib/devices/lamps/lamp.ts).
pub const MIRED_WARM: u16 = 500;
pub const MIRED_COOL: u16 = 153;

// ------------------------------------------------------------------ frames sent --

pub fn build_on_off_command_payload(enabled: bool, sequence: u8) -> Vec<u8> {
    let command = if enabled { ZCL_ON_OFF_COMMAND_ON } else { ZCL_ON_OFF_COMMAND_OFF };
    vec![ZCL_CLUSTER_COMMAND_FRAME_CONTROL, sequence, command]
}

pub fn brightness_percent_to_level(brightness: u8) -> u8 {
    ((u16::from(brightness.min(100)) * 254) / 100).max(1) as u8
}

/// A ZCL level (0–254) as the API's 0–100.
pub fn level_to_brightness_percent(level: u8) -> u8 {
    ((u16::from(level) * 100) / 254).min(100) as u8
}

pub fn build_brightness_command_payload(brightness: u8, sequence: u8) -> Vec<u8> {
    let level = brightness_percent_to_level(brightness);
    vec![ZCL_CLUSTER_COMMAND_FRAME_CONTROL, sequence, ZCL_LEVEL_CONTROL_COMMAND_MOVE_TO_LEVEL, level, 0x00, 0x00]
}

pub fn temperature_percent_to_mireds(temperature: u8) -> u16 {
    MIRED_WARM.saturating_sub((u16::from(temperature.min(100)) * (MIRED_WARM - MIRED_COOL)) / 100)
}

pub fn mireds_to_temperature_percent(mireds: u16) -> u8 {
    let percent = (MIRED_WARM.saturating_sub(mireds.min(MIRED_WARM)) * 100) / (MIRED_WARM - MIRED_COOL);
    percent.min(100) as u8
}

pub fn build_color_temperature_command_payload(temperature: u8, sequence: u8) -> Vec<u8> {
    let [low, high] = temperature_percent_to_mireds(temperature).to_le_bytes();
    vec![ZCL_CLUSTER_COMMAND_FRAME_CONTROL, sequence, ZCL_COLOR_CONTROL_COMMAND_MOVE_TO_COLOR_TEMPERATURE, low, high, 0x00, 0x00]
}

/// `Move to Color` (Color Control): CIE 1931 `x`, `y` (0.0–1.0) scaled to 0–65535, at once.
pub fn build_color_xy_command_payload(x: f32, y: f32, sequence: u8) -> Vec<u8> {
    let [x_low, x_high] = ((x.clamp(0.0, 1.0) * 65535.0) as u16).to_le_bytes();
    let [y_low, y_high] = ((y.clamp(0.0, 1.0) * 65535.0) as u16).to_le_bytes();
    vec![ZCL_CLUSTER_COMMAND_FRAME_CONTROL, sequence, ZCL_COLOR_CONTROL_COMMAND_MOVE_TO_COLOR, x_low, x_high, y_low, y_high, 0x00, 0x00]
}

pub fn build_read_attributes_payload(attributes: &[u16], sequence: u8) -> Vec<u8> {
    let mut payload = vec![ZCL_GLOBAL_FRAME_CONTROL, sequence, ZCL_READ_ATTRIBUTES_COMMAND_ID];
    payload.extend(attributes.iter().flat_map(|attribute| attribute.to_le_bytes()));
    payload
}

/// A lamp effect: the Identify cluster's standard ones, Philips' own on the Hue lamps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZigbeeEffect {
    Blink,
    Breathe,
    Okay,
    ChannelChange,
    FinishEffect,
    StopEffect,
    Candle,
    Fireplace,
    Colorloop,
    Sunrise,
    Sparkle,
    Opal,
    Glisten,
    StopHueEffect,
}

impl ZigbeeEffect {
    pub const ALL: [ZigbeeEffect; 14] = [
        Self::Blink,
        Self::Breathe,
        Self::Okay,
        Self::ChannelChange,
        Self::FinishEffect,
        Self::StopEffect,
        Self::Candle,
        Self::Fireplace,
        Self::Colorloop,
        Self::Sunrise,
        Self::Sparkle,
        Self::Opal,
        Self::Glisten,
        Self::StopHueEffect,
    ];

    /// The name the API takes.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Blink => "blink",
            Self::Breathe => "breathe",
            Self::Okay => "okay",
            Self::ChannelChange => "channel_change",
            Self::FinishEffect => "finish_effect",
            Self::StopEffect => "stop_effect",
            Self::Candle => "candle",
            Self::Fireplace => "fireplace",
            Self::Colorloop => "colorloop",
            Self::Sunrise => "sunrise",
            Self::Sparkle => "sparkle",
            Self::Opal => "opal",
            Self::Glisten => "glisten",
            Self::StopHueEffect => "stop_hue_effect",
        }
    }

    /// The cluster it is sent on.
    pub fn cluster(self) -> u16 {
        self.frame(0).0
    }

    /// The cluster it is sent on, and its frame.
    pub fn frame(self, sequence: u8) -> (u16, Vec<u8>) {
        let identify = |effect_id: u8| {
            // effect variant 0: the default
            let frame = vec![ZCL_CLUSTER_COMMAND_FRAME_CONTROL, sequence, ZCL_IDENTIFY_TRIGGER_EFFECT_COMMAND_ID, effect_id, 0x00];
            (IDENTIFY_CLUSTER_ID, frame)
        };
        // Philips multiColor: start `[0x21, 0x00, 0x01, effect]`, stop `[0x20, 0x00, 0x00]`
        let hue = |body: &[u8]| {
            let [code_low, code_high] = PHILIPS_MANUFACTURER_CODE.to_le_bytes();
            let mut frame = vec![ZCL_MANU_CLUSTER_COMMAND_FRAME_CONTROL, code_low, code_high, sequence, PHILIPS_MULTI_COLOR_COMMAND_ID];
            frame.extend_from_slice(body);
            (PHILIPS_EFFECTS_CLUSTER_ID, frame)
        };
        match self {
            Self::Blink => identify(0x00),
            Self::Breathe => identify(0x01),
            Self::Okay => identify(0x02),
            Self::ChannelChange => identify(0x0B),
            Self::FinishEffect => identify(0xFE),
            Self::StopEffect => identify(0xFF),
            Self::Candle => hue(&[0x21, 0x00, 0x01, 0x01]),
            Self::Fireplace => hue(&[0x21, 0x00, 0x01, 0x02]),
            Self::Colorloop => hue(&[0x21, 0x00, 0x01, 0x03]),
            Self::Sunrise => hue(&[0x21, 0x00, 0x01, 0x09]),
            Self::Sparkle => hue(&[0x21, 0x00, 0x01, 0x0A]),
            Self::Opal => hue(&[0x21, 0x00, 0x01, 0x0B]),
            Self::Glisten => hue(&[0x21, 0x00, 0x01, 0x0C]),
            Self::StopHueEffect => hue(&[0x20, 0x00, 0x00]),
        }
    }
}

impl FromStr for ZigbeeEffect {
    type Err = AppError;

    fn from_str(name: &str) -> Result<Self, AppError> {
        Self::ALL.into_iter().find(|effect| effect.as_str().eq_ignore_ascii_case(name)).ok_or_else(|| {
            let known = Self::ALL.map(Self::as_str).join(", ");
            AppError::bad_request(format!("Unknown effect. Supported: {known}"))
        })
    }
}

// ------------------------------------------------------------------ frames read --

/// A ZCL frame's header: `[frame control, (manufacturer code ×2), sequence, command]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZclHeader {
    /// Cluster-specific (frame type 01) rather than global (00, e.g. read responses).
    pub cluster_specific: bool,
    pub manufacturer: Option<u16>,
    pub command_id: u8,
    /// Where the command's payload starts.
    pub body: usize,
}

pub fn parse_zcl_header(payload: &[u8]) -> Option<ZclHeader> {
    let frame_control = *payload.first()?;
    let cluster_specific = match frame_control & 0x03 {
        0x00 => false,
        0x01 => true,
        _ => return None,
    };
    let (manufacturer, command_at) = if frame_control & ZCL_MANUFACTURER_SPECIFIC_FLAG != 0 {
        (Some(u16::from_le_bytes([*payload.get(1)?, *payload.get(2)?])), 4)
    } else {
        (None, 2)
    };
    Some(ZclHeader { cluster_specific, manufacturer, command_id: *payload.get(command_at)?, body: command_at + 1 })
}

/// What a lamp's ZCL frame says about it: attribute reads and reports, and the On/Off and
/// Level commands it echoes. The frame type is read before the command id (a global Write
/// Attributes Response is 0x04, like Move to Level; a Read Attributes Response is 0x01,
/// like On), and manufacturer-specific frames are not ours to read. Says whether the frame
/// was a well-formed standard one: the lamp answered, so it is alive.
pub fn apply_lamp_frame(device: &mut DiscoveredDevice, cluster_id: u16, payload: &[u8]) -> bool {
    let Some(header) = parse_zcl_header(payload) else {
        return false;
    };
    if header.manufacturer.is_some() {
        return false;
    }
    let body = &payload[header.body..];

    if !header.cluster_specific {
        let records = match header.command_id {
            ZCL_READ_ATTRIBUTES_RESPONSE_COMMAND_ID => parse_attribute_records(body, true),
            ZCL_REPORT_ATTRIBUTES_COMMAND_ID => parse_attribute_records(body, false),
            // a default or write response: nothing to read, but an answer all the same
            _ => Vec::new(),
        };
        for (attribute_id, value) in records {
            apply_attribute(device, cluster_id, attribute_id, value);
        }
        return true;
    }

    let light = &mut device.light;
    match (cluster_id, header.command_id) {
        (ON_OFF_CLUSTER_ID, ZCL_ON_OFF_COMMAND_OFF) => light.is_on = false,
        (ON_OFF_CLUSTER_ID, ZCL_ON_OFF_COMMAND_ON) => light.is_on = true,
        (ON_OFF_CLUSTER_ID, ZCL_ON_OFF_COMMAND_TOGGLE) => light.is_on = !light.is_on,
        (LEVEL_CONTROL_CLUSTER_ID, ZCL_LEVEL_CONTROL_COMMAND_MOVE_TO_LEVEL) => {
            if let Some(&level) = body.first() {
                light.brightness = level_to_brightness_percent(level);
                light.is_on = level > 0;
            }
        }
        _ => {}
    }
    true
}

/// The successful records of a Read Attributes Response (`with_status`) or of an Attribute
/// Report: `(attribute, value bytes)`. An error record (status ≠ 0) has no type nor value
/// and is skipped; a type of unknown length ends the parse.
pub fn parse_attribute_records(body: &[u8], with_status: bool) -> Vec<(u16, &[u8])> {
    let mut records = Vec::new();
    let mut rest = body;
    while let [low, high, after @ ..] = rest {
        let attribute_id = u16::from_le_bytes([*low, *high]);
        rest = after;
        if with_status {
            let Some((&status, after)) = rest.split_first() else { break };
            rest = after;
            if status != 0 {
                continue;
            }
        }
        let Some((&data_type, after)) = rest.split_first() else { break };
        let Some((length, value)) = parse_zcl_attribute_value(data_type, after) else { break };
        records.push((attribute_id, value));
        rest = &after[length..];
    }
    records
}

fn apply_attribute(device: &mut DiscoveredDevice, cluster_id: u16, attribute_id: u16, value: &[u8]) {
    let word = || match value {
        [low, high, ..] => Some(u16::from_le_bytes([*low, *high])),
        _ => None,
    };
    let text = || String::from_utf8(value.to_vec()).ok();
    let light = &mut device.light;
    match (cluster_id, attribute_id) {
        (ON_OFF_CLUSTER_ID, 0x0000) => {
            if let Some(value) = value.first() {
                light.is_on = *value != 0;
            }
        }
        (LEVEL_CONTROL_CLUSTER_ID, 0x0000) => {
            if let Some(&level) = value.first() {
                light.brightness = level_to_brightness_percent(level);
            }
        }
        (COLOR_CONTROL_CLUSTER_ID, 0x0003) => light.colour.color_x = word().map(|raw| f32::from(raw) / 65535.0).or(light.colour.color_x),
        (COLOR_CONTROL_CLUSTER_ID, 0x0004) => light.colour.color_y = word().map(|raw| f32::from(raw) / 65535.0).or(light.colour.color_y),
        (COLOR_CONTROL_CLUSTER_ID, 0x0007) => {
            if let Some(raw) = word() {
                device.info.supports_temperature = true;
                light.temperature = Some(mireds_to_temperature_percent(raw));
            }
        }
        (COLOR_CONTROL_CLUSTER_ID, 0x0008) => light.colour.color_mode = value.first().copied().or(light.colour.color_mode),
        // colorCapabilities: bit 0 hue/saturation, bit 3 XY, bit 4 colour temperature
        (COLOR_CONTROL_CLUSTER_ID, 0x400A) => {
            if let Some(caps) = word() {
                device.info.supports_color = (caps & 0x08) != 0;
                debug!(
                    node_id = format_args!("0x{:04x}", device.node_id),
                    caps = format_args!("0x{caps:04x}"),
                    supports_xy = device.info.supports_color,
                    "parsed colorCapabilities"
                );
            }
        }
        (BASIC_CLUSTER_ID, 0x0004) => device.info.manufacturer = text().or(device.info.manufacturer.take()),
        (BASIC_CLUSTER_ID, 0x0005) => device.info.model = text().or(device.info.model.take()),
        _ => {}
    }
}

fn parse_zcl_attribute_value(data_type: u8, payload: &[u8]) -> Option<(usize, &[u8])> {
    match data_type {
        // 1-byte: boolean (0x10), 8-bit bitmap (0x18), uint8 (0x20), enum8 (0x30)
        0x10 | 0x18 | 0x20 | 0x30 => payload.first().map(|_| (1, &payload[..1])),
        // 2-byte: 16-bit bitmap (0x19), uint16 (0x21), enum16 (0x31)
        // (`then`, not `then_some`: the slice must only be taken when it fits)
        0x19 | 0x21 | 0x31 => (payload.len() >= 2).then(|| (2, &payload[..2])),
        // length-prefixed octet string
        0x42 => {
            let len = *payload.first()? as usize;
            (payload.len() > len).then(|| (1 + len, &payload[1..1 + len]))
        }
        _ => None,
    }
}

pub fn parse_active_ep_response(payload: &[u8]) -> Option<Vec<u8>> {
    if payload.len() < 5 || payload[1] != 0 {
        return None;
    }
    let count = payload[4] as usize;
    payload.get(5..5 + count).map(<[u8]>::to_vec)
}

pub struct DeviceAnnouncement {
    pub node_id: u16,
    pub eui64: String,
}

pub fn parse_device_announce(payload: &[u8]) -> Option<DeviceAnnouncement> {
    if payload.len() < 11 {
        return None;
    }
    let node_id = u16::from_le_bytes([payload[1], payload[2]]);
    // ZDP sends the IEEE address least significant byte first; it is shown (and matched)
    // most significant first, as ChildJoin and TrustCenterJoin give it.
    let mut eui64 = [0_u8; 8];
    eui64.copy_from_slice(&payload[3..11]);
    eui64.reverse();
    Some(DeviceAnnouncement { node_id, eui64: format_eui64(eui64) })
}

pub fn format_eui64(eui64: [u8; 8]) -> String {
    crate::util::hex(&eui64, ":")
}

/// A colon-separated EUI64 (`00:17:88:01:08:0c:00:0b`), most significant byte first.
pub fn parse_eui64(eui64: &str) -> Option<[u8; 8]> {
    let mut bytes = [0_u8; 8];
    let mut parts = eui64.split(':');
    for byte in &mut bytes {
        *byte = u8::from_str_radix(parts.next()?, 16).ok()?;
    }
    parts.next().is_none().then_some(bytes)
}

pub struct SimpleDescriptor {
    pub endpoint: u8,
    pub profile_id: u16,
    pub device_id: u16,
    pub input_clusters: Vec<u16>,
    pub output_clusters: Vec<u16>,
}

pub fn parse_simple_desc_response(payload: &[u8]) -> Option<SimpleDescriptor> {
    if payload.len() < 8 || payload[1] != 0 {
        return None;
    }
    let descriptor_length = payload[4] as usize;
    if payload.len() < 5 + descriptor_length || descriptor_length < 8 {
        return None;
    }
    let descriptor = &payload[5..5 + descriptor_length];
    let word = |at: usize| u16::from_le_bytes([descriptor[at], descriptor[at + 1]]);
    let (input_clusters, after) = parse_cluster_list(descriptor, 6)?;
    let (output_clusters, _) = parse_cluster_list(descriptor, after)?;
    Some(SimpleDescriptor { endpoint: descriptor[0], profile_id: word(1), device_id: word(3), input_clusters, output_clusters })
}

/// A count then that many clusters, from `offset`; and where the list ends.
fn parse_cluster_list(descriptor: &[u8], offset: usize) -> Option<(Vec<u16>, usize)> {
    let count = *descriptor.get(offset)? as usize;
    let start = offset + 1;
    let bytes = descriptor.get(start..start + count * 2)?;
    let clusters = bytes.as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
    Some((clusters, start + count * 2))
}

pub fn is_preferred_light_endpoint(description: &SimpleDescriptor) -> bool {
    description.profile_id == HOME_AUTOMATION_PROFILE_ID
        && [ON_OFF_CLUSTER_ID, LEVEL_CONTROL_CLUSTER_ID, COLOR_CONTROL_CLUSTER_ID]
            .iter()
            .any(|cluster| description.input_clusters.contains(cluster))
}

/// A remote or dimmer switch: home-automation profile, and a controller device id (0x0820
/// non-colour, 0x0830 colour, 0x0840 scene controller, 0x0006 remote) or On/Off or Level
/// in its output clusters but not in its input ones (it sends light commands, it does not
/// receive them).
pub fn classify_device_type(description: &SimpleDescriptor) -> ZigbeeDeviceType {
    if description.profile_id != HOME_AUTOMATION_PROFILE_ID {
        return ZigbeeDeviceType::Unknown;
    }
    let is_controller_device = matches!(description.device_id, 0x0820 | 0x0830 | 0x0840 | 0x0006);
    let light_clusters = [ON_OFF_CLUSTER_ID, LEVEL_CONTROL_CLUSTER_ID];
    let sends_light_commands = light_clusters.iter().any(|cluster| description.output_clusters.contains(cluster));
    let receives_light_commands = light_clusters.iter().any(|cluster| description.input_clusters.contains(cluster));

    if is_controller_device || (sends_light_commands && !receives_light_commands) {
        ZigbeeDeviceType::Remote
    } else if is_preferred_light_endpoint(description) {
        ZigbeeDeviceType::Lamp
    } else {
        ZigbeeDeviceType::Unknown
    }
}

pub fn hex_bytes(bytes: &[u8]) -> String {
    crate::util::hex(bytes, " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lamp() -> DiscoveredDevice {
        DiscoveredDevice::test_lamp()
    }

    #[test]
    fn on_off_payload_uses_cluster_command_frame() {
        assert_eq!(build_on_off_command_payload(false, 0x34), vec![ZCL_CLUSTER_COMMAND_FRAME_CONTROL, 0x34, ZCL_ON_OFF_COMMAND_OFF]);
        assert_eq!(build_on_off_command_payload(true, 0x35), vec![ZCL_CLUSTER_COMMAND_FRAME_CONTROL, 0x35, ZCL_ON_OFF_COMMAND_ON]);
    }

    #[test]
    fn brightness_payload_maps_percent_to_move_to_level() {
        assert_eq!(brightness_percent_to_level(0), 1);
        assert_eq!(brightness_percent_to_level(50), 127);
        assert_eq!(brightness_percent_to_level(100), 254);
        assert_eq!(
            build_brightness_command_payload(50, 0x22),
            vec![ZCL_CLUSTER_COMMAND_FRAME_CONTROL, 0x22, ZCL_LEVEL_CONTROL_COMMAND_MOVE_TO_LEVEL, 127, 0x00, 0x00]
        );
    }

    #[test]
    fn color_temperature_payload_maps_percent_to_mireds() {
        assert_eq!(temperature_percent_to_mireds(0), 500);
        assert_eq!(temperature_percent_to_mireds(100), 153);
        assert_eq!(temperature_percent_to_mireds(50), 327);
        assert_eq!(
            build_color_temperature_command_payload(50, 0x44),
            vec![ZCL_CLUSTER_COMMAND_FRAME_CONTROL, 0x44, ZCL_COLOR_CONTROL_COMMAND_MOVE_TO_COLOR_TEMPERATURE, 0x47, 0x01, 0x00, 0x00]
        );
    }

    #[test]
    fn mireds_and_percent_agree_at_the_ends() {
        assert_eq!(mireds_to_temperature_percent(MIRED_WARM), 0);
        assert_eq!(mireds_to_temperature_percent(MIRED_COOL), 100);
        assert_eq!(mireds_to_temperature_percent(600), 0, "warmer than the range");
        assert_eq!(mireds_to_temperature_percent(100), 100, "cooler than the range");
        assert_eq!(temperature_percent_to_mireds(0), MIRED_WARM);
        assert_eq!(temperature_percent_to_mireds(100), MIRED_COOL);
    }

    #[test]
    fn colour_and_read_payloads() {
        assert_eq!(build_color_xy_command_payload(1.0, 0.0, 7), vec![0x11, 7, 0x07, 0xff, 0xff, 0, 0, 0, 0]);
        assert_eq!(build_read_attributes_payload(&[0x0004, 0x0005], 9), vec![0x00, 9, 0x00, 0x04, 0x00, 0x05, 0x00]);
    }

    #[test]
    fn every_effect_round_trips_by_name_and_keeps_its_frame() {
        for effect in ZigbeeEffect::ALL {
            assert_eq!(effect.as_str().parse::<ZigbeeEffect>().unwrap(), effect);
            assert_eq!(effect.as_str().to_uppercase().parse::<ZigbeeEffect>().unwrap(), effect, "any case");
        }
        assert!("disco".parse::<ZigbeeEffect>().is_err());
        assert_eq!(ZigbeeEffect::Breathe.frame(3), (IDENTIFY_CLUSTER_ID, vec![0x11, 3, 0x40, 0x01, 0x00]));
        assert_eq!(
            ZigbeeEffect::Candle.frame(4),
            (PHILIPS_EFFECTS_CLUSTER_ID, vec![0x05, 0x0b, 0x10, 4, 0x00, 0x21, 0x00, 0x01, 0x01])
        );
        assert_eq!(ZigbeeEffect::StopHueEffect.frame(5).1[5..], [0x20, 0x00, 0x00]);
    }

    #[test]
    fn zcl_headers_tell_global_from_cluster_specific_and_skip_the_manufacturer_code() {
        assert_eq!(
            parse_zcl_header(&[0x18, 0x10, 0x01, 0x00]),
            Some(ZclHeader { cluster_specific: false, manufacturer: None, command_id: 0x01, body: 3 })
        );
        assert_eq!(
            parse_zcl_header(&[0x01, 0x10, 0x04, 0x7f]),
            Some(ZclHeader { cluster_specific: true, manufacturer: None, command_id: 0x04, body: 3 })
        );
        assert_eq!(
            parse_zcl_header(&[0x1d, 0x0b, 0x10, 0x22, 0x00, 0x01]),
            Some(ZclHeader { cluster_specific: true, manufacturer: Some(0x100b), command_id: 0x00, body: 5 })
        );
        assert_eq!(parse_zcl_header(&[0x18, 0x10]), None, "too short");
        assert_eq!(parse_zcl_header(&[0x1d, 0x0b]), None, "manufacturer code cut");
        assert_eq!(parse_zcl_header(&[0x03, 0x00, 0x00]), None, "reserved frame type");
    }

    #[test]
    fn a_read_error_reply_does_not_switch_a_lamp_on() {
        let mut device = lamp();
        device.light.is_on = false;
        // Read Attributes Response, OnOff (0x0000), status 0x86 (unsupported attribute)
        assert!(apply_lamp_frame(&mut device, ON_OFF_CLUSTER_ID, &[0x18, 0x10, 0x01, 0x00, 0x00, 0x86]));
        assert!(!device.light.is_on, "status 0x86 read as « on »");
        // the same read, answered: on
        assert!(apply_lamp_frame(&mut device, ON_OFF_CLUSTER_ID, &[0x18, 0x11, 0x01, 0x00, 0x00, 0x00, 0x10, 0x01]));
        assert!(device.light.is_on);
        // an attribute report (no status byte): off
        assert!(apply_lamp_frame(&mut device, ON_OFF_CLUSTER_ID, &[0x18, 0x12, 0x0a, 0x00, 0x00, 0x10, 0x00]));
        assert!(!device.light.is_on);
        // cluster-specific On, then Toggle
        apply_lamp_frame(&mut device, ON_OFF_CLUSTER_ID, &[0x01, 0x13, 0x01]);
        assert!(device.light.is_on);
        apply_lamp_frame(&mut device, ON_OFF_CLUSTER_ID, &[0x01, 0x14, 0x02]);
        assert!(!device.light.is_on);
    }

    #[test]
    fn a_write_ack_is_not_a_move_to_level() {
        let mut device = lamp();
        let light = |device: &DiscoveredDevice| (device.light.brightness, device.light.is_on);
        // Write Attributes Response (global 0x04), status success
        assert!(apply_lamp_frame(&mut device, LEVEL_CONTROL_CLUSTER_ID, &[0x18, 0x20, 0x04, 0x00]));
        assert_eq!(light(&device), (100, true), "a write ack turned the lamp off");
        // a Default Response neither
        apply_lamp_frame(&mut device, LEVEL_CONTROL_CLUSTER_ID, &[0x18, 0x21, 0x0b, 0x04, 0x00]);
        assert_eq!(light(&device), (100, true));
        // the real Move to Level (with On/Off), level 127 → 50 %
        assert!(apply_lamp_frame(&mut device, LEVEL_CONTROL_CLUSTER_ID, &[0x01, 0x22, 0x04, 127, 0x00, 0x00]));
        assert_eq!(light(&device), (50, true));
        // a manufacturer-specific frame is not read
        assert!(!apply_lamp_frame(&mut device, LEVEL_CONTROL_CLUSTER_ID, &[0x05, 0x0b, 0x10, 0x23, 0x04, 0x00]));
        assert_eq!(device.light.brightness, 50);
    }

    #[test]
    fn read_responses_skip_error_records_and_read_the_rest() {
        // manufacturer (0x0004): unsupported; model (0x0005): "LTG"
        let body = [0x04, 0x00, 0x86, 0x05, 0x00, 0x00, 0x42, 0x03, b'L', b'T', b'G'];
        assert_eq!(parse_attribute_records(&body, true), vec![(0x0005, &b"LTG"[..])]);
        let mut device = lamp();
        let mut frame = vec![0x18, 0x30, 0x01];
        frame.extend_from_slice(&body);
        apply_lamp_frame(&mut device, BASIC_CLUSTER_ID, &frame);
        assert_eq!(device.info.model.as_deref(), Some("LTG"));
        assert_eq!(device.info.manufacturer.as_deref(), Some("Signify Netherlands B.V."), "kept");

        // colour temperature 153 mireds (coolest) and capabilities with XY
        let colour = [0x18, 0x31, 0x01, 0x07, 0x00, 0x00, 0x21, 153, 0x00, 0x0a, 0x40, 0x00, 0x19, 0x18, 0x00];
        apply_lamp_frame(&mut device, COLOR_CONTROL_CLUSTER_ID, &colour);
        assert_eq!(device.light.temperature, Some(100));
        assert!(device.info.supports_temperature && device.info.supports_color);

        // a type of unknown length ends the parse, a cut record too
        assert!(parse_attribute_records(&[0x00, 0x00, 0x00, 0xff, 0x01], true).is_empty());
        assert!(parse_attribute_records(&[0x00, 0x00, 0x00, 0x21, 0x01], true).is_empty());
        assert!(parse_attribute_records(&[0x00], true).is_empty());
        assert!(parse_attribute_records(&[0x05, 0x00, 0x00, 0x42, 0x05, b'L'], true).is_empty(), "a cut string");
    }

    #[test]
    fn simple_descriptors_are_parsed_and_bad_ones_refused() {
        // seq, status, nwk addr, length 18, then: endpoint 11, profile 0x0104, device 0x010c,
        // version, 4 inputs (Basic, On/Off, Level, Colour), 1 output (OTA)
        let payload = [
            0x05, 0x00, 0x34, 0x2e, 18, 11, 0x04, 0x01, 0x0c, 0x01, 0x01, 4, 0x00, 0x00, 0x06, 0x00, 0x08, 0x00,
            0x00, 0x03, 1, 0x19, 0x00,
        ];
        let descriptor = parse_simple_desc_response(&payload).expect("a valid descriptor");
        assert_eq!(descriptor.endpoint, 11);
        assert_eq!(descriptor.profile_id, 0x0104);
        assert_eq!(descriptor.device_id, 0x010c);
        assert_eq!(descriptor.input_clusters, vec![0x0000, 0x0006, 0x0008, 0x0300]);
        assert_eq!(descriptor.output_clusters, vec![0x0019]);
        assert_eq!(classify_device_type(&descriptor), ZigbeeDeviceType::Lamp);

        let mut failed = payload;
        failed[1] = 0x81;
        assert!(parse_simple_desc_response(&failed).is_none(), "a failed status");
        assert!(parse_simple_desc_response(&payload[..payload.len() - 1]).is_none(), "cut short");
        let mut overlong = payload;
        overlong[11] = 9;
        assert!(parse_simple_desc_response(&overlong).is_none(), "more clusters than bytes");
    }

    #[test]
    fn a_dimmer_is_a_remote() {
        let dimmer = SimpleDescriptor {
            endpoint: 1,
            profile_id: HOME_AUTOMATION_PROFILE_ID,
            device_id: 0x0820,
            input_clusters: vec![0x0000],
            output_clusters: vec![0x0006, 0x0008],
        };
        assert_eq!(classify_device_type(&dimmer), ZigbeeDeviceType::Remote);
        let zll = SimpleDescriptor { profile_id: 0xc05e, ..dimmer };
        assert_eq!(classify_device_type(&zll), ZigbeeDeviceType::Unknown);
    }

    #[test]
    fn addresses_and_endpoint_lists() {
        assert_eq!(parse_eui64("00:17:88:01:08:0c:00:0b"), Some([0x00, 0x17, 0x88, 0x01, 0x08, 0x0c, 0x00, 0x0b]));
        assert_eq!(parse_eui64("00:17:88"), None);
        assert_eq!(parse_eui64("00:17:88:01:08:0c:00:0b:00"), None);
        assert_eq!(parse_active_ep_response(&[1, 0, 0x34, 0x2e, 2, 11, 242]), Some(vec![11, 242]));
        assert_eq!(parse_active_ep_response(&[1, 0, 0x34, 0x2e, 3, 11, 242]), None, "cut");
        let announce = [0, 0x34, 0x2e, 0x0b, 0x00, 0x0c, 0x08, 0x01, 0x88, 0x17, 0x00, 0x8e];
        let announce = parse_device_announce(&announce).unwrap();
        assert_eq!((announce.node_id, announce.eui64.as_str()), (0x2e34, "00:17:88:01:08:0c:00:0b"));
    }
}
