//! The cats' Tuya devices (feeder, litter box, fountain) over the local Tuya protocol.
//!
//! - `worker`: one worker per device owns its session (connect, heartbeat, commands,
//!   reconnect), supervised;
//! - `cache`: the values a device reports now and then, kept across restarts;
//! - `parse`: payloads read into data points, and data points named for the web;
//! - `feeder`: the feeder's meal-plan codec.

mod cache;
pub mod feeder;
mod parse;
mod worker;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize},
    },
};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tokio::sync::{RwLock, oneshot, watch};

use crate::{
    error::AppError,
    net,
    routes::DeviceRef,
    store::{self, Corrupt},
};
use cache::{DeviceCache, cacheable_dps_keys};
pub use parse::parse_hhmm;
use parse::parse_device_data;
use worker::{COMMAND_REPLY_TIMEOUT, Link, STATUS_TIMEOUT, WorkerCommand, WorkerHandle};

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
#[serde(rename_all = "camelCase")]
pub struct TuyaDeviceListEntry {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub device_type: TuyaDeviceType,
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

/// What a device is, as the web names it (`"feeder"`, `"litter-box"`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TuyaDeviceType {
    Feeder,
    LitterBox,
    Fountain,
    Unknown,
}

impl TuyaDeviceType {
    /// From the cloud's product name or category.
    fn of(config: &TuyaDeviceConfig) -> Self {
        let product_name = config.product_name.to_lowercase();
        let category = config.category.to_lowercase();
        if product_name.contains("feeder") || category == "cwwsq" {
            Self::Feeder
        } else if product_name.contains("litter") || category == "msp" {
            Self::LitterBox
        } else if product_name.contains("fountain") || category == "cwysj" {
            Self::Fountain
        } else {
            Self::Unknown
        }
    }

