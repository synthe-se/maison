//! One worker per device owns its session: it connects, keeps the link alive, reads what
//! the device pushes and runs the commands it is handed, and reconnects with a back-off
//! when the device drops. Its state is published on the device's `watch` channel
//! ([`Link`]), cleared by a guard when the worker ends however it ends (a panic too), and a
//! supervisor starts a fresh worker after a panic.

use std::{
    net::IpAddr,
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use rust_async_tuyapi::{DpId, Payload, PayloadStruct, mesparse::Message, tuyadevice::TuyaDevice};
use serde_json::{Map, Value, json};
use tokio::{
    sync::{Notify, mpsc, oneshot, watch},
    task::JoinHandle,
};

use super::{
    ManagedTuyaDevice, TuyaDeviceConfig, TuyaManager,
    parse::{merge_messages_into_dps, parse_device_data},
};
use crate::{error::AppError, net};

pub(super) const STATUS_TIMEOUT: Duration = Duration::from_millis(12_000);
const MESSAGE_DRAIN_TIMEOUT: Duration = Duration::from_millis(1_500);
const COMMAND_SETTLE_TIMEOUT: Duration = Duration::from_millis(400);
const HEARTBEAT_INTERVAL: Duration = Duration::from_millis(30_000);
pub(super) const COMMAND_REPLY_TIMEOUT: Duration = Duration::from_millis(20_000);
const RECONNECT_BASE_DELAY_MS: u64 = 1_000;
const RECONNECT_MAX_DELAY_MS: u64 = 60_000;

/// How errors from the device library name the device to the client (the detail is logged).
const DEVICE: &str = "Tuya device";

/// A device's connection, as its worker publishes it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Link {
    /// The worker that owns the session; `None` when nobody does.
    pub worker: Option<u64>,
    pub connected: bool,
    pub connecting: bool,
    pub reconnect_attempts: i32,
}

/// The manager's side of a worker. Dropping it cancels the worker (its stop sender goes).
#[derive(Debug)]
pub(super) struct WorkerHandle {
    pub id: u64,
    pub commands: mpsc::Sender<WorkerCommand>,
    /// Cuts a retry back-off short when someone asks to connect now.
    pub wake: Arc<Notify>,
    /// Never sent on: the worker stops when this is dropped.
    pub stop: watch::Sender<()>,
    /// Resolves (with an error) once the worker task is gone.
    pub exited: oneshot::Receiver<()>,
}

/// The worker's side: what it listens to, and the guard that says it is gone.
pub(super) struct WorkerParts {
    id: u64,
    commands: mpsc::Receiver<WorkerCommand>,
    wake: Arc<Notify>,
    stop: StopSignal,
    _exit: WorkerExit,
}

/// Fires when the manager drops the worker's handle.
struct StopSignal(watch::Receiver<()>);

impl StopSignal {
    async fn wait(&mut self) {
        while self.0.changed().await.is_ok() {}
    }
}

/// Dropped when the worker task ends, a panic included: counts it out, clears the link it
/// still owned (so nobody waits on a dead worker) and wakes whoever waits for its end.
struct WorkerExit {
    id: u64,
    link: Arc<watch::Sender<Link>>,
    live: Arc<AtomicUsize>,
    _exited: oneshot::Sender<()>,
}

impl Drop for WorkerExit {
    fn drop(&mut self) {
        self.live.fetch_sub(1, Ordering::SeqCst);
        self.link.send_if_modified(|link| {
            if link.worker != Some(self.id) {
                return false;
            }
            *link = Link { worker: None, connected: false, connecting: false, ..*link };
            true
        });
    }
}

#[derive(Debug)]
pub(super) enum WorkerCommand {
    FetchStatus { reply: oneshot::Sender<Result<Map<String, Value>, AppError>> },
    SetValues { updates: Vec<(String, Value)>, reply: oneshot::Sender<Result<(), AppError>> },
}

