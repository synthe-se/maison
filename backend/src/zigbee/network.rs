//! The link to the dongle and the Zigbee network on it: opening the serial port, negotiating
//! EZSP, configuring the stack, forming or resuming the coordinator's network, and telling
//! a quiet link from a dead one.

use std::{num::NonZero, sync::Arc, time::Duration};

use ashv2::{Payload, ezsp::Receiver as AshEzspReceiver, start as start_ash};
use ezsp::{
    Callback, Client as EzspClient, Configuration, Connection as EzspConnection, Networking, Security, Utilities,
    ember::{
        Eui64,
        join::Method as EmberJoinMethod,
        key::Data as EmberKeyData,
        network::{Parameters as EmberNetworkParameters, Status as EmberNetworkStatus},
        security::initial,
    },
    ezsp::{config, decision, network::InitBitmask as NetworkInitBitmask, policy, value},
};
use tokio::{
    sync::mpsc,
    time::{Instant, timeout},
};
use tokio_serial::SerialPortBuilderExt;
use tracing::{debug, info, warn};

use super::{
    config::ZigbeeConfig,
    context::{EzspContext, PipelineTasks},
    device::DiscoveredDevice,
    driver::{EZSP_COMMAND_TIMEOUT, Shared},
    error::DriverError,
    zcl::{DEFAULT_SOURCE_ENDPOINT, HOME_AUTOMATION_PROFILE_ID, format_eui64},
};

const EZSP_CHANNEL_SIZE: usize = 256;
const EZSP_INIT_TIMEOUT: Duration = Duration::from_secs(10);
/// Upper bound for the full network bring-up (endpoint + stack configuration, network
/// init/form, security state): an NCP that hangs mid-initialization must not leave the
/// driver `Starting` forever.
const NETWORK_INIT_TIMEOUT: Duration = Duration::from_secs(60);
/// Silent this long (no command, no callback), the link is presumed dead: rebuild it.
const WATCHDOG_TIMEOUT: Duration = Duration::from_secs(180);
/// Quiet for this long (a calm night), the link is probed with a cheap `network_state()`
/// round-trip, so a healthy dongle is never torn down by the watchdog for want of traffic.
const KEEPALIVE_AFTER: Duration = Duration::from_secs(60);
const DEFAULT_HOME_GATEWAY_DEVICE_ID: u16 = 0x0050;
const DEFAULT_LOCAL_INPUT_CLUSTERS: &[u16] = &[0x0000, 0x0006, 0x0008, 0x0300, 0x0403, 0x0201, 0xFC00];
const DEFAULT_LOCAL_OUTPUT_CLUSTERS: &[u16] = &[0x0000, 0x0006, 0x0008, 0x0300, 0x0403];
const DEFAULT_STACK_PROFILE: u16 = 2;
const DEFAULT_SECURITY_LEVEL: u16 = 5;
pub const ZIGBEE_ALLIANCE09_LINK_KEY: EmberKeyData = *b"ZigBeeAlliance09";
/// EZSP policy decision: ZLL (Touchlink) messages processed by the NCP.
const ZLL_POLICY_ENABLED: u8 = 0x00;

/// The coordinator's network, as the driver sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkState {
    Unknown,
    Down,
    Joined,
}

impl NetworkState {
    /// What the status says while it is not up (a healthy network says nothing).
    pub fn problem(self) -> Option<&'static str> {
        match self {
            Self::Joined => None,
            Self::Down => Some("The Zigbee network is down"),
            Self::Unknown => Some("The Zigbee network state is unknown"),
        }
    }
}

impl From<EmberNetworkStatus> for NetworkState {
    fn from(state: EmberNetworkStatus) -> Self {
        match state {
            EmberNetworkStatus::JoinedNetwork => Self::Joined,
            EmberNetworkStatus::JoiningNetwork
            | EmberNetworkStatus::NoNetwork
            | EmberNetworkStatus::JoinedNetworkNoParent
            | EmberNetworkStatus::LeavingNetwork => Self::Down,
        }
    }
}