    /// The refusal when a route for this type meets another device.
    fn mismatch(self) -> &'static str {
        match self {
            Self::Feeder => "Device is not a feeder",
            Self::LitterBox => "Device is not a litter box",
            Self::Fountain => "Device is not a fountain",
            Self::Unknown => "Unsupported device type",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TuyaManager {
    devices: Arc<HashMap<String, ManagedTuyaDevice>>,
    cache: Arc<RwLock<DeviceCache>>,
    cache_path: Arc<PathBuf>,
    runtime: Arc<RwLock<HashMap<String, RuntimeDeviceState>>>,
    next_worker: Arc<AtomicU64>,
    live_workers: Arc<AtomicUsize>,
}

#[derive(Debug, Clone)]
struct ManagedTuyaDevice {
    config: TuyaDeviceConfig,
    device_type: TuyaDeviceType,
    /// The connection, as the device's worker publishes it.
    link: Arc<watch::Sender<Link>>,
}

impl ManagedTuyaDevice {
    fn reference(&self) -> DeviceRef {
        DeviceRef { id: self.config.id.clone(), name: self.config.name.clone() }
    }
}

#[derive(Debug)]
struct RuntimeDeviceState {
    last_data: Map<String, Value>,
    parsed_data: Value,
    /// The handle of the worker that owns the session, if any. It may outlive a worker
    /// that panicked: `Link::worker` says whether it is still the live one.
    worker: Option<WorkerHandle>,
}

impl TuyaManager {
    pub fn new(devices_path: &Path, cache_path: &Path) -> Result<Self, AppError> {
        let configs = store::read_json::<Vec<TuyaDeviceConfig>>(devices_path, Corrupt::Fail)?;
        let cache = store::read_json::<DeviceCache>(cache_path, Corrupt::Reset)?;

        let devices = configs
            .into_iter()
            .map(|config| {
                let device_type = TuyaDeviceType::of(&config);
                let link = Arc::new(watch::Sender::new(Link::default()));
                (config.id.clone(), ManagedTuyaDevice { config, device_type, link })
            })
            .collect::<HashMap<_, _>>();

        let runtime = devices
            .values()
            .map(|device| {
                let cached = cache.dps(&device.config.id);
                let state = RuntimeDeviceState {
                    parsed_data: parse_device_data(device.device_type, &cached),
                    last_data: cached,
                    worker: None,
                };
                (device.config.id.clone(), state)
            })
            .collect::<HashMap<_, _>>();

        Ok(Self {
            devices: Arc::new(devices),
            cache: Arc::new(RwLock::new(cache)),
            cache_path: Arc::new(cache_path.to_path_buf()),
            runtime: Arc::new(RwLock::new(runtime)),
            next_worker: Arc::new(AtomicU64::new(1)),
            live_workers: Arc::new(AtomicUsize::new(0)),
        })
    }

    pub async fn list_devices(&self) -> Vec<TuyaDeviceListEntry> {
        let runtime = self.runtime.read().await;
        self.devices
            .values()
            .filter_map(|device| {
                let state = runtime.get(&device.config.id)?;
                let link = *device.link.borrow();
                Some(TuyaDeviceListEntry {
                    id: device.config.id.clone(),
                    name: device.config.name.clone(),
                    device_type: device.device_type,
                    product_name: device.config.product_name.clone(),
                    model: device.config.model.clone(),
                    ip: device.config.ip.clone(),
                    version: device.config.version.clone(),
                    connected: link.connected,
                    connecting: link.connecting,
                    reconnect_attempts: link.reconnect_attempts,
                    last_data: TuyaStatusPayload { dps: state.last_data.clone() },
                    parsed_data: state.parsed_data.clone(),
                })
            })
            .collect()
    }

    /// A device's live data points (the cached ones filling the gaps) and their reading.
    pub async fn get_status(&self, device_id: &str) -> Result<(DeviceRef, Map<String, Value>, Value), AppError> {
        let device = self.device(device_id)?;
        let dps = self.fetch_device_dps(device).await?;
        let parsed = parse_device_data(device.device_type, &dps);
        Ok((device.reference(), dps, parsed))
    }

    pub async fn get_typed_status(
        &self,
        device_id: &str,
        expected: TuyaDeviceType,
    ) -> Result<(DeviceRef, Map<String, Value>, Value), AppError> {
        self.typed_device(device_id, expected)?;
        self.get_status(device_id).await
    }

    pub async fn send_typed_command(
        &self,
        device_id: &str,
        expected: TuyaDeviceType,
        dps: &str,
        value: Value,
    ) -> Result<DeviceRef, AppError> {
        self.send_typed_commands(device_id, expected, vec![(dps.to_string(), value)]).await
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

    /// The feeder's meal plan as it keeps it (base64), if it said it yet.
    pub async fn feeder_meal_plan(&self, device_id: &str) -> Result<(DeviceRef, Option<String>), AppError> {
        let device = self.typed_device(device_id, TuyaDeviceType::Feeder)?;
        let dps = self.fetch_device_dps(device).await?;
        let plan = dps.get(dps::feeder::MEAL_PLAN).and_then(Value::as_str).map(ToOwned::to_owned);
        Ok((device.reference(), plan))
    }

    fn device(&self, device_id: &str) -> Result<&ManagedTuyaDevice, AppError> {
        self.devices.get(device_id).ok_or_else(|| AppError::not_found("Device not found"))
    }

    fn typed_device(&self, device_id: &str, expected: TuyaDeviceType) -> Result<&ManagedTuyaDevice, AppError> {
        let device = self.device(device_id)?;
        if device.device_type != expected {
            return Err(AppError::bad_request(expected.mismatch()));
        }
        Ok(device)
    }

    /// Makes sure one worker owns the device's session and waits for it to be connected.
    /// Checking and claiming happen under one write lock: two callers never spawn two
    /// workers (two TCP sessions fighting over one device). A handle left by a worker that
    /// died (a panic) is replaced, not waited on.
    pub async fn connect_device(&self, device_id: &str) -> Result<(), AppError> {
        let device = self.device(device_id)?;

        let spawn = {
            let mut runtime = self.runtime.write().await;
            let Some(state) = runtime.get_mut(device_id) else {
                return Err(AppError::not_found("Device not found"));
            };
            let link = *device.link.borrow();
            if state.worker.as_ref().is_some_and(|handle| link.worker != Some(handle.id)) {
                state.worker = None;
            }
            match &state.worker {
                Some(_) if link.connected || link.connecting => None,
                Some(worker) => {
                    // A worker waiting out its back-off: retry now.
                    device.link.send_modify(|link| {
                        link.connecting = true;
                        link.reconnect_attempts += 1;
                    });
                    worker.wake.notify_one();
                    None
                }
                None => {
                    let (handle, parts) = self.new_worker(device);
                    device.link.send_modify(|link| {
                        link.worker = Some(handle.id);
                        link.connecting = true;
                        if !link.connected {
                            link.reconnect_attempts += 1;
                        }
                    });
                    state.worker = Some(handle);
                    Some(parts)
                }
            }
        };
        if let Some(parts) = spawn {
            self.spawn_worker(device, parts);
        }

        let mut link = device.link.subscribe();
        let connected = tokio::time::timeout(STATUS_TIMEOUT, link.wait_for(|link| link.connected || !link.connecting))
            .await
            .is_ok_and(|seen| seen.is_ok_and(|link| link.connected));
        if connected {
            Ok(())
        } else {
            Err(AppError::service_unavailable(format!("Failed to connect to {}", device.config.name)))
        }
    }

    /// Cancels the device's worker (connected, connecting or waiting to retry) and waits
    /// for it to be gone, so a following connect starts the only session.
    pub async fn disconnect_device(&self, device_id: &str) -> Result<(), AppError> {
        let device = self.device(device_id)?;
        let worker = {
            let mut runtime = self.runtime.write().await;
            device.link.send_modify(|link| *link = Link { reconnect_attempts: link.reconnect_attempts, ..Link::default() });
            runtime.get_mut(device_id).and_then(|state| state.worker.take())
        };
        if let Some(WorkerHandle { exited, stop, .. }) = worker {
            drop(stop);
            // gone, or still closing its socket after the wait: either way no longer ours
            tokio::time::timeout(STATUS_TIMEOUT, exited).await.ok();
        }
        Ok(())
    }

    /// Every device at once (each may take a connect timeout): failures are logged.
    pub async fn connect_all_devices(&self) {
        let connects = self.devices.keys().map(|device_id| async move {
            if let Err(error) = self.connect_device(device_id).await {
                tracing::warn!(%device_id, %error, "tuya device not connected");
            }
        });
        futures::future::join_all(connects).await;
    }

    pub async fn disconnect_all_devices(&self) {
        // only an unknown id fails, and these are the known ones
        let disconnects = self.devices.keys().map(|device_id| self.disconnect_device(device_id));
        futures::future::join_all(disconnects).await;
    }

    async fn fetch_device_dps(&self, device: &ManagedTuyaDevice) -> Result<Map<String, Value>, AppError> {
        let config = &device.config;
        self.connect_device(&config.id).await?;

        let (reply, answer) = oneshot::channel();
        self.send_to_worker(device, WorkerCommand::FetchStatus { reply }).await?;
        let mut dps = await_reply(answer, &config.name, "Status request").await?;

        self.record_dps(&config.id, device.device_type, &dps).await;
        self.store_cacheable_dps(&config.id, device.device_type, &dps).await;
        self.merge_cached_dps(&mut dps, device.device_type, &config.id).await;
        Ok(dps)
    }

    async fn send_device_commands(&self, device: &ManagedTuyaDevice, updates: &[(String, Value)]) -> Result<(), AppError> {
        let (reply, answer) = oneshot::channel();
        self.send_to_worker(device, WorkerCommand::SetValues { updates: updates.to_vec(), reply }).await?;
        await_reply(answer, &device.config.name, "Command").await?;

        let mut runtime = self.runtime.write().await;
        if let Some(state) = runtime.get_mut(&device.config.id) {
            for (key, value) in updates {
                state.last_data.insert(key.clone(), value.clone());
            }
            state.parsed_data = parse_device_data(device.device_type, &state.last_data);
        }
        Ok(())
    }

    /// Hands `command` to the device's worker; a device without a connected session fails
    /// at once (a worker waiting to retry reads no command).
    async fn send_to_worker(&self, device: &ManagedTuyaDevice, command: WorkerCommand) -> Result<(), AppError> {
        let name = &device.config.name;
        let sender = {
            let runtime = self.runtime.read().await;
            let link = *device.link.borrow();
            runtime
                .get(&device.config.id)
                .and_then(|state| state.worker.as_ref())
                .filter(|worker| link.connected && link.worker == Some(worker.id))
                .map(|worker| worker.commands.clone())
        };
        let sender = sender.ok_or_else(|| AppError::service_unavailable(format!("{name} is not connected")))?;
        sender.send(command).await.map_err(|_| AppError::service_unavailable(format!("{name} worker is unavailable")))
    }
}

/// A worker's reply, within `COMMAND_REPLY_TIMEOUT`.
async fn await_reply<T>(
    answer: oneshot::Receiver<Result<T, AppError>>,
    name: &str,
    what: &str,
) -> Result<T, AppError> {
    tokio::time::timeout(COMMAND_REPLY_TIMEOUT, answer)
        .await
        .map_err(|_| AppError::service_unavailable(format!("{what} timeout for {name}")))?
        .map_err(|_| net::unreachable("Tuya device", format!("{name} worker dropped the reply")))?
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::atomic::Ordering;

    use serde_json::json;

    use super::*;
    use crate::tuya::worker::reconnect_delay;

    pub(crate) fn config(id: &str, product_name: &str, category: &str) -> Value {
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
    pub(crate) fn make(devices: Value, cache: Option<&str>) -> (TuyaManager, tempfile::TempDir) {
        let dir = crate::util::test_dir();
        let root = dir.path();
        std::fs::write(root.join("devices.json"), devices.to_string()).unwrap();
        if let Some(cache) = cache {
            std::fs::write(root.join("device-cache.json"), cache).unwrap();
        }
        let manager = TuyaManager::new(&root.join("devices.json"), &root.join("device-cache.json")).unwrap();
        (manager, dir)
    }

    fn link(manager: &TuyaManager, id: &str) -> Link {
        *manager.device(id).unwrap().link.borrow()
    }

    fn live(manager: &TuyaManager) -> usize {
        manager.live_workers.load(Ordering::SeqCst)
    }

    #[test]
    fn device_type_comes_from_the_product_name_or_category() {
        let parse = |name: &str, category: &str| TuyaDeviceType::of(&serde_json::from_value(config("x", name, category)).unwrap());
        assert_eq!(parse("Smart Pet Feeder", ""), TuyaDeviceType::Feeder);
        assert_eq!(parse("box", "cwwsq"), TuyaDeviceType::Feeder);
        assert_eq!(parse("Self-cleaning LITTER box", ""), TuyaDeviceType::LitterBox);
        assert_eq!(parse("box", "msp"), TuyaDeviceType::LitterBox);
        assert_eq!(parse("Pet Fountain", ""), TuyaDeviceType::Fountain);
        assert_eq!(parse("box", "cwysj"), TuyaDeviceType::Fountain);
        assert_eq!(parse("Lamp", "dj"), TuyaDeviceType::Unknown);
    }

    /// The web reads the type as these strings.
    #[test]
    fn device_types_keep_their_json_names() {
        for (kind, name) in [
            (TuyaDeviceType::Feeder, "feeder"),
            (TuyaDeviceType::LitterBox, "litter-box"),
            (TuyaDeviceType::Fountain, "fountain"),
            (TuyaDeviceType::Unknown, "unknown"),
        ] {
            assert_eq!(serde_json::to_value(kind).unwrap(), json!(name));
            assert_eq!(serde_json::from_value::<TuyaDeviceType>(json!(name)).unwrap(), kind);
        }
    }

    #[test]
    fn a_corrupt_device_list_is_an_error_and_a_missing_one_is_empty() {
        let dir = crate::util::test_dir();
        let root = dir.path();
        let cache = root.join("device-cache.json");
        let missing = TuyaManager::new(&root.join("devices.json"), &cache).unwrap();
        assert!(missing.devices.is_empty());
        std::fs::write(root.join("devices.json"), "[{").unwrap();
        assert!(TuyaManager::new(&root.join("devices.json"), &cache).is_err());
    }

    #[tokio::test]
    async fn disconnect_stops_a_retrying_worker() {
        let (manager, _dir) = make(json!([config("f", "Feeder", "")]), None);
        assert!(manager.connect_device("f").await.is_err());
        assert_eq!(live(&manager), 1);

        manager.disconnect_device("f").await.unwrap();
        assert_eq!(live(&manager), 0);
        // Past the first back-off: no worker came back and reinstalled itself.
        tokio::time::sleep(reconnect_delay(1) + std::time::Duration::from_millis(500)).await;
        assert_eq!(live(&manager), 0);
        assert!(!link(&manager, "f").connecting);
        assert!(manager.runtime.read().await["f"].worker.is_none());
    }

    #[tokio::test]
    async fn disconnect_all_then_connect_all_keeps_one_worker_per_device() {
        let (manager, _dir) = make(json!([config("a", "Feeder", ""), config("b", "Pet Fountain", "")]), None);
        manager.connect_all_devices().await;
        assert_eq!(live(&manager), 2);
        manager.disconnect_all_devices().await;
        assert_eq!(live(&manager), 0);
        manager.connect_all_devices().await;
        manager.connect_all_devices().await;
        assert_eq!(live(&manager), 2);
    }

    #[tokio::test]
    async fn concurrent_connects_claim_the_device_once() {
        let (manager, _dir) = make(json!([config("f", "Feeder", "")]), None);
        let (first, second, third) =
            tokio::join!(manager.connect_device("f"), manager.connect_device("f"), manager.connect_device("f"));
        assert!(first.is_err() && second.is_err() && third.is_err());
        assert_eq!(live(&manager), 1);
    }

    #[tokio::test]
    async fn connecting_again_wakes_the_waiting_worker() {
        let (manager, _dir) = make(json!([config("f", "Feeder", "")]), None);
        assert!(manager.connect_device("f").await.is_err());
        let before = link(&manager, "f").reconnect_attempts;
        assert!(manager.connect_device("f").await.is_err());
        assert_eq!(link(&manager, "f").reconnect_attempts, before + 1);
        assert_eq!(live(&manager), 1);
    }

    /// Installs a worker for `f` the way `connect_device` does, connected, whose task
    /// panics: its id and its task.
    async fn panicking_worker(manager: &TuyaManager) -> (u64, tokio::task::JoinHandle<()>) {
        let device = manager.device("f").unwrap();
        let (handle, parts) = manager.new_worker(device);
        let id = handle.id;
        device.link.send_modify(|link| {
            link.worker = Some(id);
            link.connected = true;
        });
        manager.runtime.write().await.get_mut("f").unwrap().worker = Some(handle);
        let task = tokio::spawn(async move {
            let _parts = parts;
            panic!("a worker bug");
        });
        (id, task)
    }

    /// A worker that panics clears its link at once (nobody waits 12 s on it), and its
    /// supervisor starts a fresh one after the back-off.
    #[tokio::test(start_paused = true)]
    async fn a_panicked_worker_is_cleared_and_replaced() {
        let (manager, _dir) = make(json!([config("f", "Feeder", "")]), None);
        let (dead, task) = panicking_worker(&manager).await;
        let supervisor = tokio::spawn(manager.clone().supervise("f".into(), dead, task));
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;
        let cleared = link(&manager, "f");
        assert_eq!((cleared.worker, cleared.connected, cleared.connecting), (None, false, false));
        assert_eq!(live(&manager), 0);

        supervisor.await.unwrap();
        let fresh = link(&manager, "f").worker;
        assert!(fresh.is_some_and(|id| id != dead), "a new worker owns the device: {fresh:?}");
        assert_eq!(live(&manager), 1);
    }

    /// Without its supervisor (or before it acts), the next connect replaces the dead
    /// worker's handle instead of waking nobody and waiting out the timeout.
    #[tokio::test(start_paused = true)]
    async fn a_connect_after_a_panic_starts_a_new_worker() {
        let (manager, _dir) = make(json!([config("f", "Feeder", "")]), None);
        let (dead, task) = panicking_worker(&manager).await;
        assert!(task.await.unwrap_err().is_panic());
        let started = tokio::time::Instant::now();
        assert!(manager.connect_device("f").await.is_err(), "the test device has no address");
        assert!(started.elapsed() < STATUS_TIMEOUT, "it did not wait on the dead worker");
        assert!(link(&manager, "f").worker.is_some_and(|id| id != dead));
        assert_eq!(live(&manager), 1);
    }

    /// A disconnect after the panic wins: the supervisor does not reconnect the device.
    #[tokio::test(start_paused = true)]
    async fn the_supervisor_respects_a_disconnect() {
        let (manager, _dir) = make(json!([config("f", "Feeder", "")]), None);
        let (dead, task) = panicking_worker(&manager).await;
        let supervisor = tokio::spawn(manager.clone().supervise("f".into(), dead, task));
        manager.disconnect_device("f").await.unwrap();
        supervisor.await.unwrap();
        assert_eq!(link(&manager, "f").worker, None);
        assert_eq!(live(&manager), 0);
    }

    #[tokio::test]
    async fn commands_to_an_offline_device_fail_at_once() {
        let (manager, _dir) = make(json!([config("f", "Feeder", ""), config("l", "Litter", "")]), None);
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
