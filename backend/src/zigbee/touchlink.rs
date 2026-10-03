//! Touchlink (ZLL) commissioning, for factory-new ZLL lamps (the Hue Lightstrip Plus) that
//! ignore the NWK-level permit-join: set the ZLL security state, scan, and ask each lamp
//! found to join our network. It then shows up through the usual TrustCenterJoin /
//! ChildJoin path.

use std::time::Duration;

use ezsp::{
    Zll,
    ember::{
        node::Type as EmberNodeType,
        zll::{self, InitialSecurityState as ZllInitialSecurityState, Network as ZllNetwork},
    },
    ezsp::zll::NetworkOperation as ZllNetworkOperation,
    parameters::zll::handler::Handler as ZllHandler,
};
use tokio::time::Instant;
use tracing::{debug, info, warn};

use super::{
    callbacks::{drain_callbacks, on_callback},
    context::EzspContext,
    error::DriverError,
    zcl::format_eui64,
};

/// Primary ZLL channels (11, 15, 20, 25), scanned first.
const ZLL_PRIMARY_CHANNEL_MASK: u32 = (1 << 11) | (1 << 15) | (1 << 20) | (1 << 25);
/// Every Zigbee channel (11–26), when the primary ones found nothing.
const ZLL_ALL_CHANNELS_MASK: u32 = 0x07FF_F800;
/// The well-known « ZigBee Light Link Master Key », shared by every certified ZLL device:
/// it encrypts the network key during commissioning.
const ZLL_MASTER_KEY: [u8; 16] = [0x9F, 0x55, 0x95, 0xF1, 0x02, 0x57, 0xC8, 0xA4, 0x69, 0xCB, 0xF4, 0x2B, 0xC9, 0x3F, 0xEE, 0x31];
/// ZLL InitialSecurityState bitmask: reserved per the EZSP documentation.
const ZLL_SECURITY_KEY_BITMASK: u32 = 0;
const TOUCHLINK_SCAN_TIMEOUT: Duration = Duration::from_secs(15);
/// Touchlink tells proximity by RSSI: the strongest signal there is.
const TOUCHLINK_TX_POWER: i8 = 20;

/// The scan under way: the lamps found so far.
#[derive(Debug, Default)]
pub struct TouchlinkScan {
    found: Vec<TouchlinkFoundNetwork>,
    in_progress: bool,
}

#[derive(Debug, Clone)]
struct TouchlinkFoundNetwork {
    network_info: ZllNetwork,
    device_endpoint: Option<u8>,
    device_id: Option<u16>,
    rssi: i8,
}

pub async fn touchlink_scan(context: &mut EzspContext) -> Result<(), DriverError> {
    info!("touchlink: starting ZLL Touchlink scan");
    configure_zll(context).await?;
    let found = scan(context).await?;
    if found.is_empty() {
        info!("touchlink: no ZLL devices found on any channel");
        return Ok(());
    }
    for (index, lamp) in found.iter().enumerate() {
        commission(context, index, lamp).await;
    }
    info!(commissioned = found.len(), "touchlink: commissioning complete");
    Ok(())
}

/// The ZLL master key as the initiator's key (the preconfigured one is not needed). Set
/// without a network key: `set_initial_security_state` only works before a network is
/// formed, and ours runs; the NCP knows its own network key.
async fn configure_zll(context: &mut EzspContext) -> Result<(), DriverError> {
    let security = ZllInitialSecurityState::new(ZLL_SECURITY_KEY_BITMASK, zll::KeyIndex::Master, ZLL_MASTER_KEY, [0u8; 16]);
    Zll::set_security_state_without_key(&mut context.connection, security)
        .await
        .map_err(DriverError::ezsp("set ZLL security state"))?;
    info!("touchlink: ZLL security state configured (master key)");
    context
        .connection
        .set_primary_channel_mask(ZLL_PRIMARY_CHANNEL_MASK)
        .await
        .map_err(DriverError::ezsp("set ZLL primary channel mask"))?;
    info!(channel_mask = format_args!("0x{ZLL_PRIMARY_CHANNEL_MASK:08x}"), "touchlink: primary channel mask set");
    Ok(())
}

