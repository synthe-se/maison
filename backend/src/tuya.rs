use std::{
    collections::HashMap,
    net::IpAddr,
    path::{Path, PathBuf},
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

use rust_async_tuyapi::{DpId, Payload, PayloadStruct, mesparse::Message, tuyadevice::TuyaDevice};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use tokio::sync::{Mutex, Notify, RwLock, mpsc, oneshot, watch};

use crate::{
    error::AppError,
    store::{self, Access, Corrupt},
};

const STATUS_TIMEOUT_MS: u64 = 12_000;
const MESSAGE_DRAIN_TIMEOUT_MS: u64 = 1_500;
const COMMAND_SETTLE_TIMEOUT_MS: u64 = 400;
const HEARTBEAT_INTERVAL_MS: u64 = 30_000;
const COMMAND_REPLY_TIMEOUT_MS: u64 = 20_000;
const RECONNECT_BASE_DELAY_MS: u64 = 1_000;
const RECONNECT_MAX_DELAY_MS: u64 = 60_000;

/// The data points (DPS) of each device type, said once: the status parsers read them and
/// the routes write them.
pub mod dps {
    pub mod feeder {
        pub const MEAL_PLAN: &str = "1";
        pub const QUICK_FEED: &str = "2";
        pub const MANUAL_FEED: &str = "3";
        pub const FAULT: &str = "14";
        pub const FEED_REPORT: &str = "15";
        pub const FEED_SIZE: &str = "101";
        pub const MANUAL_FEED_ENABLED: &str = "102";
        pub const SOUND: &str = "103";
        pub const HISTORY: &str = "104";
        pub const POWER_MODE: &str = "105";
        pub const ALEXA_FEED: &str = "106";
        pub const IP_ADDRESS: &str = "107";
    }

    pub mod litter {
        pub const CLEAN_DELAY: &str = "101";
        pub const SLEEP_ENABLED: &str = "102";
        pub const SLEEP_START: &str = "103";
        pub const SLEEP_END: &str = "104";
        pub const DEFECATION_FREQUENCY: &str = "105";
        pub const DEFECATION_DURATION: &str = "106";
        /// Written to start a cycle, read as « cleaning in progress ».
        pub const CLEAN: &str = "107";
        pub const MAINTENANCE: &str = "108";
        pub const STATE: &str = "109";
        pub const CHILD_LOCK: &str = "110";
        pub const KITTEN_MODE: &str = "111";
        pub const LITTER_LEVEL: &str = "112";
        pub const RESET_SAND_LEVEL: &str = "113";
        pub const FAULT_ALARM: &str = "114";
        pub const FACTORY_RESET: &str = "115";
        pub const LIGHTING: &str = "116";
        pub const PROMPT_SOUND: &str = "117";
        pub const AUTOMATIC_HOMING: &str = "119";
    }

    pub mod fountain {
        pub const POWER: &str = "1";
        pub const WATER_TIME: &str = "3";
        pub const FILTER_LIFE: &str = "4";
        pub const PUMP_TIME: &str = "5";
        pub const WATER_RESET: &str = "6";
        pub const FILTER_RESET: &str = "7";
        pub const PUMP_RESET: &str = "8";
        pub const UV: &str = "10";
        pub const UV_RUNTIME: &str = "11";
        pub const WATER_LEVEL: &str = "12";
        pub const LOW_WATER: &str = "101";
        pub const ECO_MODE: &str = "102";
        pub const ECO_WATERING_STATUS: &str = "103";
        pub const NO_WATER: &str = "104";
        pub const ASSOCIATED_CAMERA: &str = "110";
        pub const MAC_ADDRESS: &str = "130";
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TuyaDeviceConfig {
    pub name: String,
    pub id: String,
    pub key: String,
    pub category: String,
    pub product_name: String,
    pub port: Option<u16>,
    pub model: Option<String>,
    pub ip: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TuyaDeviceListEntry {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub device_type: String,
    pub product_name: String,
    pub model: Option<String>,
    pub ip: String,
    pub version: String,
    pub connected: bool,
    pub connecting: bool,
    pub reconnect_attempts: i32,
    pub last_data: TuyaStatusPayload,
    pub parsed_data: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct TuyaStatusPayload {
    pub dps: Map<String, Value>,
}

/// A device as the API names it in its answers (Tuya and Meross alike).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeviceRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct DeviceCache(HashMap<String, HashMap<String, Value>>);

#[derive(Debug, Clone)]
pub struct TuyaManager {
    devices: Arc<HashMap<String, ManagedTuyaDevice>>,
    cache: Arc<RwLock<DeviceCache>>,
    cache_path: Arc<PathBuf>,
    /// Held across a cache write, so writes land in order and the last one is the newest.
    cache_writer: Arc<Mutex<()>>,
    runtime: Arc<RwLock<HashMap<String, RuntimeDeviceState>>>,
    next_worker: Arc<AtomicU64>,
    live_workers: Arc<AtomicUsize>,
}

#[derive(Debug, Clone)]
struct ManagedTuyaDevice {
    config: TuyaDeviceConfig,
    device_type: TuyaDeviceType,
}

#[derive(Debug)]
struct RuntimeDeviceState {
    connected: bool,
    connecting: bool,
    reconnect_attempts: i32,
    last_data: Map<String, Value>,
    parsed_data: Value,
    /// The one worker that owns the device's session; `None` when nobody does.
    worker: Option<WorkerHandle>,
}

/// The manager's side of a worker. Dropping it cancels the worker (its stop sender goes).
#[derive(Debug)]
struct WorkerHandle {
    id: u64,
    commands: mpsc::Sender<WorkerCommand>,
    /// Cuts a retry back-off short when someone asks to connect now.
    wake: Arc<Notify>,
    /// Never sent on: the worker stops when this is dropped.
    stop: watch::Sender<()>,
    /// Resolves (with an error) once the worker task is gone.
    exited: oneshot::Receiver<()>,
}

/// The worker's side: what it listens to and what tells the manager it is gone.
struct WorkerParts {
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

/// Dropped when the worker task ends: counts it out and wakes whoever waits for its end.
struct WorkerExit {
    live: Arc<AtomicUsize>,
    _exited: oneshot::Sender<()>,
}

impl Drop for WorkerExit {
    fn drop(&mut self) {
        self.live.fetch_sub(1, Ordering::SeqCst);
    }
}

#[derive(Debug)]
enum WorkerCommand {
    FetchStatus {
        reply: oneshot::Sender<Result<Map<String, Value>, String>>,
    },
    SetValues {
        updates: Vec<(String, Value)>,
        reply: oneshot::Sender<Result<(), String>>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuyaDeviceType {
    Feeder,
    LitterBox,
    Fountain,
    Unknown,
}

impl TuyaManager {
    pub fn new(devices_path: &Path, cache_path: &Path) -> Result<Self, AppError> {
        let configs = store::read_json::<Vec<TuyaDeviceConfig>>(devices_path, Corrupt::Fail)?;
        let cache = store::read_json::<DeviceCache>(cache_path, Corrupt::Reset)?;

        let devices = configs
            .into_iter()
            .map(|config| {
                let device_type = determine_device_type(&config);
                (config.id.clone(), ManagedTuyaDevice { config, device_type })
            })
            .collect::<HashMap<_, _>>();

        let runtime = devices
            .values()
            .map(|device| {
                let cached = cache_entry_dps(&cache, &device.config.id);
                (
                    device.config.id.clone(),
                    RuntimeDeviceState {
                        connected: false,
                        connecting: false,
                        reconnect_attempts: 0,
                        parsed_data: parse_device_data(device.device_type, &cached),
                        last_data: cached,
                        worker: None,
                    },
                )
            })
            .collect::<HashMap<_, _>>();

        Ok(Self {
            devices: Arc::new(devices),
            cache: Arc::new(RwLock::new(cache)),
            cache_path: Arc::new(cache_path.to_path_buf()),
            cache_writer: Arc::new(Mutex::new(())),
            runtime: Arc::new(RwLock::new(runtime)),
            next_worker: Arc::new(AtomicU64::new(1)),
            live_workers: Arc::new(AtomicUsize::new(0)),
        })
    }

    pub fn device_ids(&self) -> Vec<String> {
        self.devices.keys().cloned().collect()
    }

    pub async fn list_devices(&self) -> Vec<TuyaDeviceListEntry> {
        let runtime = self.runtime.read().await;
        self.devices
            .values()
            .filter_map(|device| {
                let state = runtime.get(&device.config.id)?;
                Some(TuyaDeviceListEntry {
                    id: device.config.id.clone(),
                    name: device.config.name.clone(),
                    device_type: device.device_type.as_str().to_string(),
                    product_name: device.config.product_name.clone(),
                    model: device.config.model.clone(),
                    ip: device.config.ip.clone(),
                    version: device.config.version.clone(),
                    connected: state.connected,
                    connecting: state.connecting,
                    reconnect_attempts: state.reconnect_attempts,
                    last_data: TuyaStatusPayload { dps: state.last_data.clone() },
                    parsed_data: state.parsed_data.clone(),
                })
            })
            .collect()
    }

    pub async fn get_status(&self, device_id: &str) -> Result<(DeviceRef, Map<String, Value>, Value), AppError> {
        let device = self.device(device_id)?;
        let dps = self.fetch_device_dps(device).await?;
        let parsed = match device.device_type {
            TuyaDeviceType::Fountain => Value::Null,
            other => parse_device_data(other, &dps),
        };
        Ok((device.reference(), dps, parsed))
    }

    pub async fn get_typed_status(
        &self,
        device_id: &str,
        expected: TuyaDeviceType,
    ) -> Result<(DeviceRef, Map<String, Value>, Value), AppError> {
        let device = self.typed_device(device_id, expected)?;
        let dps = self.fetch_device_dps(device).await?;
        let parsed = parse_device_data(device.device_type, &dps);
        Ok((device.reference(), dps, parsed))
    }

    pub async fn send_typed_command(
        &self,
        device_id: &str,
        expected: TuyaDeviceType,
        dps: &str,
        value: Value,
    ) -> Result<DeviceRef, AppError> {
        self.send_typed_commands(device_id, expected, vec![(dps.to_string(), value)])
            .await
    }

    pub fn get_device_ref(&self, device_id: &str) -> Result<DeviceRef, AppError> {
        Ok(self.device(device_id)?.reference())
    }

    pub async fn send_typed_commands(
        &self,
        device_id: &str,
        expected: TuyaDeviceType,
        updates: Vec<(String, Value)>,
    ) -> Result<DeviceRef, AppError> {
        let device = self.typed_device(device_id, expected)?;
        self.send_device_commands(device, &updates).await?;
        let cacheable = updates
            .iter()
            .filter(|(dps_id, _)| cacheable_dps_keys(device.device_type).contains(&dps_id.as_str()))
            .cloned()
            .collect();
        self.apply_cache_updates(&device.config.id, cacheable).await;
        Ok(device.reference())
    }

    pub async fn feeder_meal_plan(&self, device_id: &str) -> Result<(DeviceRef, Option<String>), AppError> {
        let device = self.typed_device(device_id, TuyaDeviceType::Feeder)?;
        let dps = self.fetch_device_dps(device).await?;
        let plan = dps.get(dps::feeder::MEAL_PLAN).and_then(Value::as_str).map(ToOwned::to_owned);
        Ok((device.reference(), plan))
    }

    fn device(&self, device_id: &str) -> Result<&ManagedTuyaDevice, AppError> {
        self.devices
            .get(device_id)
            .ok_or_else(|| AppError::not_found("Device not found"))
    }

    fn typed_device(&self, device_id: &str, expected: TuyaDeviceType) -> Result<&ManagedTuyaDevice, AppError> {
        let device = self.device(device_id)?;
        if device.device_type != expected {
            let message = match expected {
                TuyaDeviceType::Feeder => "Device is not a feeder",
                TuyaDeviceType::LitterBox => "Device is not a litter box",
                TuyaDeviceType::Fountain => "Device is not a fountain",
                TuyaDeviceType::Unknown => "Unsupported device type",
            };
            return Err(AppError::bad_request(message));
        }
        Ok(device)
    }

    /// Makes sure one worker owns the device's session and waits for it to be connected.
    /// Checking and claiming happen under one write lock: two callers never spawn two
    /// workers (two TCP sessions fighting over one device).
    pub async fn connect_device(&self, device_id: &str) -> Result<(), AppError> {
        let device = self.device(device_id)?;

        let spawn = {
            let mut runtime = self.runtime.write().await;
            let Some(state) = runtime.get_mut(device_id) else {
                return Err(AppError::not_found("Device not found"));
            };
            match &state.worker {
                Some(_) if state.connected || state.connecting => None,
                Some(worker) => {
                    // A worker waiting out its back-off: retry now.
                    state.connecting = true;
                    state.reconnect_attempts += 1;
                    worker.wake.notify_one();
                    None
                }
                None => {
                    let (handle, parts) = self.new_worker();
                    state.worker = Some(handle);
                    state.connecting = true;
                    if !state.connected {
                        state.reconnect_attempts += 1;
                    }
                    Some(parts)
                }
            }
        };

        if let Some(parts) = spawn {
            let manager = self.clone();
            let config = device.config.clone();
            let device_type = device.device_type;
            tokio::spawn(async move { manager.run_device_worker(config, device_type, parts).await });
        }

        let connected = tokio::time::timeout(Duration::from_millis(STATUS_TIMEOUT_MS), async {
            loop {
                if let Some((connected, connecting)) = self.connection_flags(device_id).await {
                    if connected || !connecting {
                        return connected;
                    }
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .unwrap_or(false);

        if connected {
            Ok(())
        } else {
            Err(AppError::service_unavailable(format!("Failed to connect to {}", device.config.name)))
        }
    }

    fn new_worker(&self) -> (WorkerHandle, WorkerParts) {
        let id = self.next_worker.fetch_add(1, Ordering::SeqCst);
        let (commands_tx, commands_rx) = mpsc::channel(16);
        let (stop_tx, stop_rx) = watch::channel(());
        let (exited_tx, exited_rx) = oneshot::channel();
        let wake = Arc::new(Notify::new());
        self.live_workers.fetch_add(1, Ordering::SeqCst);
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
                _exit: WorkerExit { live: self.live_workers.clone(), _exited: exited_tx },
            },
        )
    }

    /// Cancels the device's worker (connected, connecting or waiting to retry) and waits
    /// for it to be gone, so a following connect starts the only session.
    pub async fn disconnect_device(&self, device_id: &str) -> Result<(), AppError> {
        self.device(device_id)?;
        let worker = {
            let mut runtime = self.runtime.write().await;
            runtime.get_mut(device_id).and_then(|state| {
                state.connected = false;
                state.connecting = false;
                state.worker.take()
            })
        };
        if let Some(worker) = worker {
            let WorkerHandle { exited, stop, .. } = worker;
            drop(stop);
            let _ = tokio::time::timeout(Duration::from_millis(STATUS_TIMEOUT_MS), exited).await;
        }
        Ok(())
    }

    pub async fn reconnect_disconnected(&self) {
        let device_ids = {
            let runtime = self.runtime.read().await;
            runtime
                .iter()
                .filter(|(_, state)| !state.connected)
                .map(|(device_id, _)| device_id.clone())
                .collect::<Vec<_>>()
        };
        for device_id in device_ids {
            let _ = self.connect_device(&device_id).await;
        }
    }

    pub async fn connect_all_devices(&self) {
        for device_id in self.device_ids() {
            if let Err(error) = self.connect_device(&device_id).await {
                tracing::warn!(%device_id, %error, "tuya device not connected");
            }
        }
    }

    pub async fn disconnect_all_devices(&self) {
        for device_id in self.device_ids() {
            let _ = self.disconnect_device(&device_id).await;
        }
    }

    pub async fn connection_stats(&self) -> TuyaConnectionStats {
        let runtime = self.runtime.read().await;
        let devices = self
            .devices
            .values()
            .map(|device| {
                let state = runtime.get(&device.config.id);
                TuyaConnectionStatsEntry {
                    id: device.config.id.clone(),
                    name: device.config.name.clone(),
                    device_type: device.device_type.as_str().to_string(),
                    connected: state.is_some_and(|state| state.connected),
                    connecting: state.is_some_and(|state| state.connecting),
                    reconnect_attempts: state.map(|state| state.reconnect_attempts).unwrap_or_default(),
                }
            })
            .collect::<Vec<_>>();
        let connected = devices.iter().filter(|device| device.connected).count();

        TuyaConnectionStats {
            total: devices.len(),
            connected,
            disconnected: devices.len().saturating_sub(connected),
            devices,
        }
    }

    async fn fetch_device_dps(&self, device: &ManagedTuyaDevice) -> Result<Map<String, Value>, AppError> {
        let config = &device.config;
        self.connect_device(&config.id).await?;

        let (reply, answer) = oneshot::channel();
        self.send_to_worker(config, WorkerCommand::FetchStatus { reply }).await?;
        let mut dps = tokio::time::timeout(Duration::from_millis(COMMAND_REPLY_TIMEOUT_MS), answer)
            .await
            .map_err(|_| AppError::service_unavailable(format!("Status request timeout for {}", config.name)))?
            .map_err(|_| AppError::service_unavailable(format!("{} worker dropped status reply", config.name)))?
            .map_err(AppError::service_unavailable)?;

        self.record_dps(&config.id, device.device_type, &dps).await;
        self.store_cacheable_dps(&config.id, device.device_type, &dps).await;
        self.merge_cached_dps(&mut dps, device.device_type, &config.id).await;
        Ok(dps)
    }

    async fn send_device_commands(
        &self,
        device: &ManagedTuyaDevice,
        updates: &[(String, Value)],
    ) -> Result<(), AppError> {
        let config = &device.config;
        let (reply, answer) = oneshot::channel();
        self.send_to_worker(config, WorkerCommand::SetValues { updates: updates.to_vec(), reply })
            .await?;
        tokio::time::timeout(Duration::from_millis(COMMAND_REPLY_TIMEOUT_MS), answer)
            .await
            .map_err(|_| AppError::service_unavailable(format!("Command timeout for {}", config.name)))?
            .map_err(|_| AppError::service_unavailable(format!("{} worker dropped command reply", config.name)))?
            .map_err(AppError::service_unavailable)?;

        let mut runtime = self.runtime.write().await;
        if let Some(state) = runtime.get_mut(&config.id) {
            for (key, value) in updates {
                state.last_data.insert(key.clone(), value.clone());
            }
            state.parsed_data = parse_device_data(device.device_type, &state.last_data);
        }
        Ok(())
    }

    /// Hands `command` to the device's worker; a device without a connected session fails
    /// at once (a worker waiting to retry reads no command).
    async fn send_to_worker(&self, config: &TuyaDeviceConfig, command: WorkerCommand) -> Result<(), AppError> {
        let sender = {
            let runtime = self.runtime.read().await;
            runtime
                .get(&config.id)
                .filter(|state| state.connected)
                .and_then(|state| state.worker.as_ref())
                .map(|worker| worker.commands.clone())
        };
        let sender = sender.ok_or_else(|| AppError::service_unavailable(format!("{} is not connected", config.name)))?;
        sender
            .send(command)
            .await
            .map_err(|_| AppError::service_unavailable(format!("{} worker is unavailable", config.name)))
    }

    async fn connection_flags(&self, device_id: &str) -> Option<(bool, bool)> {
        self.runtime
            .read()
            .await
            .get(device_id)
            .map(|state| (state.connected, state.connecting))
    }

    /// Changes the device's state only while `worker` still owns it: a cancelled worker
    /// finishing late must not mark a disconnected device connected.
    async fn update_own_state(&self, device_id: &str, worker: u64, change: impl FnOnce(&mut RuntimeDeviceState)) {
        let mut runtime = self.runtime.write().await;
        if let Some(state) = runtime.get_mut(device_id) {
            if state.worker.as_ref().is_some_and(|handle| handle.id == worker) {
                change(state);
            }
        }
    }

    async fn mark_failed(&self, device_id: &str, worker: u64) {
        self.update_own_state(device_id, worker, |state| {
            state.connected = false;
            state.connecting = false;
        })
        .await;
    }

    /// The device's latest values, as the list shows them.
    async fn record_dps(&self, device_id: &str, device_type: TuyaDeviceType, dps: &Map<String, Value>) {
        let mut runtime = self.runtime.write().await;
        if let Some(state) = runtime.get_mut(device_id) {
            state.last_data = dps.clone();
            state.parsed_data = parse_device_data(device_type, dps);
        }
    }

    async fn merge_cached_dps(&self, merged: &mut Map<String, Value>, device_type: TuyaDeviceType, device_id: &str) {
        let cache = self.cache.read().await;
        let Some(cached) = cache.0.get(device_id) else {
            return;
        };
        for dps_id in cacheable_dps_keys(device_type) {
            if let Some(value) = cached.get(*dps_id) {
                merged.entry((*dps_id).to_string()).or_insert_with(|| value.clone());
            }
        }
    }

    async fn store_cacheable_dps(&self, device_id: &str, device_type: TuyaDeviceType, dps: &Map<String, Value>) {
        let updates = cacheable_dps_keys(device_type)
            .iter()
            .filter_map(|key| dps.get(*key).cloned().map(|value| ((*key).to_string(), value)))
            .collect::<Vec<_>>();
        self.apply_cache_updates(device_id, updates).await;
    }

    /// Puts `updates` in the cache and rewrites the file only when they changed it: the
    /// devices push their values all day long, the SD card need not hear each one.
    async fn apply_cache_updates(&self, device_id: &str, updates: Vec<(String, Value)>) {
        if updates.is_empty() {
            return;
        }
        let _writer = self.cache_writer.lock().await;
        let snapshot = {
            let mut cache = self.cache.write().await;
            let entry = cache.0.entry(device_id.to_string()).or_default();
            if !merge_cache_updates(entry, updates) {
                return;
            }
            cache.clone()
        };
        let path = self.cache_path.clone();
        let written = tokio::task::spawn_blocking(move || store::write_json(&path, &snapshot, Access::Shared)).await;
        match written {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::warn!(%error, path = %self.cache_path.display(), "tuya device cache not saved"),
            Err(error) => tracing::warn!(%error, path = %self.cache_path.display(), "tuya device cache not saved"),
        }
    }

    async fn run_device_worker(self, config: TuyaDeviceConfig, device_type: TuyaDeviceType, parts: WorkerParts) {
        let WorkerParts { id, mut commands, wake, mut stop, _exit } = parts;
        loop {
            // Dropping the session on stop drops the device, which closes its socket.
            tokio::select! {
                _ = stop.wait() => return,
                _ = self.run_device_worker_session(&config, device_type, id, &mut commands) => {}
            }

            let attempts = {
                let runtime = self.runtime.read().await;
                runtime.get(&config.id).map(|state| state.reconnect_attempts).unwrap_or(1)
            };
            let nudged = tokio::select! {
                _ = stop.wait() => return,
                _ = tokio::time::sleep(reconnect_delay(attempts)) => false,
                _ = wake.notified() => true,
            };
            // A nudge already counted its attempt.
            if !nudged {
                self.update_own_state(&config.id, id, |state| {
                    state.connecting = true;
                    state.reconnect_attempts += 1;
                })
                .await;
            }
        }
    }

    /// One session: connect, then serve heartbeats, pushed values and commands until the
    /// device fails (the caller then waits and retries).
    async fn run_device_worker_session(
        &self,
        config: &TuyaDeviceConfig,
        device_type: TuyaDeviceType,
        worker: u64,
        commands: &mut mpsc::Receiver<WorkerCommand>,
    ) {
        let Ok(ip) = IpAddr::from_str(&config.ip) else {
            tracing::warn!(device = %config.name, ip = %config.ip, "tuya device has an invalid IP address");
            return self.mark_failed(&config.id, worker).await;
        };
        let Ok(mut device) = TuyaDevice::new(&config.version, &config.id, Some(&config.key), ip) else {
            return self.mark_failed(&config.id, worker).await;
        };
        let Ok(Ok(mut rx)) = tokio::time::timeout(Duration::from_millis(STATUS_TIMEOUT_MS), device.connect()).await
        else {
            return self.mark_failed(&config.id, worker).await;
        };

        self.update_own_state(&config.id, worker, |state| {
            state.connected = true;
            state.connecting = false;
            state.reconnect_attempts = 0;
        })
        .await;
        if let Ok(dps) = worker_fetch_status(&mut device, &mut rx, config).await {
            self.store_cacheable_dps(&config.id, device_type, &dps).await;
            self.record_dps(&config.id, device_type, &dps).await;
        }

        let mut heartbeat = tokio::time::interval(Duration::from_millis(HEARTBEAT_INTERVAL_MS));
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
                    Some(WorkerCommand::FetchStatus { reply }) => {
                        let _ = reply.send(worker_fetch_status(&mut device, &mut rx, config).await.map_err(|err| err.to_string()));
                    }
                    Some(WorkerCommand::SetValues { updates, reply }) => {
                        let result = worker_send_commands(&mut device, &mut rx, config, &updates).await;
                        let _ = reply.send(result.map_err(|err| err.to_string()));
                    }
                    None => break,
                },
            }
        }
        self.mark_failed(&config.id, worker).await;
        let _ = device.disconnect().await;
    }
}

impl ManagedTuyaDevice {
    fn reference(&self) -> DeviceRef {
        DeviceRef { id: self.config.id.clone(), name: self.config.name.clone() }
    }
}

fn reconnect_delay(attempts: i32) -> Duration {
    let exponent = attempts.max(1).saturating_sub(1) as u32;
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
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| AppError::service_unavailable(error.to_string()))?
        .as_secs();

    let payload = Payload::Struct(PayloadStruct {
        gw_id: Some(config.id.clone()),
        dev_id: config.id.clone(),
        uid: Some(config.id.clone()),
        t: Some(now.to_string()),
        dp_id: None,
        dps: Some(json!({})),
    });

    tokio::time::timeout(Duration::from_millis(STATUS_TIMEOUT_MS), device.get(payload))
        .await
        .map_err(|_| AppError::service_unavailable(format!("Status request timeout for {}", config.name)))?
        .map_err(|error| AppError::service_unavailable(error.to_string()))?;

    let received = tokio::time::timeout(Duration::from_millis(STATUS_TIMEOUT_MS), rx.recv())
        .await
        .map_err(|_| AppError::service_unavailable(format!("Status request timeout for {}", config.name)))?
        .ok_or_else(|| AppError::service_unavailable(format!("No response from {}", config.name)))?
        .map_err(|error| AppError::service_unavailable(error.to_string()))?;

    let refresh_payload = Payload::new(
        config.id.clone(),
        Some(config.id.clone()),
        Some(config.id.clone()),
        Some(u32::try_from(now).map_err(|error| AppError::service_unavailable(error.to_string()))?),
        Some(DpId::Higher),
        None,
    );
    let _ = tokio::time::timeout(Duration::from_millis(STATUS_TIMEOUT_MS), device.refresh(refresh_payload)).await;
    let maybe_refresh = tokio::time::timeout(Duration::from_millis(MESSAGE_DRAIN_TIMEOUT_MS), rx.recv())
        .await
        .ok();

    let mut merged = merge_messages_into_dps(Map::new(), received);
    if let Some(Some(Ok(messages))) = maybe_refresh {
        merged = merge_messages_into_dps(merged, messages);
    }
    while let Ok(Some(Ok(messages))) =
        tokio::time::timeout(Duration::from_millis(COMMAND_SETTLE_TIMEOUT_MS), rx.recv()).await
    {
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
        tokio::time::timeout(Duration::from_millis(STATUS_TIMEOUT_MS), device.set_values(command_payload(dps, value)))
            .await
            .map_err(|_| AppError::service_unavailable(format!("Command timeout for {}", config.name)))?
            .map_err(|error| AppError::service_unavailable(error.to_string()))?;

        let _ = tokio::time::timeout(Duration::from_millis(COMMAND_SETTLE_TIMEOUT_MS), rx.recv()).await;
    }
    Ok(())
}

/// What a device is sent to set one data point: `{"<dps>": value}`.
fn command_payload(dps: &str, value: &Value) -> Value {
    let mut payload = Map::new();
    payload.insert(dps.to_string(), value.clone());
    Value::Object(payload)
}

fn merge_messages_into_dps(mut current: Map<String, Value>, messages: Vec<Message>) -> Map<String, Value> {
    for message in messages {
        merge_payload_dps(&mut current, message.payload);
    }
    current
}

#[derive(Debug, Clone)]
pub struct TuyaConnectionStats {
    pub total: usize,
    pub connected: usize,
    pub disconnected: usize,
    pub devices: Vec<TuyaConnectionStatsEntry>,
}

#[derive(Debug, Clone)]
pub struct TuyaConnectionStatsEntry {
    pub id: String,
    pub name: String,
    pub device_type: String,
    pub connected: bool,
    pub connecting: bool,
    pub reconnect_attempts: i32,
}

/// The values worth keeping across restarts: the ones a device only reports now and then.
fn cacheable_dps_keys(device_type: TuyaDeviceType) -> &'static [&'static str] {
    match device_type {
        TuyaDeviceType::Feeder => &[dps::feeder::MEAL_PLAN],
        TuyaDeviceType::LitterBox => &[dps::litter::LITTER_LEVEL],
        TuyaDeviceType::Fountain | TuyaDeviceType::Unknown => &[],
    }
}

fn cache_entry_dps(cache: &DeviceCache, device_id: &str) -> Map<String, Value> {
    cache
        .0
        .get(device_id)
        .map(|values| values.iter().map(|(key, value)| (key.clone(), value.clone())).collect())
        .unwrap_or_default()
}

/// Puts `updates` into `entry`; true when one of them changed it.
fn merge_cache_updates(entry: &mut HashMap<String, Value>, updates: Vec<(String, Value)>) -> bool {
    let mut changed = false;
    for (key, value) in updates {
        if entry.get(&key) != Some(&value) {
            entry.insert(key, value);
            changed = true;
        }
    }
    changed
}

fn merge_payload_dps(merged: &mut Map<String, Value>, payload: Payload) {
    let value = match payload {
        Payload::Struct(payload) => payload.dps.map(|dps| json!({ "dps": dps })),
        Payload::String(raw) => serde_json::from_str::<Value>(&raw).ok(),
        Payload::ControlNewStruct(payload) => serde_json::to_value(payload)
            .ok()
            .and_then(|value| value.get("data").cloned()),
        Payload::Raw(_) => None,
    };
    if let Some(object) = value.as_ref().and_then(|value| value.get("dps")).and_then(Value::as_object) {
        for (key, value) in object {
            merged.insert(key.clone(), value.clone());
        }
    }
}

fn determine_device_type(config: &TuyaDeviceConfig) -> TuyaDeviceType {
    let product_name = config.product_name.to_lowercase();
    let category = config.category.to_lowercase();
    if product_name.contains("feeder") || category == "cwwsq" {
        TuyaDeviceType::Feeder
    } else if product_name.contains("litter") || category == "msp" {
        TuyaDeviceType::LitterBox
    } else if product_name.contains("fountain") || category == "cwysj" {
        TuyaDeviceType::Fountain
    } else {
        TuyaDeviceType::Unknown
    }
}

impl TuyaDeviceType {
    fn as_str(self) -> &'static str {
        match self {
            TuyaDeviceType::Feeder => "feeder",
            TuyaDeviceType::LitterBox => "litter-box",
            TuyaDeviceType::Fountain => "fountain",
            TuyaDeviceType::Unknown => "unknown",
        }
    }
}

fn parse_device_data(device_type: TuyaDeviceType, dps: &Map<String, Value>) -> Value {
    match device_type {
        TuyaDeviceType::Feeder => parse_feeder_status(dps),
        TuyaDeviceType::LitterBox => parse_litter_status(dps),
        TuyaDeviceType::Fountain => parse_fountain_status(dps),
        TuyaDeviceType::Unknown => Value::Object(Map::new()),
    }
}

fn parse_feeder_status(dps: &Map<String, Value>) -> Value {
    use dps::feeder::*;
    let read = |id: &str, default: Value| dps.get(id).cloned().unwrap_or(default);

    let history = dps.get(HISTORY).and_then(Value::as_str).map(|history| {
        let parts = history.split("  ").collect::<Vec<_>>();
        json!({
            "raw": history,
            "parsed": {
                "remaining": parts.first().map(|part| part.replace("R:", "")).unwrap_or_default(),
                "count": parts.get(1).map(|part| part.replace("C:", "")),
                "timestamp": parts.get(2).map(|part| part.replace("T:", "")),
                "timestamp_readable": "",
            }
        })
    });

    let feed_size = dps
        .get(FEED_SIZE)
        .and_then(Value::as_i64)
        .map(|size| format!("{size} portion{}", if size > 1 { "s" } else { "" }))
        .unwrap_or_else(|| "Unknown".to_string());

    let powered_by = match dps.get(POWER_MODE).and_then(Value::as_i64) {
        Some(0) => "AC Power".to_string(),
        Some(1) => "Battery".to_string(),
        Some(mode) => format!("Mode {mode}"),
        None => "Unknown".to_string(),
    };

    json!({
        "feeding": {
            "manual_feed_enabled": read(MANUAL_FEED_ENABLED, json!(true)),
            "last_feed_size": feed_size,
            "last_feed_report": read(FEED_REPORT, json!(0)),
            "quick_feed_available": read(QUICK_FEED, json!(false)),
        },
        "settings": {
            "sound_enabled": read(SOUND, json!(true)),
            "alexa_feed_enabled": read(ALEXA_FEED, json!(false)),
        },
        "system": {
            "fault_status": dps.get(FAULT).and_then(Value::as_i64).unwrap_or_default() != 0,
            "powered_by": powered_by,
            "ip_address": read(IP_ADDRESS, json!("Unknown")),
        },
        "history": history,
    })
}

fn parse_litter_status(dps: &Map<String, Value>) -> Value {
    use dps::litter::*;
    let read = |id: &str, default: Value| dps.get(id).cloned().unwrap_or(default);
    let number = |id: &str| dps.get(id).and_then(Value::as_i64).unwrap_or_default();
    let clean_delay = number(CLEAN_DELAY);
    let start_minutes = number(SLEEP_START);
    let end_minutes = number(SLEEP_END);

    json!({
        "clean_delay": {
            "seconds": clean_delay,
            "formatted": format_seconds(clean_delay),
        },
        "sleep_mode": {
            "enabled": read(SLEEP_ENABLED, json!(false)),
            "start_time_minutes": start_minutes,
            "start_time_formatted": format_minutes(start_minutes),
            "end_time_minutes": end_minutes,
            "end_time_formatted": format_minutes(end_minutes),
        },
        "sensors": {
            "defecation_duration": read(DEFECATION_DURATION, json!(0)),
            "defecation_frequency": read(DEFECATION_FREQUENCY, json!(0)),
            "fault_alarm": read(FAULT_ALARM, json!(0)),
            "litter_level": read(LITTER_LEVEL, json!("unknown")),
        },
        "system": {
            "state": read(STATE, json!("unknown")),
            "cleaning_in_progress": read(CLEAN, json!(false)),
            "maintenance_required": read(MAINTENANCE, json!(false)),
        },
        "settings": {
            "lighting": read(LIGHTING, json!(false)),
            "child_lock": read(CHILD_LOCK, json!(false)),
            "prompt_sound": read(PROMPT_SOUND, json!(false)),
            "kitten_mode": read(KITTEN_MODE, json!(false)),
            "automatic_homing": read(AUTOMATIC_HOMING, json!(false)),
        },
    })
}

fn parse_fountain_status(dps: &Map<String, Value>) -> Value {
    use dps::fountain::*;
    let parsed = [
        (POWER, "power"),
        (WATER_TIME, "water_time"),
        (FILTER_LIFE, "filter_life"),
        (PUMP_TIME, "pump_time"),
        (WATER_RESET, "water_reset"),
        (FILTER_RESET, "filter_reset"),
        (PUMP_RESET, "pump_reset"),
        (UV, "uv"),
        (UV_RUNTIME, "uv_runtime"),
        (WATER_LEVEL, "water_level"),
        (LOW_WATER, "low_water"),
        (ECO_MODE, "eco_mode"),
        (ECO_WATERING_STATUS, "eco_watering_status"),
        (NO_WATER, "no_water"),
        (ASSOCIATED_CAMERA, "associated_camera"),
        (MAC_ADDRESS, "mac_address"),
    ]
    .into_iter()
    .filter_map(|(id, field)| dps.get(id).map(|value| (field.to_string(), value.clone())))
    .collect();
    Value::Object(parsed)
}

fn format_seconds(seconds: i64) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn format_minutes(minutes: i64) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(id: &str, product_name: &str, category: &str) -> Value {
        json!({
            "name": format!("{product_name} {id}"),
            "id": id,
            "key": "0123456789abcdef",
            "category": category,
            "product_name": product_name,
            "ip": "not-an-ip",
            "version": "3.4",
        })
    }

    /// A manager over `devices`, its files in a fresh temp dir (returned for inspection).
    fn make(devices: Value, cache: Option<&str>) -> (TuyaManager, PathBuf) {
        let root = std::env::temp_dir().join("maison-tuya-unit").join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("devices.json"), devices.to_string()).unwrap();
        if let Some(cache) = cache {
            std::fs::write(root.join("device-cache.json"), cache).unwrap();
        }
        let manager = TuyaManager::new(&root.join("devices.json"), &root.join("device-cache.json")).unwrap();
        (manager, root)
    }

    fn map(value: Value) -> Map<String, Value> {
        value.as_object().cloned().unwrap()
    }

    #[test]
    fn device_type_comes_from_the_product_name_or_category() {
        let parse = |name: &str, category: &str| {
            determine_device_type(&serde_json::from_value(config("x", name, category)).unwrap())
        };
        assert_eq!(parse("Smart Pet Feeder", ""), TuyaDeviceType::Feeder);
        assert_eq!(parse("box", "cwwsq"), TuyaDeviceType::Feeder);
        assert_eq!(parse("Self-cleaning LITTER box", ""), TuyaDeviceType::LitterBox);
        assert_eq!(parse("box", "msp"), TuyaDeviceType::LitterBox);
        assert_eq!(parse("Pet Fountain", ""), TuyaDeviceType::Fountain);
        assert_eq!(parse("box", "cwysj"), TuyaDeviceType::Fountain);
        assert_eq!(parse("Lamp", "dj"), TuyaDeviceType::Unknown);
    }

    #[test]
    fn feeder_status_maps_its_data_points() {
        let parsed = parse_feeder_status(&map(json!({
            "101": 2, "105": 1, "14": 0, "103": false, "107": "10.0.0.2",
            "104": "R:3  C:5  T:1700000000",
        })));
        assert_eq!(parsed["feeding"]["last_feed_size"], "2 portions");
        assert_eq!(parsed["feeding"]["manual_feed_enabled"], true);
        assert_eq!(parsed["settings"]["sound_enabled"], false);
        assert_eq!(parsed["system"]["powered_by"], "Battery");
        assert_eq!(parsed["system"]["fault_status"], false);
        assert_eq!(parsed["system"]["ip_address"], "10.0.0.2");
        assert_eq!(parsed["history"]["parsed"]["remaining"], "3");
        assert_eq!(parsed["history"]["parsed"]["count"], "5");
        assert_eq!(parsed["history"]["parsed"]["timestamp"], "1700000000");

        let empty = parse_feeder_status(&Map::new());
        assert_eq!(empty["feeding"]["last_feed_size"], "Unknown");
        assert_eq!(empty["system"]["powered_by"], "Unknown");
        assert_eq!(empty["history"], Value::Null);
        assert_eq!(parse_feeder_status(&map(json!({ "101": 1, "105": 7 })))["feeding"]["last_feed_size"], "1 portion");
        assert_eq!(parse_feeder_status(&map(json!({ "105": 7 })))["system"]["powered_by"], "Mode 7");
    }

    #[test]
    fn litter_status_maps_its_data_points() {
        let parsed = parse_litter_status(&map(json!({
            "101": 125, "102": true, "103": 1290, "104": 420, "110": true, "112": "half", "109": "standby",
        })));
        assert_eq!(parsed["clean_delay"], json!({ "seconds": 125, "formatted": "2:05" }));
        assert_eq!(parsed["sleep_mode"]["enabled"], true);
        assert_eq!(parsed["sleep_mode"]["start_time_formatted"], "21:30");
        assert_eq!(parsed["sleep_mode"]["end_time_formatted"], "07:00");
        assert_eq!(parsed["settings"]["child_lock"], true);
        assert_eq!(parsed["settings"]["kitten_mode"], false);
        assert_eq!(parsed["sensors"]["litter_level"], "half");
        assert_eq!(parsed["system"]["state"], "standby");
        assert_eq!(parse_litter_status(&Map::new())["sensors"]["litter_level"], "unknown");
    }

    #[test]
    fn fountain_status_names_only_the_reported_points() {
        let parsed = parse_fountain_status(&map(json!({ "1": true, "4": 80, "102": 2, "999": "x" })));
        assert_eq!(parsed, json!({ "power": true, "filter_life": 80, "eco_mode": 2 }));
        assert_eq!(parse_device_data(TuyaDeviceType::Unknown, &map(json!({ "1": true }))), json!({}));
    }

    /// The parsers still answer what the legacy server answered for the same data points
    /// (captured from the real devices, `tests/fixtures/tuya`).
    #[test]
    fn parsers_match_the_captured_legacy_answers() {
        let fixture = |text: &str| serde_json::from_str::<Value>(text).unwrap()["body"].clone();
        for (kind, text) in [
            (TuyaDeviceType::LitterBox, include_str!("../tests/fixtures/tuya/litter-box-status.json")),
            (TuyaDeviceType::Fountain, include_str!("../tests/fixtures/tuya/fountain-status.json")),
        ] {
            let body = fixture(text);
            assert_eq!(parse_device_data(kind, &map(body["raw_dps"].clone())), body["parsed_status"], "{kind:?}");
        }
        let listed = fixture(include_str!("../tests/fixtures/tuya/devices.json"));
        for device in listed["devices"].as_array().unwrap() {
            let kind = match device["type"].as_str().unwrap() {
                "feeder" => TuyaDeviceType::Feeder,
                "litter-box" => TuyaDeviceType::LitterBox,
                _ => TuyaDeviceType::Fountain,
            };
            let parsed = parse_device_data(kind, &map(device["last_data"]["dps"].clone()));
            assert_eq!(parsed, device["parsed_data"], "{kind:?}");
        }
    }

    #[test]
    fn payloads_of_every_shape_merge_their_data_points() {
        let mut merged = map(json!({ "1": "old", "2": 2 }));
        merge_payload_dps(
            &mut merged,
            Payload::Struct(PayloadStruct {
                gw_id: None,
                dev_id: "d".into(),
                uid: None,
                t: None,
                dp_id: None,
                dps: Some(json!({ "1": "new" })),
            }),
        );
        merge_payload_dps(&mut merged, Payload::String(r#"{"dps":{"3":true}}"#.into()));
        merge_payload_dps(&mut merged, Payload::String("not json".into()));
        merge_payload_dps(&mut merged, Payload::Raw(vec![1, 2, 3]));
        assert_eq!(Value::Object(merged), json!({ "1": "new", "2": 2, "3": true }));
    }

    #[test]
    fn a_command_sets_one_data_point() {
        assert_eq!(command_payload(dps::litter::CHILD_LOCK, &json!(true)), json!({ "110": true }));
        assert_eq!(command_payload(dps::feeder::MANUAL_FEED, &json!(2)), json!({ "3": 2 }));
    }

    #[test]
    fn the_cache_changes_only_on_new_values() {
        let mut entry = HashMap::new();
        assert!(merge_cache_updates(&mut entry, vec![("1".into(), json!("plan"))]));
        assert!(!merge_cache_updates(&mut entry, vec![("1".into(), json!("plan"))]));
        assert!(merge_cache_updates(&mut entry, vec![("1".into(), json!("plan")), ("112".into(), json!("full"))]));
        assert_eq!(entry.len(), 2);
    }

    #[test]
    fn cached_keys_are_per_device_type() {
        assert_eq!(cacheable_dps_keys(TuyaDeviceType::Feeder), &["1"]);
        assert_eq!(cacheable_dps_keys(TuyaDeviceType::LitterBox), &["112"]);
        assert!(cacheable_dps_keys(TuyaDeviceType::Fountain).is_empty());
    }

    #[test]
    fn back_off_doubles_up_to_a_minute() {
        assert_eq!(reconnect_delay(0), Duration::from_secs(1));
        assert_eq!(reconnect_delay(1), Duration::from_secs(1));
        assert_eq!(reconnect_delay(3), Duration::from_secs(4));
        assert_eq!(reconnect_delay(40), Duration::from_secs(60));
    }

    #[test]
    fn format_helpers_pad_as_a_clock() {
        assert_eq!(format_seconds(65), "1:05");
        assert_eq!(format_minutes(65), "01:05");
    }

    #[tokio::test]
    async fn the_cache_file_is_written_only_when_it_changes() {
        let (manager, root) = make(json!([config("f", "Feeder", "")]), None);
        let path = root.join("device-cache.json");
        manager.apply_cache_updates("f", vec![("1".into(), json!("plan"))]).await;
        let written: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written, json!({ "f": { "1": "plan" } }));

        // The same value again: the file is left alone.
        std::fs::remove_file(&path).unwrap();
        manager.apply_cache_updates("f", vec![("1".into(), json!("plan"))]).await;
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn the_cache_seeds_the_listed_values_and_a_corrupt_one_is_reset() {
        let (manager, _) = make(json!([config("f", "Feeder", "")]), Some(r#"{"f":{"1":"BQgeAgE="}}"#));
        let listed = manager.list_devices().await;
        assert_eq!(listed[0].last_data.dps.get("1"), Some(&json!("BQgeAgE=")));
        assert_eq!(listed[0].device_type, "feeder");

        let (manager, root) = make(json!([config("f", "Feeder", "")]), Some("{torn"));
        assert!(manager.list_devices().await[0].last_data.dps.is_empty());
        assert!(root.join("device-cache.json.corrupt").exists());
    }

    #[test]
    fn a_corrupt_device_list_is_an_error_and_a_missing_one_is_empty() {
        let root = std::env::temp_dir().join("maison-tuya-unit").join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&root).unwrap();
        let cache = root.join("device-cache.json");
        let missing = TuyaManager::new(&root.join("devices.json"), &cache).unwrap();
        assert!(missing.device_ids().is_empty());
        std::fs::write(root.join("devices.json"), "[{").unwrap();
        assert!(TuyaManager::new(&root.join("devices.json"), &cache).is_err());
    }

    #[tokio::test]
    async fn disconnect_stops_a_retrying_worker() {
        let (manager, _) = make(json!([config("f", "Feeder", "")]), None);
        assert!(manager.connect_device("f").await.is_err());
        assert_eq!(manager.live_workers.load(Ordering::SeqCst), 1);

        manager.disconnect_device("f").await.unwrap();
        assert_eq!(manager.live_workers.load(Ordering::SeqCst), 0);
        // Past the first back-off: no worker came back and reinstalled itself.
        tokio::time::sleep(reconnect_delay(1) + Duration::from_millis(500)).await;
        assert_eq!(manager.live_workers.load(Ordering::SeqCst), 0);
        let stats = manager.connection_stats().await;
        assert!(!stats.devices[0].connecting);
        assert!(manager.runtime.read().await["f"].worker.is_none());
    }

    #[tokio::test]
    async fn disconnect_all_then_connect_all_keeps_one_worker_per_device() {
        let (manager, _) = make(json!([config("a", "Feeder", ""), config("b", "Pet Fountain", "")]), None);
        manager.connect_all_devices().await;
        assert_eq!(manager.live_workers.load(Ordering::SeqCst), 2);
        manager.disconnect_all_devices().await;
        assert_eq!(manager.live_workers.load(Ordering::SeqCst), 0);
        manager.connect_all_devices().await;
        manager.connect_all_devices().await;
        assert_eq!(manager.live_workers.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn concurrent_connects_claim_the_device_once() {
        let (manager, _) = make(json!([config("f", "Feeder", "")]), None);
        let (first, second, third) = tokio::join!(
            manager.connect_device("f"),
            manager.connect_device("f"),
            manager.connect_device("f"),
        );
        assert!(first.is_err() && second.is_err() && third.is_err());
        assert_eq!(manager.live_workers.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn connecting_again_wakes_the_waiting_worker() {
        let (manager, _) = make(json!([config("f", "Feeder", "")]), None);
        let _ = manager.connect_device("f").await;
        let before = manager.connection_stats().await.devices[0].reconnect_attempts;
        let _ = manager.connect_device("f").await;
        let after = manager.connection_stats().await.devices[0].reconnect_attempts;
        assert_eq!(after, before + 1);
        assert_eq!(manager.live_workers.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn commands_to_an_offline_device_fail_at_once() {
        let (manager, _) = make(json!([config("f", "Feeder", ""), config("l", "Litter", "")]), None);
        let error = manager
            .send_typed_command("f", TuyaDeviceType::Feeder, dps::feeder::MANUAL_FEED, json!(1))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("is not connected"));

        let error = manager.get_typed_status("l", TuyaDeviceType::Feeder).await.unwrap_err();
        assert_eq!(error.to_string(), "Device is not a feeder");
        let error = manager.get_status("nope").await.unwrap_err();
        assert_eq!(error.to_string(), "Device not found");
    }
}