impl TuyaManager {
    /// A new worker's two sides, counted live until its parts are dropped.
    pub(super) fn new_worker(&self, device: &ManagedTuyaDevice) -> (WorkerHandle, WorkerParts) {
        let id = self.next_worker.fetch_add(1, Ordering::SeqCst);
        let (commands_tx, commands_rx) = mpsc::channel(16);
        let (stop_tx, stop_rx) = watch::channel(());
        let (exited_tx, exited_rx) = oneshot::channel();
        let wake = Arc::new(Notify::new());
        self.live_workers.fetch_add(1, Ordering::SeqCst);
        let exit = WorkerExit { id, link: device.link.clone(), live: self.live_workers.clone(), _exited: exited_tx };
        (
            WorkerHandle { id, commands: commands_tx, wake: wake.clone(), stop: stop_tx, exited: exited_rx },
            WorkerParts { id, commands: commands_rx, wake, stop: StopSignal(stop_rx), _exit: exit },
        )
    }

    /// Runs the worker, under a supervisor.
    pub(super) fn spawn_worker(&self, device: &ManagedTuyaDevice, parts: WorkerParts) {
        let id = parts.id;
        let worker = tokio::spawn(self.clone().run_device_worker(device.config.clone(), parts));
        tokio::spawn(self.clone().supervise(device.config.id.clone(), id, worker));
    }

    /// Waits for the worker's end: a panic is logged, and once the back-off has passed a
    /// fresh worker takes the device, unless someone has disconnected or reconnected it
    /// meanwhile (its handle is no longer the dead worker's).
    pub(super) async fn supervise(self, device_id: String, id: u64, worker: JoinHandle<()>) {
        let Err(error) = worker.await else { return };
        if !error.is_panic() {
            return;
        }
        tracing::error!(%device_id, %error, "tuya worker panicked; a new one takes over");
        let Ok(attempts) = self.device(&device_id).map(|device| device.link.borrow().reconnect_attempts) else { return };
        tokio::time::sleep(reconnect_delay(attempts)).await;
        let still_dead = self.runtime.read().await.get(&device_id).is_some_and(|state| {
            state.worker.as_ref().is_some_and(|handle| handle.id == id)
        });
        if still_dead {
            if let Err(error) = self.connect_device(&device_id).await {
                tracing::warn!(%device_id, %error, "tuya device not connected after a worker panic");
            }
        }
    }

    /// Changes the device's link only while `worker` still owns it: a cancelled worker
    /// finishing late must not mark a disconnected device connected.
    fn update_own_link(&self, device_id: &str, worker: u64, change: impl FnOnce(&mut Link)) {
        if let Ok(device) = self.device(device_id) {
            device.link.send_if_modified(|link| {
                if link.worker != Some(worker) {
                    return false;
                }
                change(link);
                true
            });
        }
    }

    fn mark_failed(&self, device_id: &str, worker: u64) {
        self.update_own_link(device_id, worker, |link| {
            link.connected = false;
            link.connecting = false;
        });
    }

    async fn run_device_worker(self, config: TuyaDeviceConfig, parts: WorkerParts) {
        let WorkerParts { id, mut commands, wake, mut stop, _exit } = parts;
        loop {
            // Dropping the session on stop drops the device, which closes its socket.
            tokio::select! {
                _ = stop.wait() => return,
                _ = self.run_device_worker_session(&config, id, &mut commands) => {}
            }

            let attempts = self.device(&config.id).map_or(1, |device| device.link.borrow().reconnect_attempts);
            let nudged = tokio::select! {
                _ = stop.wait() => return,
                _ = tokio::time::sleep(reconnect_delay(attempts)) => false,
                _ = wake.notified() => true,
            };
            // A nudge already counted its attempt.
            if !nudged {
                self.update_own_link(&config.id, id, |link| {
                    link.connecting = true;
                    link.reconnect_attempts += 1;
                });
            }
        }
    }

