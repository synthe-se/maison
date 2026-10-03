//! The commands the manager queues to the driver, and how each is carried out.

use std::time::Duration;

use ezsp::{
    Configuration, Networking, Security,
    ember::{Eui64, network::Duration as NetworkDuration},
    ezsp::{decision, policy},
};
use silizium::zigbee::security::man as security_man;
use tracing::{debug, info, warn};

use super::{
    context::EzspContext,
    device::{DiscoveredDevice, ZigbeeDeviceInfo, normalize_device_id},
    discovery::{refresh_device_state, request_active_endpoints, should_probe_active_endpoints},
    driver::{EZSP_COMMAND_TIMEOUT, TOUCHLINK_COMMAND_TIMEOUT},
    error::DriverError,
    network::ZIGBEE_ALLIANCE09_LINK_KEY,
    touchlink::touchlink_scan,
    zcl::{
        COLOR_CONTROL_CLUSTER_ID, COLOR_MODE_TEMPERATURE, COLOR_MODE_XY, IDENTIFY_CLUSTER_ID, LEVEL_CONTROL_CLUSTER_ID,
        ON_OFF_CLUSTER_ID, ZigbeeEffect, build_brightness_command_payload, build_color_temperature_command_payload,
        build_color_xy_command_payload, build_on_off_command_payload,
    },
};

#[derive(Debug, Clone)]
pub enum DriverCommand {
    PermitJoin { seconds: u16 },
    DiscoverDevices,
    SetPower { lamp_id: String, enabled: bool },
    SetBrightness { lamp_id: String, brightness: u8 },
    SetTemperature { lamp_id: String, temperature: u8 },
    SetColor { lamp_id: String, x: f32, y: f32 },
    SetEffect { lamp_id: String, effect: ZigbeeEffect },
    /// A Touchlink (ZLL) scan: commissions factory-new ZLL devices (the Hue Lightstrip Plus)
    /// that ignore the NWK-level permit-join.
    TouchlinkScan,
}

impl DriverCommand {
    /// How long it may take: a Touchlink scan is long, every other command a few EZSP
    /// round-trips.
    pub fn timeout(&self) -> Duration {
        match self {
            Self::TouchlinkScan => TOUCHLINK_COMMAND_TIMEOUT,
            _ => EZSP_COMMAND_TIMEOUT,
        }
    }
}

pub async fn handle_command(context: &mut EzspContext, command: DriverCommand) -> Result<(), DriverError> {
    match command {
        DriverCommand::PermitJoin { seconds } => permit_join(context, seconds).await,
        DriverCommand::DiscoverDevices => {
            discover_devices(context).await;
            Ok(())
        }
        DriverCommand::SetPower { lamp_id, enabled } => set_power(context, &lamp_id, enabled).await,
        DriverCommand::SetBrightness { lamp_id, brightness } => set_brightness(context, &lamp_id, brightness).await,
        DriverCommand::SetTemperature { lamp_id, temperature } => set_temperature(context, &lamp_id, temperature).await,
        DriverCommand::SetColor { lamp_id, x, y } => set_color(context, &lamp_id, x, y).await,
        DriverCommand::SetEffect { lamp_id, effect } => set_effect(context, &lamp_id, effect).await,
        DriverCommand::TouchlinkScan => touchlink_scan(context).await,
    }
}

async fn permit_join(context: &mut EzspContext, seconds: u16) -> Result<(), DriverError> {
    if seconds > 0 {
        open_to_joins(context).await;
    } else {
        close_to_joins(context).await;
    }
    let duration = if seconds == 0 {
        NetworkDuration::Disable
    } else {
        NetworkDuration::try_from(Duration::from_secs(u64::from(seconds)))
            .map_err(|error| DriverError::Unsupported(format!("Invalid permit-join duration {seconds}: {error}")))?
    };
    context.connection.permit_joining(duration).await.map_err(DriverError::ezsp("permit join"))?;
    info!(seconds, "native zigbee permit join updated");
    Ok(())
}

