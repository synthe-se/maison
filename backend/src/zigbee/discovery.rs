//! Interviewing devices: their endpoints and clusters (ZDO), what they are (lamp, remote),
//! their names and state (ZCL reads). Sleepy remotes rarely answer, so a remote is also
//! recognised by the commands it sends.

use std::time::Duration;

use tokio::time::Instant;
use tracing::{debug, info, warn};

use super::{
    callbacks::DriverEvent,
    context::EzspContext,
    device::{DiscoveredDevice, ZigbeeDeviceType},
    error::DriverError,
    remotes::bind_remote_clusters,
    zcl::{
        ACTIVE_EP_REQ_CLUSTER_ID, BASIC_CLUSTER_ID, COLOR_CONTROL_CLUSTER_ID, LEVEL_CONTROL_CLUSTER_ID, ON_OFF_CLUSTER_ID,
        PHILIPS_SPECIFIC_CLUSTER_ID, SIMPLE_DESC_REQ_CLUSTER_ID, SimpleDescriptor, ZDO_PROFILE_ID, classify_device_type,
        hex_bytes, is_preferred_light_endpoint, parse_active_ep_response, parse_device_announce, parse_simple_desc_response,
        parse_zcl_header,
    },
};

/// Interview retries made unasked: a sleepy remote will not answer them; it is interviewed
/// when it wakes up instead (see [`intercept_uninterviewed`]).
const MAX_PROACTIVE_INTERVIEW_ATTEMPTS: u32 = 3;
/// At most one discovery per device in this time, however chatty it is while uninterviewed.
const DISCOVERY_COOLDOWN: Duration = Duration::from_secs(10);
/// Endpoints asked directly: sleepy devices (the Hue dimmer) often ignore Active_EP_req but
/// answer Simple_Desc_req while briefly awake.
const COMMON_ENDPOINTS: [u8; 2] = [1, 2];

pub fn should_probe_active_endpoints(target: &DiscoveredDevice) -> bool {
    target.info.endpoint.is_none() || target.info.input_clusters.is_empty()
}

pub async fn request_active_endpoints(context: &mut EzspContext, node_id: u16) -> Result<(), DriverError> {
    context
        .send_zdo(node_id, ACTIVE_EP_REQ_CLUSTER_ID, "send Active_EP_req", |sequence| {
            let [low, high] = node_id.to_le_bytes();
            vec![sequence, low, high]
        })
        .await
}

async fn request_simple_descriptor(context: &mut EzspContext, node_id: u16, endpoint: u8) -> Result<(), DriverError> {
    context
        .send_zdo(node_id, SIMPLE_DESC_REQ_CLUSTER_ID, "send Simple_Desc_req", |sequence| {
            let [low, high] = node_id.to_le_bytes();
            vec![sequence, low, high, endpoint]
        })
        .await
}

/// Asks a lamp its names (Basic) and its light (On/Off, Level, colour).
pub async fn refresh_device_state(context: &mut EzspContext, target: &DiscoveredDevice) -> Result<(), DriverError> {
    let (node_id, info) = (target.node_id, &target.info);
    let endpoint = info.endpoint.ok_or_else(|| DriverError::Unsupported("This Zigbee device has no endpoint yet".into()))?;
    context.send_read_attributes(node_id, endpoint, BASIC_CLUSTER_ID, &[0x0004, 0x0005]).await?;
    if info.has_input(ON_OFF_CLUSTER_ID) {
        context.send_read_attributes(node_id, endpoint, ON_OFF_CLUSTER_ID, &[0x0000]).await?;
    }
    if info.has_input(LEVEL_CONTROL_CLUSTER_ID) {
        context.send_read_attributes(node_id, endpoint, LEVEL_CONTROL_CLUSTER_ID, &[0x0000]).await?;
    }
    if info.has_color_control_cluster() {
        let attributes = [0x0003, 0x0004, 0x0007, 0x0008, 0x400A];
        context.send_read_attributes(node_id, endpoint, COLOR_CONTROL_CLUSTER_ID, &attributes).await?;
    }
    Ok(())
}

/// Interviews a device that is not (endpoints), or reads a known one's state. A remote is
/// classified when it joins: nothing to ask it.
pub async fn request_known_device_discovery(context: &mut EzspContext, node_id: u16) {
    let Some(target) = context.device(node_id).cloned() else {
        return;
    };
    if target.info.device_type == ZigbeeDeviceType::Remote {
        return;
    }
    if let Some(device) = context.device_mut(node_id) {
        device.last_discovery_at = Some(Instant::now());
    }
    if should_probe_active_endpoints(&target) {
        let _ = request_active_endpoints(context, node_id).await;
        for endpoint in COMMON_ENDPOINTS {
            let _ = request_simple_descriptor(context, node_id, endpoint).await;
        }
    } else {
        let _ = refresh_device_state(context, &target).await;
    }
}