    /// One session: connect, then serve heartbeats, pushed values and commands until the
    /// device fails (the caller then waits and retries).
    async fn run_device_worker_session(
        &self,
        config: &TuyaDeviceConfig,
        worker: u64,
        commands: &mut mpsc::Receiver<WorkerCommand>,
    ) {
        let Ok(device_type) = self.device(&config.id).map(|device| device.device_type) else { return };
        let Ok(ip) = IpAddr::from_str(&config.ip) else {
            tracing::warn!(device = %config.name, ip = %config.ip, "tuya device has an invalid IP address");
            return self.mark_failed(&config.id, worker);
        };
        let Ok(mut device) = TuyaDevice::new(&config.version, &config.id, Some(&config.key), ip) else {
            return self.mark_failed(&config.id, worker);
        };
        let Ok(Ok(mut rx)) = tokio::time::timeout(STATUS_TIMEOUT, device.connect()).await else {
            return self.mark_failed(&config.id, worker);
        };

        self.update_own_link(&config.id, worker, |link| {
            link.connected = true;
            link.connecting = false;
            link.reconnect_attempts = 0;
        });
        if let Ok(dps) = worker_fetch_status(&mut device, &mut rx, config).await {
            self.store_cacheable_dps(&config.id, device_type, &dps).await;
            self.record_dps(&config.id, device_type, &dps).await;
        }

        let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
        loop {
            tokio::select! {
                _ = heartbeat.tick() => {
                    if device.heartbeat().await.is_err() {
                        break;
                    }
                }
                messages = rx.recv() => {
                    let Some(Ok(messages)) = messages else { break };
                    let current = {
                        let runtime = self.runtime.read().await;
                        runtime.get(&config.id).map(|state| state.last_data.clone()).unwrap_or_default()
                    };
                    let merged = merge_messages_into_dps(current, messages);
                    self.store_cacheable_dps(&config.id, device_type, &merged).await;
                    self.record_dps(&config.id, device_type, &merged).await;
                }
                command = commands.recv() => match command {
                    // a caller that gave up waiting has dropped its receiver: nothing to tell it
                    Some(WorkerCommand::FetchStatus { reply }) => {
                        reply.send(worker_fetch_status(&mut device, &mut rx, config).await).ok();
                    }
                    Some(WorkerCommand::SetValues { updates, reply }) => {
                        reply.send(worker_send_commands(&mut device, &mut rx, config, &updates).await).ok();
                    }
                    None => break,
                },
            }
        }
        self.mark_failed(&config.id, worker);
        if let Err(error) = device.disconnect().await {
            tracing::debug!(device = %config.name, %error, "tuya session closed uncleanly");
        }
    }

    /// The device's latest values, as the list shows them.
    pub(super) async fn record_dps(&self, device_id: &str, device_type: super::TuyaDeviceType, dps: &Map<String, Value>) {
        let mut runtime = self.runtime.write().await;
        if let Some(state) = runtime.get_mut(device_id) {
            state.last_data = dps.clone();
            state.parsed_data = parse_device_data(device_type, dps);
        }
    }
}

pub(super) fn reconnect_delay(attempts: i32) -> Duration {
    let exponent = u32::try_from(attempts.max(1) - 1).unwrap_or(0);
    let delay_ms = RECONNECT_BASE_DELAY_MS
        .saturating_mul(2_u64.saturating_pow(exponent))
        .min(RECONNECT_MAX_DELAY_MS);
    Duration::from_millis(delay_ms)
}