/// Before opening: the well-known « ZigBeeAlliance09 » key in the transient key table for
/// any joiner (the wildcard EUI64 is all 0xFF, as zigbee2mqtt does; all zeros is no
/// wildcard), and joins allowed (bitmask 0x03).
async fn open_to_joins(context: &mut EzspContext) {
    let anyone = Eui64::new(0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF);
    // the security-manager context is only on the wire from EZSP v14; the uplg/ezsp fork
    // drops it for the MG21's v13 firmware
    let key_context = security_man::Context::new(
        security_man::KeyType::TcLink,
        0,
        security_man::DerivedKeyType::None,
        anyone,
        0,
        security_man::Flags::NONE,
        0,
    );
    let key: security_man::Key = ZIGBEE_ALLIANCE09_LINK_KEY;
    match context.connection.import_transient_key(key_context, anyone, key, security_man::Flags::NONE).await {
        Ok(()) => info!("transient key (ZigBeeAlliance09) imported for all joining devices (wildcard EUI64)"),
        Err(error) => warn!(%error, "failed to import transient key — new devices may not be able to join"),
    }
    let joins = (decision::Bitmask::ALLOW_JOINS | decision::Bitmask::ALLOW_UNSECURED_REJOINS).bits();
    match context.connection.set_policy(policy::Id::TrustCenter, joins).await {
        Ok(()) => info!("trust center policy set to ALLOW_JOINS | ALLOW_UNSECURED_REJOINS"),
        Err(error) => warn!(%error, "failed to update trust center policy for joining"),
    }
}

/// On closing: no transient key left (a joiner needs one), rejoins only.
async fn close_to_joins(context: &mut EzspContext) {
    match context.connection.clear_transient_link_keys().await {
        Ok(()) => info!("transient link keys cleared (network closed)"),
        Err(error) => warn!(%error, "failed to clear transient link keys"),
    }
    match context.connection.set_policy(policy::Id::TrustCenter, decision::Bitmask::ALLOW_UNSECURED_REJOINS.bits()).await {
        Ok(()) => info!("trust center policy set to ALLOW_UNSECURED_REJOINS only (network closed)"),
        Err(error) => warn!(%error, "failed to restrict trust center policy after closing"),
    }
}

/// Probes every known device: its endpoints when not interviewed, its state otherwise.
/// No timeout around each call: EZSP over UART is strictly request-response, and dropping a
/// `communicate()` future mid-receive orphans the answer and desynchronises every later
/// command. A slow NCP is waited for.
async fn discover_devices(context: &mut EzspContext) {
    let targets = context.joined_devices.clone();
    info!(known_devices = targets.len(), "native zigbee discovery requested");
    for target in targets {
        let (node_id, eui64) = (format!("0x{:04x}", target.node_id), &target.info.eui64);
        info!(%node_id, %eui64, endpoint = ?target.info.endpoint, "probing known Zigbee device");
        if should_probe_active_endpoints(&target) {
            if let Err(error) = request_active_endpoints(context, target.node_id).await {
                warn!(%node_id, %eui64, %error, "native zigbee active endpoint probe failed");
                continue;
            }
        }
        if target.info.endpoint.is_some() {
            if let Err(error) = refresh_device_state(context, &target).await {
                warn!(%node_id, %eui64, %error, "native zigbee state refresh failed during discovery");
            }
        }
    }
    if context.joined_devices.is_empty() {
        debug!("native zigbee discovery skipped because no joined devices are known yet");
    }
}

/// The device `lamp_id` names: its EUI64, the API's id, or a node id.
fn find_target_device<'a>(context: &'a EzspContext, lamp_id: &str) -> Result<&'a DiscoveredDevice, DriverError> {
    let wanted = normalize_device_id(lamp_id, 0);
    context
        .joined_devices
        .iter()
        .find(|device| {
            device.info.eui64 == lamp_id
                || normalize_device_id(&device.info.eui64, device.node_id) == wanted
                || format!("{:016x}", device.node_id) == wanted
        })
        .ok_or_else(|| DriverError::UnknownDevice(lamp_id.to_string()))
}