/// The primary channels, then every channel if they found nothing.
async fn scan(context: &mut EzspContext) -> Result<Vec<TouchlinkFoundNetwork>, DriverError> {
    let passes = [("primary ZLL (11,15,20,25)", ZLL_PRIMARY_CHANNEL_MASK), ("all Zigbee (11-26)", ZLL_ALL_CHANNELS_MASK)];
    for (label, channel_mask) in passes {
        context.touchlink = TouchlinkScan { found: Vec::new(), in_progress: true };
        info!(channels = label, channel_mask = format_args!("0x{channel_mask:08x}"), tx_power = TOUCHLINK_TX_POWER, "touchlink: starting scan pass");
        Zll::start_scan(&mut context.connection, channel_mask, TOUCHLINK_TX_POWER, EmberNodeType::Coordinator)
            .await
            .map_err(DriverError::ezsp("start ZLL Touchlink scan"))?;
        // until ScanComplete, or the timeout
        let deadline = Instant::now() + TOUCHLINK_SCAN_TIMEOUT;
        while context.touchlink.in_progress {
            tokio::select! {
                Some(callback) = context.callbacks_rx.recv() => on_callback(context, callback).await,
                _ = tokio::time::sleep_until(deadline) => break,
            }
        }
        if context.touchlink.in_progress {
            warn!(channels = label, "touchlink: scan timed out after {TOUCHLINK_SCAN_TIMEOUT:?}");
            context.touchlink.in_progress = false;
        }
        info!(found_count = context.touchlink.found.len(), channels = label, "touchlink: scan pass complete");
        if !context.touchlink.found.is_empty() {
            break;
        }
    }
    Ok(std::mem::take(&mut context.touchlink.found))
}

/// Tells one lamp found to join our network, then gives it time to (its joins and announce
/// are handled as they come).
async fn commission(context: &mut EzspContext, index: usize, lamp: &TouchlinkFoundNetwork) {
    let eui64 = format_eui64(lamp.network_info.eui64().into_array());
    info!(index, %eui64, rssi = lamp.rssi, device_id = ?lamp.device_id, endpoint = ?lamp.device_endpoint, "touchlink: commissioning ZLL device (JoinTarget)");
    match context.connection.network_ops(lamp.network_info.clone(), ZllNetworkOperation::JoinTarget, TOUCHLINK_TX_POWER).await {
        Ok(()) => {
            info!(%eui64, "touchlink: JoinTarget command sent successfully — device should join the network");
            tokio::time::sleep(Duration::from_secs(2)).await;
            if let Err(error) = drain_callbacks(context).await {
                warn!(%error, "callback after a Touchlink join");
            }
        }
        Err(error) => warn!(%eui64, %error, "touchlink: JoinTarget failed for device"),
    }
}

/// The ZLL callbacks: lamps found while scanning, the scan's end.
pub fn handle_zll_callback(context: &mut EzspContext, handler: ZllHandler) {
    match handler {
        ZllHandler::NetworkFound(found) => {
            let network = found.network_info();
            let (device_endpoint, device_id) = match found.device_info() {
                Some(info) => {
                    info!(endpoint = info.endpoint_id(), profile_id = format_args!("0x{:04x}", info.profile_id()),
                        device_id = format_args!("0x{:04x}", info.device_id()), device_eui64 = %format_eui64(info.ieee_address().into_array()),
                        "touchlink: device info available");
                    (Some(info.endpoint_id()), Some(info.device_id()))
                }
                None => {
                    debug!("touchlink: no device info in NetworkFound callback");
                    (None, None)
                }
            };
            info!(eui64 = %format_eui64(network.eui64().into_array()), rssi = found.last_hop_rssi(), node_id = format_args!("0x{:04x}", network.node_id()),
                number_sub_devices = network.number_sub_devices(), "touchlink: ZLL device found during scan");
            context.touchlink.found.push(TouchlinkFoundNetwork {
                network_info: network.clone(),
                device_endpoint,
                device_id,
                rssi: found.last_hop_rssi(),
            });
        }
        ZllHandler::ScanComplete(scan) => {
            info!(status = ?scan.result(), found_count = context.touchlink.found.len(), "touchlink: scan complete");
            context.touchlink.in_progress = false;
        }
        ZllHandler::AddressAssignment(assignment) => {
            info!(node_id = format_args!("0x{:04x}", assignment.address_info().node_id_()), "touchlink: address assignment received");
        }
        ZllHandler::TouchLinkTarget(target) => {
            info!(eui64 = %format_eui64(target.network_info().eui64().into_array()),
                "touchlink: TouchLinkTarget callback (we are being touchlinked by another device)");
        }
    }
}