async fn worker_fetch_status(
    device: &mut TuyaDevice,
    rx: &mut mpsc::Receiver<rust_async_tuyapi::Result<Vec<Message>>>,
    config: &TuyaDeviceConfig,
) -> Result<Map<String, Value>, AppError> {
    let timeout = || AppError::service_unavailable(format!("Status request timeout for {}", config.name));
    // the protocol's `t` is a u32 of seconds: a clock past 2106 sends none
    let now = u32::try_from(chrono::Utc::now().timestamp()).ok();

    let payload = Payload::Struct(PayloadStruct {
        gw_id: Some(config.id.clone()),
        dev_id: config.id.clone(),
        uid: Some(config.id.clone()),
        t: now.map(|t| t.to_string()),
        dp_id: None,
        dps: Some(json!({})),
    });

    tokio::time::timeout(STATUS_TIMEOUT, device.get(payload))
        .await
        .map_err(|_| timeout())?
        .map_err(|error| net::unreachable(DEVICE, error))?;

    let received = tokio::time::timeout(STATUS_TIMEOUT, rx.recv())
        .await
        .map_err(|_| timeout())?
        .ok_or_else(|| AppError::service_unavailable(format!("No response from {}", config.name)))?
        .map_err(|error| net::unreachable(DEVICE, error))?;

    let refresh_payload =
        Payload::new(config.id.clone(), Some(config.id.clone()), Some(config.id.clone()), now, Some(DpId::Higher), None);
    // Best effort: a device that ignores the refresh still answered the status.
    if let Ok(Err(error)) = tokio::time::timeout(STATUS_TIMEOUT, device.refresh(refresh_payload)).await {
        tracing::debug!(device = %config.name, %error, "tuya refresh refused");
    }
    let maybe_refresh = tokio::time::timeout(MESSAGE_DRAIN_TIMEOUT, rx.recv()).await.ok();

    let mut merged = merge_messages_into_dps(Map::new(), received);
    if let Some(Some(Ok(messages))) = maybe_refresh {
        merged = merge_messages_into_dps(merged, messages);
    }
    while let Ok(Some(Ok(messages))) = tokio::time::timeout(COMMAND_SETTLE_TIMEOUT, rx.recv()).await {
        merged = merge_messages_into_dps(merged, messages);
    }

    Ok(merged)
}

async fn worker_send_commands(
    device: &mut TuyaDevice,
    rx: &mut mpsc::Receiver<rust_async_tuyapi::Result<Vec<Message>>>,
    config: &TuyaDeviceConfig,
    updates: &[(String, Value)],
) -> Result<(), AppError> {
    for (dps, value) in updates {
        tokio::time::timeout(STATUS_TIMEOUT, device.set_values(command_payload(dps, value)))
            .await
            .map_err(|_| AppError::service_unavailable(format!("Command timeout for {}", config.name)))?
            .map_err(|error| net::unreachable(DEVICE, error))?;

        // the device's echo, if any, is read by the session afterwards
        tokio::time::timeout(COMMAND_SETTLE_TIMEOUT, rx.recv()).await.ok();
    }
    Ok(())
}

/// What a device is sent to set one data point: `{"<dps>": value}`.
fn command_payload(dps: &str, value: &Value) -> Value {
    let mut payload = Map::new();
    payload.insert(dps.to_string(), value.clone());
    Value::Object(payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tuya::dps;

    #[test]
    fn a_command_sets_one_data_point() {
        assert_eq!(command_payload(dps::litter::CHILD_LOCK, &json!(true)), json!({ "110": true }));
        assert_eq!(command_payload(dps::feeder::MANUAL_FEED, &json!(2)), json!({ "3": 2 }));
    }

    #[test]
    fn back_off_doubles_up_to_a_minute() {
        assert_eq!(reconnect_delay(-3), Duration::from_secs(1));
        assert_eq!(reconnect_delay(0), Duration::from_secs(1));
        assert_eq!(reconnect_delay(1), Duration::from_secs(1));
        assert_eq!(reconnect_delay(3), Duration::from_secs(4));
        assert_eq!(reconnect_delay(40), Duration::from_secs(60));
        assert_eq!(reconnect_delay(i32::MAX), Duration::from_secs(60));
    }
}
