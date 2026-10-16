//! One worker per device owns its session: it connects, keeps the link alive, reads what
//! the device pushes and runs the commands it is handed, and reconnects with a back-off
//! when the device drops. Its state is published on the device's `watch` channel
//! ([`Link`]), cleared by a guard when the worker ends however it ends (a panic too), and a
//! supervisor starts a fresh worker after a panic. What the device says (a status read, a
//! push, a command's echo) goes into the device's status (`TuyaManager::absorb`).

use std::{
    net::IpAddr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use rust_async_tuyapi::{mesparse::Message, tuyadevice::TuyaDevice, DpId, Payload, PayloadStruct};
use serde_json::{json, Map, Value};
use tokio::{
    sync::{mpsc, oneshot, watch, Notify},
    task::JoinHandle,
};

use super::{parse::merge_messages_into_dps, ManagedTuyaDevice, TuyaManager, DEVICE};
use crate::{error::AppError, net, util::capped_backoff};

pub(super) const STATUS_TIMEOUT: Duration = Duration::from_millis(12_000);
/// The protocol says no end of answer: what comes within this after a status read is part
/// of it.
const MESSAGE_DRAIN_TIMEOUT: Duration = Duration::from_millis(1_500);
/// A command's echo comes within this, and the next command waits that long anyway (the
/// device takes one data point at a time).
const COMMAND_SETTLE_TIMEOUT: Duration = Duration::from_millis(400);
const HEARTBEAT_INTERVAL: Duration = Duration::from_millis(30_000);
pub(super) const COMMAND_REPLY_TIMEOUT: Duration = Duration::from_millis(20_000);
const RECONNECT_BASE_DELAY: Duration = Duration::from_secs(1);
const RECONNECT_MAX_DELAY: Duration = Duration::from_secs(60);

/// What the device library hands the session: the device's messages, as they come.
type Messages = mpsc::Receiver<rust_async_tuyapi::Result<Vec<Message>>>;

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

#[cfg(test)]
impl WorkerParts {
    /// What the worker would be handed (a test plays the worker).
    pub(super) fn commands(&mut self) -> &mut mpsc::Receiver<WorkerCommand> {
        &mut self.commands
    }
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
            *link = Link {
                worker: None,
                connected: false,
                connecting: false,
                ..*link
            };
            true
        });
    }
}