/// The network state a stack status callback announces, if it announces one. Opening or
/// closing the network to joins happens on a network that is up.
pub fn network_state_of(status: Result<ezsp::ember::Status, u8>) -> Option<NetworkState> {
    use ezsp::ember::Status;
    match status {
        Ok(Status::NetworkUp | Status::NetworkOpened | Status::NetworkClosed) => Some(NetworkState::Joined),
        Ok(Status::NetworkDown) => Some(NetworkState::Down),
        other => {
            info!(status = ?other, "zigbee stack status");
            None
        }
    }
}

/// A pipeline on `serial_port`, its coordinator network up, `devices` carried over.
pub async fn bring_up(
    config: &ZigbeeConfig,
    serial_port: &str,
    shared: &Arc<Shared>,
    devices: &[DiscoveredDevice],
) -> Result<EzspContext, DriverError> {
    let mut context = open_ezsp_context(serial_port, config.protocol_version, Arc::clone(shared)).await?;
    context.joined_devices = devices.to_vec();
    let state = match timeout(NETWORK_INIT_TIMEOUT, ensure_coordinator_network(&mut context, config, serial_port)).await {
        Ok(Ok(state)) => state,
        Ok(Err(error)) => {
            teardown_context(context).await;
            return Err(error);
        }
        Err(_elapsed) => {
            teardown_context(context).await;
            return Err(DriverError::Timeout("Zigbee network bring-up"));
        }
    };
    info!(%serial_port, network_state = ?state, "native zigbee EZSP stack initialized");
    shared.set_network(NetworkState::from(state)).await;
    Ok(context)
}

/// Tear down the EZSP pipeline: abort and join the four actor tasks and drop the
/// connection and serial handles. Bounded (aborted tasks resolve at once): nothing leaks
/// across a reconnect, and teardown never hangs the driver.
pub async fn teardown_context(context: EzspContext) {
    info!("tearing down EZSP pipeline");
    let EzspContext { connection, tasks, callbacks_rx, .. } = context;
    drop(connection);
    drop(callbacks_rx);
    tasks.shutdown().await;
    info!("EZSP pipeline torn down");
}

/// The serial modes tried in turn, the dongle's own first.
const SERIAL_MODES: [(u32, tokio_serial::FlowControl, &str); 3] = [
    (115_200, tokio_serial::FlowControl::None, "no-flow-control"),
    (57_600, tokio_serial::FlowControl::Software, "xon-xoff"),
    (115_200, tokio_serial::FlowControl::Hardware, "rts-cts"),
];

async fn open_ezsp_context(
    serial_port: &str,
    protocol_version: NonZero<u8>,
    shared: Arc<Shared>,
) -> Result<EzspContext, DriverError> {
    let mut last_error = None;
    for (baud_rate, flow_control, mode) in SERIAL_MODES {
        info!(%serial_port, %mode, "opening EZSP serial transport");
        // Negotiation is strict: the NCP must report exactly the version asked. On a
        // mismatch (a firmware upgrade) the pipeline is rebuilt once with the version it
        // announced, so the driver adapts without a config change.
        let link = Link { serial_port, baud_rate, flow_control, mode };
        let opened = match link.open(protocol_version).await {
            Err((_, Some(announced))) => {
                warn!(%serial_port, desired = protocol_version.get(), negotiated = announced.get(), "NCP reports a different EZSP protocol version — retrying with it");
                link.open(announced).await
            }
            other => other,
        };
        match opened {
            Ok((connection, callbacks_rx, tasks)) => {
                info!(%serial_port, %mode, "EZSP connection established");
                return Ok(EzspContext::new(connection, callbacks_rx, tasks, shared));
            }
            Err((error, _)) => {
                warn!(%serial_port, %mode, %error, "EZSP init attempt failed");
                last_error = Some(error);
            }
        }
    }
    Err(last_error.unwrap_or(DriverError::Unavailable("The Zigbee radio did not answer")))
}

/// One way to open the serial port.
struct Link<'a> {
    serial_port: &'a str,
    baud_rate: u32,
    flow_control: tokio_serial::FlowControl,
    mode: &'static str,
}

type Pipeline = (EzspConnection, mpsc::Receiver<Callback>, PipelineTasks);