/// The lamp `lamp_id` names, if `able` says it can: its node id and the endpoint it is
/// driven on.
fn lamp_able(
    context: &EzspContext,
    lamp_id: &str,
    able: fn(&ZigbeeDeviceInfo) -> bool,
    unable: &str,
) -> Result<(u16, u8), DriverError> {
    let lamp = find_target_device(context, lamp_id)?;
    if !able(&lamp.info) {
        return Err(DriverError::Unsupported(format!("This Zigbee lamp {unable}")));
    }
    let endpoint = lamp
        .info
        .endpoint
        .ok_or_else(|| DriverError::Unsupported("This Zigbee lamp has not been interviewed yet".to_string()))?;
    Ok((lamp.node_id, endpoint))
}

async fn set_power(context: &mut EzspContext, lamp_id: &str, enabled: bool) -> Result<(), DriverError> {
    let able = |info: &ZigbeeDeviceInfo| info.has_input(ON_OFF_CLUSTER_ID);
    let (node_id, endpoint) = lamp_able(context, lamp_id, able, "has no On/Off cluster yet")?;
    context
        .send_zcl(node_id, endpoint, ON_OFF_CLUSTER_ID, "unicast on/off", |sequence| build_on_off_command_payload(enabled, sequence))
        .await?;
    if let Some(device) = context.device_mut(node_id) {
        device.light.is_on = enabled;
    }
    Ok(())
}

async fn set_brightness(context: &mut EzspContext, lamp_id: &str, brightness: u8) -> Result<(), DriverError> {
    let able = |info: &ZigbeeDeviceInfo| info.supports_brightness;
    let (node_id, endpoint) = lamp_able(context, lamp_id, able, "cannot be dimmed")?;
    context
        .send_zcl(node_id, endpoint, LEVEL_CONTROL_CLUSTER_ID, "unicast brightness", |sequence| {
            build_brightness_command_payload(brightness, sequence)
        })
        .await?;
    if let Some(device) = context.device_mut(node_id) {
        let brightness = brightness.min(100);
        device.light.brightness = brightness;
        device.light.is_on = brightness > 0;
        device.desired.brightness = Some(brightness);
    }
    Ok(())
}

async fn set_temperature(context: &mut EzspContext, lamp_id: &str, temperature: u8) -> Result<(), DriverError> {
    let able = |info: &ZigbeeDeviceInfo| info.supports_temperature;
    let (node_id, endpoint) = lamp_able(context, lamp_id, able, "has no colour temperature")?;
    context
        .send_zcl(node_id, endpoint, COLOR_CONTROL_CLUSTER_ID, "unicast color temperature", |sequence| {
            build_color_temperature_command_payload(temperature, sequence)
        })
        .await?;
    if let Some(device) = context.device_mut(node_id) {
        let temperature = temperature.min(100);
        device.light.temperature = Some(temperature);
        device.light.colour.color_mode = Some(COLOR_MODE_TEMPERATURE);
        device.desired.temperature = Some(temperature);
        // in temperature mode now: a stale colour restored later would switch it back
        device.desired.color = None;
    }
    Ok(())
}

async fn set_color(context: &mut EzspContext, lamp_id: &str, x: f32, y: f32) -> Result<(), DriverError> {
    let able = |info: &ZigbeeDeviceInfo| info.supports_color;
    let (node_id, endpoint) = lamp_able(context, lamp_id, able, "has no colour (XY)")?;
    context
        .send_zcl(node_id, endpoint, COLOR_CONTROL_CLUSTER_ID, "unicast color xy", |sequence| {
            build_color_xy_command_payload(x, y, sequence)
        })
        .await?;
    if let Some(device) = context.device_mut(node_id) {
        let (x, y) = (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0));
        device.light.colour.color_x = Some(x);
        device.light.colour.color_y = Some(y);
        device.light.colour.color_mode = Some(COLOR_MODE_XY);
        device.desired.color = Some((x, y));
        // in colour mode now: a stale temperature restored later would switch it back
        device.desired.temperature = None;
    }
    Ok(())
}

async fn set_effect(context: &mut EzspContext, lamp_id: &str, effect: ZigbeeEffect) -> Result<(), DriverError> {
    let (node_id, endpoint) = lamp_able(context, lamp_id, |_| true, "")?;
    let cluster_id = effect.cluster();
    let what = if cluster_id == IDENTIFY_CLUSTER_ID { "unicast identify effect" } else { "unicast hue effect" };
    context.send_zcl(node_id, endpoint, cluster_id, what, |sequence| effect.frame(sequence).1).await
}
