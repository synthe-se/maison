//! What the dongle tells unasked: joins, leaves, network state, incoming frames. Every
//! callback goes through [`on_callback`], whoever drains it (the event loop, a Touchlink
//! scan, the availability check): a network going down is never lost in a side drain.

use ezsp::{
    Callback,
    ember::{device::Update as EmberDeviceUpdate, node::Type as EmberNodeType},
    parameters,
};
use tokio::time::{Instant, timeout};
use tracing::{debug, info};

use super::{
    context::EzspContext,
    device::{DiscoveredDevice, ZigbeeDeviceType},
    discovery::{intercept_uninterviewed, on_active_endpoints, on_device_announce, on_simple_descriptor, request_known_device_discovery},
    driver::EZSP_COMMAND_TIMEOUT,
    error::DriverError,
    network::{NetworkState, network_state_of},
    remotes::handle_remote_command,
    touchlink::handle_zll_callback,
    zcl::{
        ACTIVE_EP_RSP_CLUSTER_ID, BASIC_CLUSTER_ID, COLOR_CONTROL_CLUSTER_ID, DEVICE_ANNCE_CLUSTER_ID, LEVEL_CONTROL_CLUSTER_ID,
        ON_OFF_CLUSTER_ID, SIMPLE_DESC_RSP_CLUSTER_ID, ZDO_PROFILE_ID, apply_lamp_frame, format_eui64, hex_bytes,
    },
};

/// What a callback changed that others must hear of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriverEvent {
    Network(NetworkState),
    DeviceJoined { node_id: u16, eui64: String },
    DeviceAnnounced { node_id: u16, eui64: String },
    DeliveryFailed { node_id: u16 },
}

/// Every callback already queued, each handled within the command timeout (a callback may
/// make EZSP round-trips: discovery, binding, broadcasts); a hung one is an error, the
/// pipeline is not to be trusted.
pub async fn drain_callbacks(context: &mut EzspContext) -> Result<(), DriverError> {
    while let Ok(callback) = context.callbacks_rx.try_recv() {
        timeout(EZSP_COMMAND_TIMEOUT, on_callback(context, callback))
            .await
            .map_err(|_| DriverError::Timeout("EZSP callback handling"))?;
    }
    Ok(())
}

/// One callback: handled, and what it changed applied.
pub async fn on_callback(context: &mut EzspContext, callback: Callback) {
    context.last_activity = Instant::now();
    if let Some(event) = handle_callback(context, callback).await {
        apply_event(context, event).await;
    }
}

pub async fn apply_event(context: &mut EzspContext, event: DriverEvent) {
    debug!(?event, "native zigbee callback handled");
    match event {
        DriverEvent::Network(state) => context.shared.set_network(state).await,
        // join and announce churn is for the log: the device list is what the user sees
        DriverEvent::DeviceJoined { node_id, eui64 } => {
            info!(%eui64, node_id = format_args!("{node_id:#06x}"), "zigbee device joined");
            context.publish();
        }
        DriverEvent::DeviceAnnounced { node_id, eui64 } => {
            info!(%eui64, node_id = format_args!("{node_id:#06x}"), "zigbee device announced");
            context.publish();
        }
        DriverEvent::DeliveryFailed { .. } => context.publish(),
    }
}