impl Link<'_> {
    /// Opens the port, spawns the four pipeline tasks and negotiates `version`. On failure
    /// the tasks are shut down; on a version mismatch the version the NCP announced comes
    /// back with the error, for a retry with it.
    async fn open(&self, version: NonZero<u8>) -> Result<Pipeline, (DriverError, Option<NonZero<u8>>)> {
        let stream = tokio_serial::new(self.serial_port, self.baud_rate)
            .flow_control(self.flow_control)
            .open_native_async()
            .map_err(|error| (DriverError::Serial(error), None))?;
        let (reader, writer) = tokio::io::split(stream);
        let (payload_tx, payload_rx) = mpsc::channel::<Payload>(EZSP_CHANNEL_SIZE);
        let (ash_handle, ash_futures) = start_ash(reader, writer, payload_tx);
        let (client, ezsp_futures) = EzspClient::run(ash_handle, AshEzspReceiver::new(payload_rx), EZSP_CHANNEL_SIZE);
        let tasks = PipelineTasks([
            tokio::spawn(ezsp_futures.transmitter),
            tokio::spawn(ezsp_futures.receiver),
            tokio::spawn(ash_futures.transmitter),
            tokio::spawn(ash_futures.receiver),
        ]);

        info!(serial_port = %self.serial_port, mode = %self.mode, protocol_version = version.get(), "negotiating EZSP protocol version");
        match timeout(EZSP_INIT_TIMEOUT, client.connect(version)).await {
            Ok(Ok((connection, callbacks_rx))) => Ok((connection, callbacks_rx, tasks)),
            Ok(Err(error)) => {
                tasks.shutdown().await;
                let announced = match &error {
                    ezsp::Error::ProtocolVersionMismatch { negotiated, .. } => {
                        NonZero::new(negotiated.protocol_version()).filter(|announced| *announced != version)
                    }
                    _ => None,
                };
                Err((DriverError::ezsp("protocol negotiation")(error), announced))
            }
            Err(_elapsed) => {
                tasks.shutdown().await;
                Err((DriverError::Timeout("EZSP protocol negotiation"), None))
            }
        }
    }
}

async fn ensure_coordinator_network(
    context: &mut EzspContext,
    config: &ZigbeeConfig,
    serial_port: &str,
) -> Result<EmberNetworkStatus, DriverError> {
    configure_local_endpoint(context).await?;
    configure_stack(context).await?;
    configure_trust_center(context).await?;

    if let Err(error) = context.connection.network_init(NetworkInitBitmask::PARENT_INFO_IN_TOKEN).await {
        warn!(%serial_port, %error, "ezsp network_init failed");
    }
    let mut state = context.connection.network_state().await.map_err(DriverError::ezsp("read network state"))?;

    if state == EmberNetworkStatus::NoNetwork {
        info!(%serial_port, "forming new Zigbee coordinator network");
        form_coordinator_network(context, config).await?;
        state = wait_for_network_ready(context).await?;
    } else {
        // The network exists: refresh the security state so flag changes apply to the next
        // join. Some firmware refuses it on a running network (EmberInvalidCall); the
        // runtime policies set above are the ones that matter for joins.
        info!(%serial_port, "refreshing trust center security state on existing network");
        match Security::set_initial_security_state(&mut context.connection, initial_security_state()).await {
            Ok(()) => info!("trust center security state refreshed successfully"),
            Err(error) => warn!(%error, "set_initial_security_state not accepted on running network (non-fatal — runtime policies still apply)"),
        }
    }

    log_network_parameters(context, serial_port).await?;
    // a ZDO Bind_req names the coordinator
    match context.connection.get_eui64().await {
        Ok(eui64) => {
            info!(eui64 = %format_eui64(eui64.into_array()), "coordinator EUI64 cached");
            context.coordinator_eui64 = Some(eui64);
        }
        Err(error) => warn!(%error, "failed to fetch coordinator EUI64 — remote binding will not work"),
    }
    Ok(state)
}