pub async fn retry_pending_interviews(context: &mut EzspContext) {
    let retry_targets = context
        .joined_devices
        .iter()
        .filter(|device| {
            device.connected && device.info.endpoint.is_none() && device.interview_attempts < MAX_PROACTIVE_INTERVIEW_ATTEMPTS
        })
        .map(|device| (device.node_id, device.interview_attempts))
        .collect::<Vec<_>>();

    for (node_id, attempts) in retry_targets {
        debug!(node_id = format_args!("0x{node_id:04x}"), attempts, "retrying native zigbee endpoint discovery");
        if let Some(device) = context.device_mut(node_id) {
            device.interview_attempts = device.interview_attempts.saturating_add(1);
        }
        if let Err(error) = request_active_endpoints(context, node_id).await {
            warn!(node_id = format_args!("0x{node_id:04x}"), %error, "native zigbee endpoint discovery retry failed");
        }
        for endpoint in COMMON_ENDPOINTS {
            let _ = request_simple_descriptor(context, node_id, endpoint).await;
        }
    }
}

/// An uninterviewed device that sends something is awake now. On/Off, Level or Philips
/// commands make it a remote (which will *never* answer ZDO requests: classified at once,
/// and bound so its next presses reach us); anything else but our own ZDO answers starts
/// its discovery (rate-limited).
pub async fn intercept_uninterviewed(context: &mut EzspContext, node_id: u16, cluster_id: u16, profile_id: u16, payload: &[u8]) {
    let remote_cluster = [ON_OFF_CLUSTER_ID, LEVEL_CONTROL_CLUSTER_ID, PHILIPS_SPECIFIC_CLUSTER_ID].contains(&cluster_id);
    let is_command = parse_zcl_header(payload).is_some_and(|header| header.cluster_specific);
    if remote_cluster && is_command {
        info!(node_id = format_args!("0x{node_id:04x}"), cluster_id = format_args!("0x{cluster_id:04x}"), payload = %hex_bytes(payload),
            "uninterviewed device sent ZCL remote command — auto-classifying as remote");
        let Some(device) = context.device_mut(node_id) else { return };
        device.info.device_type = ZigbeeDeviceType::Remote;
        device.info.endpoint = Some(1);
        device.info.output_clusters = vec![ON_OFF_CLUSTER_ID, LEVEL_CONTROL_CLUSTER_ID];
        device.info.supports_brightness = false;
        device.info.supports_temperature = false;
        device.info.supports_color = false;
        device.interview_completed = true;
        let eui64 = device.info.eui64.clone();
        bind_remote_clusters(context, node_id, &eui64).await;
        return;
    }
    // ZDO answers (Active_EP_rsp, Simple_Desc_rsp) reply to our own discovery: discovering
    // again on them would loop, every answer causing more round-trips, until the command
    // channel starves
    if profile_id == ZDO_PROFILE_ID {
        return;
    }
    let due = context
        .device(node_id)
        .is_some_and(|device| device.last_discovery_at.is_none_or(|at| at.elapsed() >= DISCOVERY_COOLDOWN));
    if !due {
        debug!(node_id = format_args!("0x{node_id:04x}"), "device is awake but discovery cooldown active — skipping");
        return;
    }
    info!(node_id = format_args!("0x{node_id:04x}"), cluster_id = format_args!("0x{cluster_id:04x}"),
        "device is awake but uninterviewed — triggering discovery now");
    request_known_device_discovery(context, node_id).await;
}

/// A device (re-)joined: a classification it had is dropped, so it is interviewed afresh
/// (a remote reset and re-joined may be something else now, and the remote filter must
/// not swallow its ZDO answers).
pub async fn on_device_announce(context: &mut EzspContext, payload: &[u8]) -> Option<DriverEvent> {
    let announcement = parse_device_announce(payload)?;
    let node_id = announcement.node_id;
    info!(node_id = format_args!("0x{node_id:04x}"), eui64 = %announcement.eui64, "device announce received — starting discovery");
    super::callbacks::ensure_joined_device(context, node_id, announcement.eui64.clone());
    if let Some(device) = context.device_mut(node_id).filter(|device| device.interview_completed) {
        info!(node_id = format_args!("0x{node_id:04x}"), old_type = ?device.info.device_type, old_endpoint = ?device.info.endpoint,
            "device re-announced — resetting classification for fresh interview");
        device.info.device_type = ZigbeeDeviceType::Unknown;
        device.info.endpoint = None;
        device.info.input_clusters = Vec::new();
        device.info.output_clusters = Vec::new();
        device.interview_completed = false;
        device.interview_attempts = 0;
    }
    request_known_device_discovery(context, node_id).await;
    Some(DriverEvent::DeviceAnnounced { node_id, eui64: announcement.eui64 })
}

/// Asks each endpoint's descriptor, the ZLL one (242) last.
pub async fn on_active_endpoints(context: &mut EzspContext, node_id: u16, payload: &[u8]) {
    let Some(mut endpoints) = parse_active_ep_response(payload) else { return };
    endpoints.sort_by_key(|endpoint| *endpoint == 242);
    for endpoint in endpoints {
        let _ = request_simple_descriptor(context, node_id, endpoint).await;
    }
}

