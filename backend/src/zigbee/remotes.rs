//! The Hue dimmer (and any Zigbee remote): its button presses switch and dim every lamp.
//! One press arrives twice (on the standard On/Off or Level cluster and on Philips' own
//! cluster): the second is dropped, or it would hit lamps that changed in between.

use std::time::Duration;

use tokio::time::Instant;
use tracing::{debug, info, warn};

use super::{
    context::EzspContext,
    device::{Hex, ZigbeeDeviceType},
    zcl::{
        build_brightness_command_payload, build_on_off_command_payload, parse_zcl_header,
        LEVEL_CONTROL_CLUSTER_ID, ON_OFF_CLUSTER_ID, PHILIPS_SPECIFIC_CLUSTER_ID,
        ZCL_LEVEL_CONTROL_COMMAND_MOVE, ZCL_LEVEL_CONTROL_COMMAND_MOVE_WITH_ON_OFF,
        ZCL_LEVEL_CONTROL_COMMAND_STEP, ZCL_LEVEL_CONTROL_COMMAND_STEP_WITH_ON_OFF,
        ZCL_LEVEL_CONTROL_COMMAND_STOP, ZCL_LEVEL_CONTROL_COMMAND_STOP_WITH_ON_OFF,
        ZCL_ON_OFF_COMMAND_OFF, ZCL_ON_OFF_COMMAND_ON,
    },
};

/// The same action again within this window is the same press.
const REMOTE_DEDUP_WINDOW: Duration = Duration::from_secs(3);
/// Brightness change per dimmer press, in points of 0–100.
const DIMMER_BRIGHTNESS_STEP: u8 = 15;

/// The last power and dim actions (on/up, when), to drop a press's second copy.
#[derive(Debug, Default)]
pub struct RemotePresses {
    power: Option<(bool, Instant)>,
    dim: Option<(bool, Instant)>,
}

/// `value` was last done within the window.
fn is_repeat(last: Option<(bool, Instant)>, value: bool) -> bool {
    last.is_some_and(|(done, at)| done == value && at.elapsed() < REMOTE_DEDUP_WINDOW)
}

/// A command from a remote, as the lamps' action it stands for.
pub async fn handle_remote_command(
    context: &mut EzspContext,
    remote: u16,
    cluster_id: u16,
    payload: &[u8],
) {
    let node = Hex(remote);
    info!(remote_node_id = %node, cluster_id = %Hex(cluster_id), payload = %crate::util::hex(payload, " "), "remote: incoming command from remote device");
    // [frame control, (manufacturer code ×2: Philips' hueNotification), sequence, command, …]
    let Some(header) = parse_zcl_header(payload) else {
        warn!(remote_node_id = %node, payload = %crate::util::hex(payload, " "), "remote: malformed or short ZCL frame");
        return;
    };
    // client → server cluster commands only; a report or a global answer is no press
    if !header.cluster_specific {
        info!(remote_node_id = %node, frame_control = format_args!("0x{:02x}", payload[0]), "remote: ignoring non-cluster-specific frame (global command or report)");
        return;
    }
    if let Some(device) = context.device_mut(remote) {
        device.heard();
    }
    let body = &payload[header.body..];
    match cluster_id {
        ON_OFF_CLUSTER_ID => match header.command_id {
            ZCL_ON_OFF_COMMAND_ON => remote_power(context, node, true, "ON").await,
            ZCL_ON_OFF_COMMAND_OFF => remote_power(context, node, false, "OFF").await,
            command_id => {
                debug!(remote_node_id = %node, command_id, "remote: unknown On/Off command")
            }
        },
        LEVEL_CONTROL_CLUSTER_ID => level_command(context, node, header.command_id, body).await,
        PHILIPS_SPECIFIC_CLUSTER_ID => hue_notification(context, node, body).await,
        _ => {
            debug!(remote_node_id = %node, cluster_id = %Hex(cluster_id), command_id = header.command_id, "remote: unhandled cluster command")
        }
    }
}