async fn configure_local_endpoint(context: &mut EzspContext) -> Result<(), DriverError> {
    context
        .connection
        .add_endpoint(
            DEFAULT_SOURCE_ENDPOINT,
            HOME_AUTOMATION_PROFILE_ID,
            DEFAULT_HOME_GATEWAY_DEVICE_ID,
            0,
            DEFAULT_LOCAL_INPUT_CLUSTERS.iter().copied().collect(),
            DEFAULT_LOCAL_OUTPUT_CLUSTERS.iter().copied().collect(),
        )
        .await
        .map_err(DriverError::ezsp("add local endpoint"))
}

async fn configure_stack(context: &mut EzspContext) -> Result<(), DriverError> {
    let settings = [
        (config::Id::StackProfile, DEFAULT_STACK_PROFILE, "set stack profile"),
        (config::Id::SecurityLevel, DEFAULT_SECURITY_LEVEL, "set security level"),
        // sleepy end devices (remotes, sensors) join as children: some firmware allows none
        (config::Id::MaxEndDeviceChildren, 16, "set max end device children"),
        // 2^8 minutes (~4 h) before a child that did not poll is dropped: remotes poll rarely
        (config::Id::EndDevicePollTimeout, 8, "set end device poll timeout"),
    ];
    for (id, value, what) in settings {
        context.connection.set_configuration_value(id, value).await.map_err(DriverError::ezsp(what))?;
    }
    Ok(())
}

/// The trust center's policies, as zigbee2mqtt's Ember adapter sets them.
async fn configure_trust_center(context: &mut EzspContext) -> Result<(), DriverError> {
    let connection = &mut context.connection;
    // New joins and unsecured rejoins (bitmask 0x03): the network key goes out encrypted with
    // the joiner's link key, the well-known one imported at permit-join time.
    let joins = (decision::Bitmask::ALLOW_JOINS | decision::Bitmask::ALLOW_UNSECURED_REJOINS).bits();
    connection.set_policy(policy::Id::TrustCenter, joins).await.map_err(DriverError::ezsp("set trust center policy"))?;
    // the Trust Center link key may be requested (Zigbee 3.0)
    let key_requests = u8::from(decision::Id::AllowTcKeyRequestsAndSendCurrentKey);
    connection.set_policy(policy::Id::TcKeyRequest, key_requests).await.map_err(DriverError::ezsp("set TC key request policy"))?;
    // rejoins with « ZigBeeAlliance09 » (the Hue dimmer after a reset), for 10 minutes
    connection
        .set_policy(policy::Id::TcJoinsUsingWellKnownKey, 0x01u8)
        .await
        .map_err(DriverError::ezsp("set TC well-known key rejoin policy"))?;
    connection
        .set_configuration_value(config::Id::TcRejoinsUsingWellKnownKeyTimeoutSec, 600)
        .await
        .map_err(DriverError::ezsp("set TC well-known key rejoin timeout"))?;
    // no application link keys between devices
    let app_keys = u8::from(decision::Id::DenyAppKeyRequests);
    connection.set_policy(policy::Id::AppKeyRequest, app_keys).await.map_err(DriverError::ezsp("set app key request policy (deny)"))?;
    // transient keys kept 300 s (LE u16)
    let transient: heapless::Vec<u8, 255, u8> = [0x2C, 0x01].into_iter().collect();
    connection
        .set_value(value::Id::TransientKeyTimeoutSec, transient)
        .await
        .map_err(DriverError::ezsp("set transient key timeout"))?;
    // JOINER_GLOBAL_LINK_KEY | NWK_LEAVE_REQUEST_NOT_ALLOWED (0x0110, LE): no rogue device
    // may force others off the network
    let extended: heapless::Vec<u8, 255, u8> = [0x10, 0x01].into_iter().collect();
    connection
        .set_value(value::Id::ExtendedSecurityBitmask, extended)
        .await
        .map_err(DriverError::ezsp("set extended security bitmask"))?;
    let tag_only = u8::from(decision::Id::MessageTagOnlyInCallback);
    connection
        .set_policy(policy::Id::MessageContentsInCallback, tag_only)
        .await
        .map_err(DriverError::ezsp("set message contents callback policy"))?;
    // Touchlink requests and responses handled by the NCP
    connection.set_policy(policy::Id::Zll, ZLL_POLICY_ENABLED).await.map_err(DriverError::ezsp("enable ZLL policy"))
}