/// An endpoint's descriptor: the device is classified, and a lamp's state read.
pub async fn on_simple_descriptor(context: &mut EzspContext, node_id: u16, payload: &[u8]) {
    let Some(description) = parse_simple_desc_response(payload) else { return };
    let classified = classify_device_type(&description);
    info!(
        node_id = format_args!("0x{node_id:04x}"),
        endpoint = description.endpoint,
        profile_id = format_args!("0x{:04x}", description.profile_id),
        device_id = format_args!("0x{:04x}", description.device_id),
        input_clusters = ?description.input_clusters,
        output_clusters = ?description.output_clusters,
        device_type = ?classified,
        "simple descriptor received — device classified"
    );
    let Some(device) = context.device_mut(node_id) else { return };
    device.heard();
    if !takes_endpoint(device, &description, classified) {
        return;
    }
    let info = &mut device.info;
    info.endpoint = Some(description.endpoint);
    info.device_type = classified;
    let is_remote = classified == ZigbeeDeviceType::Remote;
    if is_remote {
        info.supports_brightness = false;
        info.supports_temperature = false;
        info.supports_color = false;
        info!(node_id = format_args!("0x{node_id:04x}"), eui64 = %info.eui64, endpoint = description.endpoint, "detected Zigbee remote / dimmer switch");
    } else {
        info.supports_brightness = description.input_clusters.contains(&LEVEL_CONTROL_CLUSTER_ID);
        // a temperature is only known from its attribute; XY only from colorCapabilities,
        // read with the state below
        info.supports_temperature = info.supports_temperature && description.input_clusters.contains(&COLOR_CONTROL_CLUSTER_ID);
    }
    info.input_clusters = description.input_clusters;
    info.output_clusters = description.output_clusters;
    device.interview_completed = true;
    device.interview_attempts = 0;
    if !is_remote {
        let target = device.clone();
        let _ = refresh_device_state(context, &target).await;
    }
}

/// Whether this endpoint describes the device. A lamp endpoint wins over a remote one:
/// multi-endpoint devices like the Hue Lightstrip Plus (LCL001) have a utility endpoint
/// (1, On/Off in its output clusters only: « remote ») and the real light one (11). The
/// light endpoint corrects a remote classification; a remote one never overwrites a lamp.
fn takes_endpoint(device: &DiscoveredDevice, description: &SimpleDescriptor, classified: ZigbeeDeviceType) -> bool {
    let info = &device.info;
    if classified == ZigbeeDeviceType::Remote {
        return info.device_type != ZigbeeDeviceType::Lamp;
    }
    let light = is_preferred_light_endpoint(description);
    let has_no_light_cluster = [ON_OFF_CLUSTER_ID, LEVEL_CONTROL_CLUSTER_ID, COLOR_CONTROL_CLUSTER_ID]
        .iter()
        .all(|cluster| !info.has_input(*cluster));
    (info.endpoint.is_none() && light)
        || info.endpoint == Some(description.endpoint)
        || (light && has_no_light_cluster)
        || (light && info.device_type == ZigbeeDeviceType::Remote)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zigbee::zcl::HOME_AUTOMATION_PROFILE_ID;

    fn endpoint(endpoint: u8, inputs: &[u16], outputs: &[u16]) -> SimpleDescriptor {
        SimpleDescriptor {
            endpoint,
            profile_id: HOME_AUTOMATION_PROFILE_ID,
            device_id: 0x010c,
            input_clusters: inputs.to_vec(),
            output_clusters: outputs.to_vec(),
        }
    }

    #[test]
    fn startup_probe_only_runs_for_uninterviewed_devices() {
        let base = DiscoveredDevice::test_lamp();
        assert!(!should_probe_active_endpoints(&base));
        let mut missing_endpoint = base.clone();
        missing_endpoint.info.endpoint = None;
        assert!(should_probe_active_endpoints(&missing_endpoint));
        let mut missing_clusters = base;
        missing_clusters.info.input_clusters.clear();
        assert!(should_probe_active_endpoints(&missing_clusters));
    }

    #[test]
    fn a_light_endpoint_wins_over_a_utility_one() {
        let utility = endpoint(1, &[0x0000], &[0x0006]);
        let light = endpoint(11, &[0x0000, 0x0006, 0x0008, 0x0300], &[]);

        // the utility endpoint first: a remote, until the light endpoint corrects it
        let mut strip = DiscoveredDevice::known(1, Default::default());
        assert!(takes_endpoint(&strip, &utility, ZigbeeDeviceType::Remote));
        strip.info.endpoint = Some(1);
        strip.info.device_type = ZigbeeDeviceType::Remote;
        strip.info.input_clusters = utility.input_clusters.clone();
        assert!(takes_endpoint(&strip, &light, ZigbeeDeviceType::Lamp));

        // a lamp already: the utility endpoint never overwrites it
        let lamp = DiscoveredDevice::test_lamp();
        assert!(!takes_endpoint(&lamp, &utility, ZigbeeDeviceType::Remote));
        assert!(takes_endpoint(&lamp, &endpoint(11, &[6, 8], &[]), ZigbeeDeviceType::Lamp), "its own endpoint again");
        assert!(!takes_endpoint(&lamp, &endpoint(12, &[6, 8], &[]), ZigbeeDeviceType::Lamp), "a second light endpoint");
    }
}