async fn handle_callback(context: &mut EzspContext, callback: Callback) -> Option<DriverEvent> {
    use parameters::{messaging::handler::Handler as Messaging, networking::handler::Handler as Networking};
    match callback {
        Callback::Networking(Networking::StackStatus(status)) => network_state_of(status.result()).map(DriverEvent::Network),
        Callback::Networking(Networking::ChildJoin(join)) => {
            let eui64 = format_eui64(join.child_eui64().into_array());
            let sleepy = join.child_type() == Ok(EmberNodeType::SleepyEndDevice);
            info!(node_id = format_args!("0x{:04x}", join.child_id()), %eui64, joining = join.joining(), child_type = ?join.child_type(), "ChildJoin callback received");
            if !join.joining() {
                return None;
            }
            on_child_join(context, join.child_id(), eui64, sleepy).await
        }
        Callback::TrustCenter(parameters::trust_center::handler::Handler::TrustCenterJoin(join)) => {
            handle_trust_center_join(context, *join).await
        }
        Callback::Messaging(Messaging::IncomingMessage(message)) => {
            let (node_id, frame) = (message.sender(), message.aps_frame());
            let payload = message.message();
            debug!(node_id, cluster_id = frame.cluster_id(), payload = %hex_bytes(payload), "native zigbee incoming message");
            handle_incoming_cluster(context, node_id, frame.cluster_id(), frame.profile_id(), payload).await
        }
        // no MAC acknowledgement after every retry: unreachable now, for fast feedback
        Callback::Messaging(Messaging::MessageSent(sent)) if matches!(sent.ack_received(), Ok(false)) => {
            let node_id = sent.index_or_destination();
            if let Some(device) = context.device_mut(node_id) {
                if device.lost() {
                    info!(node_id = format_args!("0x{node_id:04x}"), eui64 = %device.info.eui64, "device marked unreachable after MAC delivery failure");
                }
            }
            Some(DriverEvent::DeliveryFailed { node_id })
        }
        Callback::Zll(handler) => {
            handle_zll_callback(context, handler);
            None
        }
        _ => None,
    }
}

/// A child joined. A sleepy one is usually a remote that never answers ZDO requests, but a
/// ZLL lamp touchlinked (the Hue Lightstrip) may say « sleepy » too: it is interviewed all
/// the same (unless done already), and a remote is recognised by its first command anyway.
async fn on_child_join(context: &mut EzspContext, node_id: u16, eui64: String, sleepy: bool) -> Option<DriverEvent> {
    ensure_joined_device(context, node_id, eui64.clone());
    let interviewed = context.device(node_id).is_some_and(|device| device.interview_completed);
    if sleepy && interviewed {
        debug!(node_id = format_args!("0x{node_id:04x}"), %eui64, "sleepy end device already interviewed — skipping re-discovery");
    } else {
        if sleepy {
            info!(node_id = format_args!("0x{node_id:04x}"), %eui64, "sleepy end device joined — attempting ZDO discovery before classifying");
        }
        request_known_device_discovery(context, node_id).await;
    }
    Some(DriverEvent::DeviceJoined { node_id, eui64 })
}

/// A join seen by the trust center. Not classified here (by the vendor prefix, every
/// Philips lamp would be a remote): ChildJoin tells sleepy from not; this only makes sure
/// the device is listed and asked.
async fn handle_trust_center_join(
    context: &mut EzspContext,
    join: parameters::trust_center::handler::TrustCenterJoin,
) -> Option<DriverEvent> {
    let status = join.status().ok()?;
    let node_id: u16 = join.new_node_id();
    let eui64 = format_eui64(join.new_node_eui64().into_array());
    match status {
        EmberDeviceUpdate::StandardSecuritySecuredRejoin
        | EmberDeviceUpdate::StandardSecurityUnsecuredJoin
        | EmberDeviceUpdate::StandardSecurityUnsecuredRejoin => {
            info!(node_id = format_args!("0x{node_id:04x}"), %eui64, join_status = ?status, "TrustCenterJoin received");
            ensure_joined_device(context, node_id, eui64.clone());
            request_known_device_discovery(context, node_id).await;
            Some(DriverEvent::DeviceJoined { node_id, eui64 })
        }
        EmberDeviceUpdate::DeviceLeft => {
            context.forget(node_id, &eui64);
            None
        }
    }
}