async fn form_coordinator_network(context: &mut EzspContext, config: &ZigbeeConfig) -> Result<(), DriverError> {
    let coordinator_eui64 = context.connection.get_eui64().await.map_err(DriverError::ezsp("get coordinator EUI64"))?;
    let pan_id = match config.pan_id {
        Some(value) => usable_pan_id(value),
        None => usable_pan_id(context.connection.get_random_number().await.map_err(DriverError::ezsp("generate PAN ID"))?),
    };
    let extended_pan_id = config.extended_pan_id.map(Eui64::from).unwrap_or_else(|| derive_extended_pan_id(coordinator_eui64));
    let (channel, tx_power) = (config.channel, config.tx_power);
    info!(pan_id = format_args!("0x{pan_id:04x}"), %extended_pan_id, channel, tx_power, "forming Zigbee coordinator network");

    Security::set_initial_security_state(&mut context.connection, initial_security_state())
        .await
        .map_err(DriverError::ezsp("set initial security state"))?;
    let parameters = EmberNetworkParameters::new(
        extended_pan_id,
        pan_id,
        tx_power,
        channel,
        EmberJoinMethod::MacAssociation,
        0,
        0,
        1_u32 << channel,
    );
    context.connection.form_network(parameters).await.map_err(DriverError::ezsp("form coordinator network"))
}

async fn log_network_parameters(context: &mut EzspContext, serial_port: &str) -> Result<(), DriverError> {
    let (node_type, parameters) =
        context.connection.get_network_parameters().await.map_err(DriverError::ezsp("get network parameters"))?;
    info!(
        %serial_port,
        node_type = %node_type,
        pan_id = format_args!("0x{:04x}", parameters.pan_id()),
        extended_pan_id = %parameters.extended_pan_id(),
        channel = parameters.radio_channel(),
        tx_power = parameters.radio_tx_power(),
        join_method = ?parameters.join_method(),
        "native zigbee network parameters"
    );
    Ok(())
}