/// Step `[mode, size, time ×2]` or Move `[mode, rate]`: mode 0 is up, 1 down. Stop is
/// ignored (a press is one step).
async fn level_command(context: &mut EzspContext, node: Hex, command_id: u8, body: &[u8]) {
    match command_id {
        ZCL_LEVEL_CONTROL_COMMAND_STEP
        | ZCL_LEVEL_CONTROL_COMMAND_STEP_WITH_ON_OFF
        | ZCL_LEVEL_CONTROL_COMMAND_MOVE
        | ZCL_LEVEL_CONTROL_COMMAND_MOVE_WITH_ON_OFF => {
            if let Some(&mode) = body.first() {
                remote_dim(context, node, mode == 0x00, "level").await;
            }
        }
        ZCL_LEVEL_CONTROL_COMMAND_STOP | ZCL_LEVEL_CONTROL_COMMAND_STOP_WITH_ON_OFF => {
            debug!(remote_node_id = %node, "remote: level stop command (ignored)");
        }
        _ => debug!(remote_node_id = %node, command_id, "remote: unknown Level Control command"),
    }
}

/// Philips' hueNotification (cluster 0xFC00, endpoint 2): `[button, ×3, action, ×1, time, ×1]`.
/// Buttons: 1 on, 2 dim up, 3 dim down, 4 off. Actions: 0 press, 1 hold (repeats),
/// 2 short release, 3 long release; releases are ignored (a press would fire twice).
async fn hue_notification(context: &mut EzspContext, node: Hex, body: &[u8]) {
    let Some(&button) = body.first() else {
        warn!(remote_node_id = %node, "remote: hueNotification payload is empty");
        return;
    };
    let action = body.get(4).copied().unwrap_or(0);
    info!(remote_node_id = %node, button, action, hue_payload = %crate::util::hex(body, " "), "remote: hueNotification from Philips dimmer");
    if action > 1 {
        debug!(remote_node_id = %node, action, "remote: ignoring release event");
        return;
    }
    match button {
        1 => remote_power(context, node, true, "hue ON").await,
        4 => remote_power(context, node, false, "hue OFF").await,
        2 => remote_dim(context, node, true, "hue DIM UP").await,
        3 => remote_dim(context, node, false, "hue DIM DOWN").await,
        _ => debug!(remote_node_id = %node, button, "remote: unknown hueNotification button"),
    }
}

/// Every lamp on or off, once per press.
async fn remote_power(context: &mut EzspContext, node: Hex, on: bool, button: &str) {
    if is_repeat(context.remote_presses.power, on) {
        debug!(remote_node_id = %node, button, "remote: suppressing duplicate (within dedup window)");
        return;
    }
    info!(remote_node_id = %node, button, "remote: power button — all lamps {}", if on { "ON" } else { "OFF" });
    context.remote_presses.power = Some((on, Instant::now()));
    broadcast_power_to_all_lamps(context, on).await;
}

/// Every lamp a step brighter or dimmer, once per press.
async fn remote_dim(context: &mut EzspContext, node: Hex, up: bool, button: &str) {
    if is_repeat(context.remote_presses.dim, up) {
        debug!(remote_node_id = %node, button, "remote: suppressing duplicate brightness step (within dedup window)");
        return;
    }
    info!(remote_node_id = %node, button, direction = if up { "up" } else { "down" }, "remote: brightness step — adjusting all lamps");
    context.remote_presses.dim = Some((up, Instant::now()));
    broadcast_brightness_step_to_all_lamps(context, up).await;
}

/// On/off to every reachable lamp with an On/Off cluster.
async fn broadcast_power_to_all_lamps(context: &mut EzspContext, enabled: bool) {
    let targets = context
        .joined_devices
        .iter()
        .filter(|device| device.info.device_type == ZigbeeDeviceType::Lamp && device.reachable)
        .filter(|device| device.info.has_input(ON_OFF_CLUSTER_ID))
        .filter_map(|device| Some((device.node_id, device.info.endpoint?)))
        .collect::<Vec<_>>();
    info!(
        enabled,
        target_count = targets.len(),
        "remote broadcast: sending power command to all lamps"
    );
    for (lamp, endpoint) in targets {
        let sent = context
            .send_zcl(
                lamp,
                endpoint,
                ON_OFF_CLUSTER_ID,
                "unicast remote on/off",
                |sequence| build_on_off_command_payload(enabled, sequence),
            )
            .await;
        if context.note(lamp, "remote on/off", sent) {
            if let Some(device) = context.device_mut(lamp) {
                device.light.is_on = enabled;
            }
        }
    }
}