#[derive(Debug)]
pub(super) enum WorkerCommand {
    /// Reads the device's whole status into it, unless the session already did and no
    /// `refresh` is asked (two callers that found it unread wait for one read).
    FetchStatus {
        refresh: bool,
        reply: oneshot::Sender<Result<(), AppError>>,
    },
    SetValues {
        updates: Vec<(String, Value)>,
        reply: oneshot::Sender<Result<(), AppError>>,
    },
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
        let exit = WorkerExit {
            id,
            link: device.link.clone(),
            live: self.live_workers.clone(),
            _exited: exited_tx,
        };
        (
            WorkerHandle {
                id,
                commands: commands_tx,
                wake: wake.clone(),
                stop: stop_tx,
                exited: exited_rx,
            },
            WorkerParts {
                id,
                commands: commands_rx,
                wake,
                stop: StopSignal(stop_rx),
                _exit: exit,
            },
        )
    }

    /// Runs the worker, under a supervisor.
    pub(super) fn spawn_worker(&self, device: &ManagedTuyaDevice, parts: WorkerParts) {
        let id = parts.id;
        let worker = tokio::spawn(
            self.clone()
                .run_device_worker(device.config.id.clone(), parts),
        );
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
        let Ok(attempts) = self
            .device(&device_id)
            .map(|device| device.link.borrow().reconnect_attempts)
        else {
            return;
        };
        tokio::time::sleep(reconnect_delay(attempts)).await;
        let still_dead = self
            .workers
            .read()
            .await
            .get(&device_id)
            .is_some_and(|handle| handle.id == id);
        if still_dead {
            if let Err(error) = self.connect_device(&device_id).await {
                tracing::warn!(%device_id, %error, "tuya device not connected after a worker panic");
            }
        }
    }

    /// Changes the device's link only while `worker` still owns it: a cancelled worker
    /// finishing late must not mark a disconnected device connected.
    fn update_own_link(
        &self,
        device: &ManagedTuyaDevice,
        worker: u64,
        change: impl FnOnce(&mut Link),
    ) {
        device.link.send_if_modified(|link| {
            if link.worker != Some(worker) {
                return false;
            }
            change(link);
            true
        });
    }

    fn mark_failed(&self, device: &ManagedTuyaDevice, worker: u64) {
        self.update_own_link(device, worker, |link| {
            link.connected = false;
            link.connecting = false;
        });
    }

    async fn run_device_worker(self, device_id: String, parts: WorkerParts) {
        let WorkerParts {
            id,
            mut commands,
            wake,
            mut stop,
            _exit,
        } = parts;
        let Ok(device) = self.device(&device_id) else {
            return;
        };
        loop {
            // Dropping the session on stop drops the device, which closes its socket.
            tokio::select! {
                _ = stop.wait() => return,
                _ = self.run_device_worker_session(device, id, &mut commands) => {}
            }

            let attempts = device.link.borrow().reconnect_attempts;
            let nudged = tokio::select! {
                _ = stop.wait() => return,
                _ = tokio::time::sleep(reconnect_delay(attempts)) => false,
                _ = wake.notified() => true,
            };
            // A nudge already counted its attempt.
            if !nudged {
                self.update_own_link(device, id, |link| {
                    link.connecting = true;
                    link.reconnect_attempts += 1;
                });
            }
        }
    }

    /// One session: connect, read the status, then serve heartbeats, pushed values and
    /// commands until the device fails (the caller then waits and retries).
    async fn run_device_worker_session(
        &self,
        device: &ManagedTuyaDevice,
        worker: u64,
        commands: &mut mpsc::Receiver<WorkerCommand>,
    ) {
        let config = &device.config;
        let Some(ip) = device.address else {
            return self.mark_failed(device, worker);
        };
        // a new session has read nothing yet
        device
            .status
            .send_if_modified(|status| std::mem::replace(&mut status.read, false));
        let mut tuya = match TuyaDevice::new(
            &config.version,
            &config.id,
            Some(&config.key),
            IpAddr::V4(ip),
        ) {
            Ok(tuya) => tuya,
            Err(error) => {
                tracing::warn!(device = %config.name, %error, "tuya device not set up (its version or key?)");
                return self.mark_failed(device, worker);
            }
        };
        let mut rx = match tokio::time::timeout(STATUS_TIMEOUT, tuya.connect()).await {
            Ok(Ok(rx)) => rx,
            Ok(Err(error)) => {
                tracing::debug!(device = %config.name, %error, "tuya device did not connect");
                return self.mark_failed(device, worker);
            }
            Err(_) => {
                tracing::debug!(device = %config.name, "tuya device did not connect in time");
                return self.mark_failed(device, worker);
            }
        };

        self.update_own_link(device, worker, |link| {
            link.connected = true;
            link.connecting = false;
            link.reconnect_attempts = 0;
        });
        self.read_status(device, &mut tuya, &mut rx).await.ok();

        let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
        loop {
            tokio::select! {
                _ = heartbeat.tick() => {
                    if let Err(error) = tuya.heartbeat().await {
                        tracing::debug!(device = %config.name, %error, "tuya heartbeat failed");
                        break;
                    }
                }
                messages = rx.recv() => {
                    let messages = match messages {
                        Some(Ok(messages)) => messages,
                        Some(Err(error)) => {
                            tracing::debug!(device = %config.name, %error, "tuya session broke");
                            break;
                        }
                        None => break,
                    };
                    self.absorb(device, merge_messages_into_dps(Map::new(), messages), false).await;
                }
                command = commands.recv() => match command {
                    // a caller that gave up waiting has dropped its receiver: nothing to tell it
                    Some(WorkerCommand::FetchStatus { refresh, reply }) => {
                        let read = if !refresh && device.status.borrow().read {
                            Ok(())
                        } else {
                            self.read_status(device, &mut tuya, &mut rx).await
                        };
                        reply.send(read).ok();
                    }
                    Some(WorkerCommand::SetValues { updates, reply }) => {
                        let sent = self.set_values(device, &mut tuya, &mut rx, updates).await;
                        reply.send(sent).ok();
                    }
                    None => break,
                },
            }
        }
        self.mark_failed(device, worker);
        if let Err(error) = tuya.disconnect().await {
            tracing::debug!(device = %config.name, %error, "tuya session closed uncleanly");
        }
    }

    /// The device's whole status, read into it.
    async fn read_status(
        &self,
        device: &ManagedTuyaDevice,
        tuya: &mut TuyaDevice,
        rx: &mut Messages,
    ) -> Result<(), AppError> {
        let read = fetch_status(tuya, rx, &device.config.id).await;
        match read {
            Ok(dps) => {
                self.absorb(device, dps, true).await;
                Ok(())
            }
            Err(error) => {
                tracing::debug!(device = %device.config.name, %error, "tuya status not read");
                Err(net::unreachable(DEVICE, error))
            }
        }
    }

    /// Sets each data point in turn; what was set, then what the device echoes, goes into
    /// its status.
    async fn set_values(
        &self,
        device: &ManagedTuyaDevice,
        tuya: &mut TuyaDevice,
        rx: &mut Messages,
        updates: Vec<(String, Value)>,
    ) -> Result<(), AppError> {
        for (dps, value) in updates {
            match tokio::time::timeout(
                STATUS_TIMEOUT,
                tuya.set_values(command_payload(&dps, &value)),
            )
            .await
            {
                Ok(Ok(_)) => {}
                Ok(Err(error)) => return Err(net::unreachable(DEVICE, error)),
                Err(_) => {
                    return Err(net::unreachable(
                        DEVICE,
                        format!("{}: no answer to a command", device.config.name),
                    ))
                }
            }
            self.absorb(device, Map::from_iter([(dps, value)]), false)
                .await;
            // the device's echo (what it really set), if it says one
            if let Ok(Some(Ok(echo))) =
                tokio::time::timeout(COMMAND_SETTLE_TIMEOUT, rx.recv()).await
            {
                self.absorb(device, merge_messages_into_dps(Map::new(), echo), false)
                    .await;
            }
        }
        Ok(())
    }
}