async fn wait_for_network_ready(context: &mut EzspContext) -> Result<EmberNetworkStatus, DriverError> {
    for _ in 0..25 {
        if let Err(error) = super::callbacks::drain_callbacks(context).await {
            warn!(%error, "callback while forming the network");
        }
        let state = context.connection.network_state().await.map_err(DriverError::ezsp("read network state after form"))?;
        if state != EmberNetworkStatus::NoNetwork && state != EmberNetworkStatus::JoiningNetwork {
            return Ok(state);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    Err(DriverError::Timeout("Zigbee coordinator network formation"))
}

/// zigbee2mqtt's Ember initial security state: TRUST_CENTER_GLOBAL_LINK_KEY |
/// HAVE_PRECONFIGURED_KEY | HAVE_NETWORK_KEY | TRUST_CENTER_USES_HASHED_LINK_KEY |
/// REQUIRE_ENCRYPTED_KEY. The last one has the NCP deliver the network key only to a joiner
/// that proves it holds the link key (the transient key imported at permit-join time), never
/// in the clear; the hashed one says the preconfigured key is the well-known
/// « ZigBeeAlliance09 ».
fn initial_security_state() -> initial::State {
    initial::State::new(
        initial::Bitmask::TRUST_CENTER_GLOBAL_LINK_KEY
            | initial::Bitmask::HAVE_PRECONFIGURED_KEY
            | initial::Bitmask::HAVE_NETWORK_KEY
            | initial::Bitmask::TRUST_CENTER_USES_HASHED_LINK_KEY
            | initial::Bitmask::REQUIRE_ENCRYPTED_KEY,
        ZIGBEE_ALLIANCE09_LINK_KEY,
        [0; 16],
        0,
        Eui64::default(),
    )
}

fn derive_extended_pan_id(coordinator_eui64: Eui64) -> Eui64 {
    let mut bytes = coordinator_eui64.into_array();
    bytes[0] ^= 0x02;
    Eui64::from(bytes)
}

/// 0x0000 and 0xffff are not PAN ids.
fn usable_pan_id(value: u16) -> u16 {
    match value {
        0x0000 | 0xffff => 0x1a62,
        other => other,
    }
}

/// What the watchdog makes of the link's silence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinkHealth {
    Fine,
    /// Quiet: prove it alive with a keepalive round-trip.
    Probe,
    /// Nothing for too long, keepalives included: rebuild the pipeline.
    Dead,
}

fn link_health(idle: Duration) -> LinkHealth {
    if idle > WATCHDOG_TIMEOUT {
        LinkHealth::Dead
    } else if idle >= KEEPALIVE_AFTER {
        LinkHealth::Probe
    } else {
        LinkHealth::Fine
    }
}

/// The pipeline's tasks are alive and the link answers (probed when quiet); else why the
/// pipeline must be rebuilt.
pub async fn check_link(context: &mut EzspContext) -> Result<(), String> {
    if !context.tasks.is_alive() {
        return Err("EZSP pipeline task(s) died".to_string());
    }
    match link_health(context.last_activity.elapsed()) {
        LinkHealth::Fine => Ok(()),
        LinkHealth::Dead => Err("EZSP watchdog timeout — no activity".to_string()),
        LinkHealth::Probe => match timeout(EZSP_COMMAND_TIMEOUT, context.connection.network_state()).await {
            Ok(Ok(_)) => {
                debug!("EZSP keepalive answered");
                context.last_activity = Instant::now();
                Ok(())
            }
            Ok(Err(error)) => Err(format!("EZSP keepalive failed: {error}")),
            // a dropped round-trip leaves the pipeline out of step: rebuild it
            Err(_elapsed) => Err("EZSP keepalive timed out".to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_watchdog_probes_a_quiet_link_before_giving_up_on_it() {
        assert_eq!(link_health(Duration::ZERO), LinkHealth::Fine);
        assert_eq!(link_health(KEEPALIVE_AFTER - Duration::from_millis(1)), LinkHealth::Fine);
        assert_eq!(link_health(KEEPALIVE_AFTER), LinkHealth::Probe, "a calm network is probed, not torn down");
        assert_eq!(link_health(WATCHDOG_TIMEOUT), LinkHealth::Probe);
        assert_eq!(link_health(WATCHDOG_TIMEOUT + Duration::from_secs(1)), LinkHealth::Dead);
        const { assert!(KEEPALIVE_AFTER.as_secs() < WATCHDOG_TIMEOUT.as_secs()) };
    }

    #[test]
    fn stack_statuses_say_the_network_state() {
        use ezsp::ember::Status;
        assert_eq!(network_state_of(Ok(Status::NetworkUp)), Some(NetworkState::Joined));
        assert_eq!(network_state_of(Ok(Status::NetworkDown)), Some(NetworkState::Down));
        assert_eq!(network_state_of(Ok(Status::NetworkOpened)), Some(NetworkState::Joined), "open to joins: up");
        assert_eq!(network_state_of(Ok(Status::NetworkClosed)), Some(NetworkState::Joined), "closed to joins: up");
        assert_eq!(network_state_of(Ok(Status::DeliveryFailed)), None);
        assert_eq!(network_state_of(Err(0xee)), None);
        assert_eq!(NetworkState::from(EmberNetworkStatus::JoinedNetwork), NetworkState::Joined);
        assert_eq!(NetworkState::from(EmberNetworkStatus::JoinedNetworkNoParent), NetworkState::Down);
        assert_eq!(NetworkState::Joined.problem(), None);
        assert!(NetworkState::Down.problem().is_some());
    }

    #[test]
    fn pan_ids() {
        assert_eq!(usable_pan_id(0), 0x1a62);
        assert_eq!(usable_pan_id(0xffff), 0x1a62);
        assert_eq!(usable_pan_id(0x1234), 0x1234);
        let eui = Eui64::from([0x00, 0x12, 0x4b, 0, 0, 0, 0, 1]);
        assert_eq!(derive_extended_pan_id(eui).into_array()[0], 0x02);
    }
}