/// Every reachable dimmable lamp a step up or down (never below 1: dimming does not switch
/// off).
async fn broadcast_brightness_step_to_all_lamps(context: &mut EzspContext, up: bool) {
    let targets = context
        .joined_devices
        .iter()
        .filter(|device| device.info.device_type == ZigbeeDeviceType::Lamp && device.reachable)
        .filter(|device| {
            device.info.supports_brightness && device.info.has_input(LEVEL_CONTROL_CLUSTER_ID)
        })
        .filter_map(|device| {
            Some((
                device.node_id,
                device.info.endpoint?,
                device.light.brightness,
            ))
        })
        .collect::<Vec<_>>();
    info!(
        direction = if up { "up" } else { "down" },
        step = DIMMER_BRIGHTNESS_STEP,
        target_count = targets.len(),
        "remote broadcast: sending brightness step to all lamps"
    );
    for (lamp, endpoint, current) in targets {
        let brightness = stepped(current, up);
        let sent = context
            .send_zcl(
                lamp,
                endpoint,
                LEVEL_CONTROL_CLUSTER_ID,
                "unicast remote brightness step",
                |sequence| build_brightness_command_payload(brightness, sequence),
            )
            .await;
        if context.note(lamp, "remote brightness step", sent) {
            if let Some(device) = context.device_mut(lamp) {
                device.light.brightness = brightness;
                device.light.is_on = true;
                device.desired.brightness = Some(brightness);
            }
        }
    }
}

/// A dimmer step from `brightness`, as `lamps::brightness` keeps it (never below the dimmest).
fn stepped(brightness: u8, up: bool) -> u8 {
    crate::lamps::brightness(if up {
        brightness.saturating_add(DIMMER_BRIGHTNESS_STEP)
    } else {
        brightness.saturating_sub(DIMMER_BRIGHTNESS_STEP)
    })
}

/// Binds a Hue dimmer's buttons to the coordinator, both ways it reports them: endpoint 1
/// On/Off and Level (lamp commands), endpoint 2 Philips' hueNotification.
pub async fn bind_remote_clusters(context: &mut EzspContext, node_id: u16, eui64: &str) {
    let bindings = [
        (1, ON_OFF_CLUSTER_ID),
        (1, LEVEL_CONTROL_CLUSTER_ID),
        (2, PHILIPS_SPECIFIC_CLUSTER_ID),
    ];
    for (endpoint, cluster_id) in bindings {
        let bound = context
            .bind_to_coordinator(node_id, eui64, endpoint, cluster_id)
            .await;
        if context.note(node_id, "remote Bind_req", bound) {
            info!(node_id = %Hex(node_id), endpoint, cluster_id = %Hex(cluster_id), "ZDO Bind_req sent");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_second_copy_of_a_press_is_dropped() {
        let now = Instant::now();
        assert!(!is_repeat(None, true));
        assert!(
            is_repeat(Some((true, now)), true),
            "the same action, just now"
        );
        assert!(!is_repeat(Some((false, now)), true), "the other button");
        let long_ago = now.checked_sub(REMOTE_DEDUP_WINDOW + Duration::from_millis(1));
        if let Some(long_ago) = long_ago {
            assert!(
                !is_repeat(Some((true, long_ago)), true),
                "a new press after the window"
            );
        }
    }

    #[test]
    fn dimming_steps_stay_in_range_and_never_switch_off() {
        assert_eq!(stepped(50, true), 65);
        assert_eq!(stepped(95, true), 100);
        assert_eq!(stepped(10, false), 1);
        assert_eq!(stepped(1, false), 1);
    }
}