pub(super) fn reconnect_delay(attempts: i32) -> Duration {
    let attempt = u32::try_from(attempts.max(1) - 1).unwrap_or(0);
    capped_backoff(RECONNECT_BASE_DELAY, attempt, RECONNECT_MAX_DELAY)
}

/// A status read: the answer, the refresh's, and what follows within the drain windows.
async fn fetch_status(
    tuya: &mut TuyaDevice,
    rx: &mut Messages,
    id: &str,
) -> Result<Map<String, Value>, String> {
    // the protocol's `t` is a u32 of seconds: a clock past 2106 sends none
    let now = u32::try_from(chrono::Utc::now().timestamp()).ok();

    let payload = Payload::Struct(PayloadStruct {
        gw_id: Some(id.to_string()),
        dev_id: id.to_string(),
        uid: Some(id.to_string()),
        t: now.map(|t| t.to_string()),
        dp_id: None,
        dps: Some(json!({})),
    });

    tokio::time::timeout(STATUS_TIMEOUT, tuya.get(payload))
        .await
        .map_err(|_| "status request timed out".to_string())?
        .map_err(|error| error.to_string())?;

    let received = tokio::time::timeout(STATUS_TIMEOUT, rx.recv())
        .await
        .map_err(|_| "no status in time".to_string())?
        .ok_or_else(|| "the session closed".to_string())?
        .map_err(|error| error.to_string())?;

    let refresh_payload = Payload::new(
        id.to_string(),
        Some(id.to_string()),
        Some(id.to_string()),
        now,
        Some(DpId::Higher),
        None,
    );
    // Best effort: a device that ignores the refresh still answered the status.
    if let Ok(Err(error)) =
        tokio::time::timeout(STATUS_TIMEOUT, tuya.refresh(refresh_payload)).await
    {
        tracing::debug!(%id, %error, "tuya refresh refused");
    }

    let mut merged = merge_messages_into_dps(Map::new(), received);
    if let Ok(Some(Ok(messages))) = tokio::time::timeout(MESSAGE_DRAIN_TIMEOUT, rx.recv()).await {
        merged = merge_messages_into_dps(merged, messages);
    }
    while let Ok(Some(Ok(messages))) = tokio::time::timeout(COMMAND_SETTLE_TIMEOUT, rx.recv()).await
    {
        merged = merge_messages_into_dps(merged, messages);
    }
    Ok(merged)
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
        assert_eq!(
            command_payload(dps::litter::CHILD_LOCK, &json!(true)),
            json!({ "110": true })
        );
        assert_eq!(
            command_payload(dps::feeder::MANUAL_FEED, &json!(2)),
            json!({ "3": 2 })
        );
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