/// A device heard joining: listed if new, else found (by EUI64 first: a node id may have
/// been given to another device after a rejoin) and marked reachable. One that was out of
/// reach was powered back on at the wall: it is on now. One that was reachable is only the
/// stack healing itself: its light stays as it was.
pub fn ensure_joined_device(context: &mut EzspContext, node_id: u16, eui64: String) {
    let index = context
        .joined_devices
        .iter()
        .position(|device| device.info.eui64 == eui64)
        .or_else(|| context.joined_devices.iter().position(|device| device.node_id == node_id));
    let Some(device) = index.map(|index| &mut context.joined_devices[index]) else {
        info!(node_id = format_args!("0x{node_id:04x}"), %eui64, "new device added to joined list — awaiting interview");
        context.joined_devices.push(DiscoveredDevice::joined(node_id, eui64));
        return;
    };
    let was_reachable = device.reachable;
    device.node_id = node_id;
    device.info.eui64 = eui64;
    device.heard();
    if !was_reachable {
        device.light.is_on = true;
        if device.light.brightness == 0 {
            device.light.brightness = 100;
        }
    }
    if device.info.endpoint.is_none() {
        device.interview_attempts = 0;
    }
}

/// A frame from a device: an uninterviewed one is interviewed (or recognised as a remote),
/// a remote's commands drive the lamps, ZDO answers carry on the interviews, a lamp's
/// frames update its light.
async fn handle_incoming_cluster(
    context: &mut EzspContext,
    node_id: u16,
    cluster_id: u16,
    profile_id: u16,
    payload: &[u8],
) -> Option<DriverEvent> {
    if let Some(device) = context.device(node_id) {
        debug!(node_id = format_args!("0x{node_id:04x}"), cluster_id = format_args!("0x{cluster_id:04x}"),
            device_type = ?device.info.device_type, endpoint = ?device.info.endpoint, interview_completed = device.interview_completed,
            "incoming message: device state at dispatch time");
    }
    if context.device(node_id).is_some_and(|device| device.info.endpoint.is_none()) {
        intercept_uninterviewed(context, node_id, cluster_id, profile_id, payload).await;
    }

    let from_remote = context.device(node_id).is_some_and(|device| device.info.device_type == ZigbeeDeviceType::Remote);
    // ZDO and ZCL cluster ids overlap (ZDO 0x0006 is not On/Off): the profile tells them
    // apart. A remote's Device_annce goes on below: it may have been reset into something
    // else, and is interviewed again.
    if from_remote && cluster_id != DEVICE_ANNCE_CLUSTER_ID {
        if profile_id == ZDO_PROFILE_ID {
            debug!(node_id = format_args!("0x{node_id:04x}"), cluster_id = format_args!("0x{cluster_id:04x}"), "ignoring ZDO frame from remote (not a ZCL command)");
        } else {
            info!(node_id = format_args!("0x{node_id:04x}"), cluster_id = format_args!("0x{cluster_id:04x}"), "routing incoming message to remote command handler");
            handle_remote_command(context, node_id, cluster_id, payload).await;
        }
        return None;
    }

    match cluster_id {
        DEVICE_ANNCE_CLUSTER_ID => on_device_announce(context, payload).await,
        ACTIVE_EP_RSP_CLUSTER_ID => {
            on_active_endpoints(context, node_id, payload).await;
            None
        }
        SIMPLE_DESC_RSP_CLUSTER_ID => {
            on_simple_descriptor(context, node_id, payload).await;
            None
        }
        ON_OFF_CLUSTER_ID | LEVEL_CONTROL_CLUSTER_ID | COLOR_CONTROL_CLUSTER_ID | BASIC_CLUSTER_ID => {
            debug!(node_id = format_args!("0x{node_id:04x}"), cluster_id = format_args!("0x{cluster_id:04x}"), payload = %hex_bytes(payload), "native zigbee lamp frame");
            if let Some(device) = context.device_mut(node_id) {
                if apply_lamp_frame(device, cluster_id, payload) {
                    device.heard();
                }
            }
            None
        }
        _ => None,
    }
}
