//! Philips Hue lamps over Bluetooth (btleplug): found by a periodic scan, connected when
//! seen, polled while connected. `ble` speaks the lamp's GATT, `store` keeps the lamps
//! and recognises them from one scan to the next.

mod ble;
mod store;

use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use btleplug::{
    api::{Central, CentralEvent, CharPropFlags, Characteristic, Manager as _, Peripheral as _, ScanFilter, ValueNotification, WriteType},
    platform::{Adapter, Manager as BleManager, Peripheral},
};
use chrono::{DateTime, Utc};
use futures::StreamExt;
use tokio::{
    sync::{Mutex, RwLock},
    task::JoinHandle,
    time::{Duration, sleep},
};
use tracing::{debug, info, warn};

use crate::{
    config::Config,
    error::AppError,
    lamps::{HueLampView, LampState, LampStats, LampStore},
    net::address_key,
};
use ble::{
    BRIGHTNESS_UUID, CONTROL_UUID, HueCharacteristics, POWER_UUID, TEMPERATURE_UUID, ble, build_control_command,
    is_hue_lamp, parse_brightness, parse_temperature, to_brightness, to_temperature, uuid_key, uuid_str_key,
};
use store::{StoredLampConfig, dedupe_stored_lamps, fallback_lamp_name, lamp_display_address, lamp_matches_scan, stable_lamp_id};

const SCAN_INTERVAL: Duration = Duration::from_secs(10);
const SCAN_DURATION: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_secs(5);
/// How soon a lost adapter event stream is subscribed to again.
const EVENTS_RETRY: Duration = Duration::from_secs(5);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const IO_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_NEW_LAMP_FAILURES: u8 = 3;

#[derive(Clone)]
pub struct HueManager {
    inner: Arc<HueManagerInner>,
}

struct HueManagerInner {
    store: LampStore<StoredLampConfig>,
    lamps: RwLock<HashMap<String, LampRuntime>>,
    blacklisted_addresses: RwLock<HashSet<String>>,
    adapter: RwLock<Option<Adapter>>,
    availability_message: RwLock<Option<String>>,
    disabled: bool,
    shutting_down: AtomicBool,
    /// One Bluetooth operation at a time (BlueZ on the Pi mishandles overlapping ones);
    /// held around the calls only, never across a wait.
    ble_lock: Mutex<()>,
    /// The scan, poll and adapter-event loops, stopped at shutdown.
    tasks: Mutex<Vec<JoinHandle<()>>>,
}

struct LampRuntime {
    config: StoredLampConfig,
    state: LampState,
    info: LampInfo,
    connected: bool,
    connecting: bool,
    reachable: bool,
    last_seen: Option<DateTime<Utc>>,
    peripheral: Option<Peripheral>,
    characteristics: HueCharacteristics,
    connection_failures: u8,
    notification_task: Option<JoinHandle<()>>,
}

impl LampRuntime {
    /// A lamp as known before any connection: its kept settings, off, full brightness.
    fn new(config: StoredLampConfig) -> Self {
        Self {
            state: LampState {
                is_on: false,
                brightness: 100,
                temperature: config.last_temperature,
                temperature_min: config.temperature_min,
                temperature_max: config.temperature_max,
                colour: None,
            },
            info: LampInfo { manufacturer: "Philips Hue".to_string(), firmware: None, model: config.model.clone() },
            config,
            connected: false,
            connecting: false,
            reachable: false,
            last_seen: None,
            peripheral: None,
            characteristics: HueCharacteristics::default(),
            connection_failures: 0,
            notification_task: None,
        }
    }

    /// The lamp no longer connected: flags down, its characteristics and notification
    /// listener gone. Says which peripheral to disconnect, if any.
    fn release(&mut self) -> Option<Peripheral> {
        self.connected = false;
        self.connecting = false;
        self.reachable = false;
        self.characteristics = HueCharacteristics::default();
        if let Some(task) = self.notification_task.take() {
            task.abort();
        }
        self.peripheral.clone()
    }

    fn view(&self) -> HueLampView {
        HueLampView {
            id: self.config.id.clone(),
            name: self.config.name.clone(),
            address: self.config.address.clone(),
            model: self.info.model.clone().or_else(|| self.config.model.clone()),
            manufacturer: self.info.manufacturer.clone(),
            firmware: self.info.firmware.clone(),
            connected: self.connected,
            connecting: self.connecting,
            reachable: self.reachable,
            state: self.state.clone(),
            last_seen: self.last_seen.map(|value| value.to_rfc3339()),
        }
    }
}

#[derive(Clone)]
struct LampInfo {
    manufacturer: String,
    firmware: Option<String>,
    model: Option<String>,
}

struct ConnectionReady {
    peripheral: Peripheral,
    characteristics: HueCharacteristics,
    info: LampInfo,
    state: LampState,
}

impl HueManager {
    pub fn new(config: &Config) -> Result<Self, AppError> {
        let store = LampStore::new(&config.hue_lamps_path, &config.hue_blacklist_path);
        let lamps = dedupe_stored_lamps(store.load_lamps()?)
            .into_iter()
            .map(|lamp| (lamp.id.clone(), LampRuntime::new(lamp)))
            .collect();
        let blacklisted_addresses = store.load_blacklist()?.iter().map(|address| address_key(address)).collect();

        let manager = Self {
            inner: Arc::new(HueManagerInner {
                store,
                lamps: RwLock::new(lamps),
                blacklisted_addresses: RwLock::new(blacklisted_addresses),
                adapter: RwLock::new(None),
                availability_message: RwLock::new(Some(
                    if config.disable_bluetooth {
                        "Bluetooth is disabled in this environment"
                    } else {
                        "Bluetooth adapter initializing"
                    }
                    .to_string(),
                )),
                disabled: config.disable_bluetooth,
                shutting_down: AtomicBool::new(false),
                ble_lock: Mutex::new(()),
                tasks: Mutex::new(Vec::new()),
            }),
        };

        if !config.disable_bluetooth {
            let init_manager = manager.clone();
            tokio::spawn(async move { init_manager.initialize_runtime().await });
        }

        Ok(manager)
    }

    pub async fn shutdown(&self) {
        self.inner.shutting_down.store(true, Ordering::SeqCst);
        for task in self.inner.tasks.lock().await.drain(..) {
            task.abort();
        }
        let lamp_ids = self.inner.lamps.read().await.keys().cloned().collect::<Vec<_>>();
        for lamp_id in lamp_ids {
            let _ = self.disconnect_lamp(&lamp_id).await;
        }
    }

    pub async fn list_lamps(&self) -> Vec<HueLampView> {
        self.inner.lamps.read().await.values().map(LampRuntime::view).collect()
    }

    pub async fn get_lamp(&self, lamp_id: &str) -> Option<HueLampView> {
        let lamp_key = self.resolve_lamp_key(lamp_id).await?;
        self.inner.lamps.read().await.get(&lamp_key).map(LampRuntime::view)
    }

    pub async fn stats(&self) -> LampStats {
        let lamps = self.inner.lamps.read().await;
        LampStats {
            total: lamps.len(),
            connected: lamps.values().filter(|lamp| lamp.connected).count(),
            reachable: lamps.values().filter(|lamp| lamp.reachable).count(),
            disabled: self.inner.disabled,
            message: self.inner.availability_message.read().await.clone(),
        }
    }

    pub async fn trigger_scan(&self) -> Result<(), AppError> {
        self.perform_scan(SCAN_DURATION).await
    }

    /// Connects a lamp the scan has seen; `false` when it could not (counted: a new lamp
    /// that never connects is blacklisted after a few tries).
    async fn connect_lamp(&self, lamp_id: &str) -> Result<bool, AppError> {
        if self.inner.disabled {
            return Ok(false);
        }

        let lamp_key = self.lamp_key(lamp_id).await?;

        let peripheral = {
            let mut lamps = self.inner.lamps.write().await;
            let lamp = lamps.get_mut(&lamp_key).ok_or_else(lamp_not_found)?;
            if lamp.connected || lamp.connecting {
                return Ok(lamp.connected);
            }
            lamp.connecting = true;
            lamp.peripheral.clone()
        };

        let Some(peripheral) = peripheral else {
            self.mark_connect_failure(&lamp_key).await?;
            return Ok(false);
        };

        match self.establish_connection(&lamp_key, peripheral).await {
            Ok(ready) => {
                self.finish_successful_connection(&lamp_key, ready).await?;
                Ok(true)
            }
            Err(error) => {
                self.mark_connect_failure(&lamp_key).await?;
                debug!(lamp_id, error = %error, "Hue lamp connection failed");
                Ok(false)
            }
        }
    }

    async fn disconnect_lamp(&self, lamp_id: &str) -> Result<(), AppError> {
        let lamp_key = self.lamp_key(lamp_id).await?;
        let peripheral = {
            let mut lamps = self.inner.lamps.write().await;
            lamps.get_mut(&lamp_key).ok_or_else(lamp_not_found)?.release()
        };
        self.disconnect_peripheral(peripheral).await;
        Ok(())
    }

    /// Drops the Bluetooth link, if it is up. Best effort: the lamp is already forgotten
    /// as connected.
    async fn disconnect_peripheral(&self, peripheral: Option<Peripheral>) {
        let Some(peripheral) = peripheral else { return };
        let _guard = self.inner.ble_lock.lock().await;
        if matches!(ble(peripheral.is_connected(), "connection check", IO_TIMEOUT).await, Ok(true)) {
            let _ = ble(peripheral.disconnect(), "disconnection", IO_TIMEOUT).await;
        }
    }

    async fn refresh_lamp_state(&self, lamp_id: &str) -> Result<Option<LampState>, AppError> {
        let lamp_key = self.lamp_key(lamp_id).await?;

        let (peripheral, characteristics) = {
            let lamps = self.inner.lamps.read().await;
            let lamp = lamps.get(&lamp_key).ok_or_else(lamp_not_found)?;
            if !lamp.connected {
                return Ok(None);
            }
            (lamp.peripheral.clone(), lamp.characteristics.clone())
        };

        let Some(peripheral) = peripheral else {
            return Ok(None);
        };

        let state = self.read_state(&peripheral, &characteristics).await?;
        if let Some(lamp) = self.inner.lamps.write().await.get_mut(&lamp_key) {
            lamp.state = state.clone();
            lamp.connected = true;
            lamp.reachable = true;
        }

        Ok(Some(state))
    }

    pub async fn set_power(&self, lamp_id: &str, enabled: bool) -> Result<LampState, AppError> {
        let (peripheral, characteristics) = self.connected_target(lamp_id).await?;
        if let Some(power) = characteristics.power.as_ref() {
            self.write_characteristic(&peripheral, power, &[u8::from(enabled)]).await?;
        } else if let Some(control) = characteristics.control.as_ref() {
            let command = build_control_command(Some(enabled), None, None);
            self.write_characteristic(&peripheral, control, &command).await?;
        } else {
            return Err(AppError::service_unavailable("Hue lamp has no power characteristic"));
        }

        self.update_state_after_write(lamp_id, |state| state.is_on = enabled).await
    }

    pub async fn set_brightness(&self, lamp_id: &str, brightness: u8) -> Result<LampState, AppError> {
        let brightness = brightness.clamp(1, 100);
        let raw = to_brightness(brightness);
        let (peripheral, characteristics) = self.connected_target(lamp_id).await?;

        if let Some(characteristic) = characteristics.brightness.as_ref() {
            self.write_characteristic(&peripheral, characteristic, &[raw]).await?;
        } else if let Some(control) = characteristics.control.as_ref() {
            let command = build_control_command(None, Some(raw), None);
            self.write_characteristic(&peripheral, control, &command).await?;
        } else {
            return Err(AppError::service_unavailable("Hue lamp has no brightness characteristic"));
        }

        self.update_state_after_write(lamp_id, |state| state.brightness = brightness).await
    }

    pub async fn set_temperature(&self, lamp_id: &str, temperature: u8) -> Result<LampState, AppError> {
        let temperature = temperature.clamp(0, 100);
        let raw = to_temperature(temperature);
        // the key, not the id it was asked by (a MAC address, a stale id): the kept
        // temperature must land on the lamp itself
        let lamp_key = self.lamp_key(lamp_id).await?;
        let (peripheral, characteristics) = self.connected_target(&lamp_key).await?;

        if let Some(characteristic) = characteristics.temperature.as_ref() {
            self.write_characteristic(&peripheral, characteristic, &[raw, 0x01]).await?;
        } else if let Some(control) = characteristics.control.as_ref() {
            let command = build_control_command(None, None, Some(raw));
            self.write_characteristic(&peripheral, control, &command).await?;
        } else {
            return Err(AppError::service_unavailable("Hue lamp does not support color temperature"));
        }

        if let Some(lamp) = self.inner.lamps.write().await.get_mut(&lamp_key) {
            lamp.config.last_temperature = Some(temperature);
        }
        self.persist_state().await?;

        self.update_state_after_write(&lamp_key, |state| state.temperature = Some(temperature)).await
    }

    /// Renames the lamp here and, when it is connected, on the lamp itself (the Hue app
    /// shows that name): a lamp that refuses the new name leaves both as they were.
    pub async fn rename_lamp(&self, lamp_id: &str, name: &str) -> Result<(), AppError> {
        let name = crate::util::name(name)?;
        let lamp_key = self.lamp_key(lamp_id).await?;

        let (peripheral, device_name) = {
            let lamps = self.inner.lamps.read().await;
            let lamp = lamps.get(&lamp_key).ok_or_else(lamp_not_found)?;
            (lamp.peripheral.clone(), lamp.characteristics.device_name.clone())
        };
        if let (Some(peripheral), Some(device_name)) = (peripheral, device_name) {
            self.write_characteristic(&peripheral, &device_name, name.as_bytes()).await?;
        }

        self.inner.lamps.write().await.get_mut(&lamp_key).ok_or_else(lamp_not_found)?.config.name = name.to_string();
        self.persist_state().await
    }

    /// Forgets the lamp and never takes it back; `false` when there was no such lamp.
    pub async fn blacklist_lamp(&self, lamp_id: &str) -> Result<bool, AppError> {
        let Some(lamp_key) = self.resolve_lamp_key(lamp_id).await else {
            return Ok(false);
        };
        let Some(mut lamp) = self.inner.lamps.write().await.remove(&lamp_key) else {
            return Ok(false);
        };
        let peripheral = lamp.release();
        self.disconnect_peripheral(peripheral).await;

        self.inner
            .blacklisted_addresses
            .write()
            .await
            .extend([address_key(&lamp.config.id), address_key(&lamp.config.address)]);
        self.persist_state().await?;
        Ok(true)
    }

    async fn initialize_runtime(&self) {
        // the stack's own error goes to the log; the shelf says only what is missing
        let adapter = match BleManager::new().await {
            Ok(manager) => match manager.adapters().await {
                Ok(adapters) => adapters.into_iter().next(),
                Err(error) => {
                    warn!(error = %error, "Failed to list Bluetooth adapters");
                    return self.unavailable("Bluetooth adapter unavailable").await;
                }
            },
            Err(error) => {
                warn!(error = %error, "Failed to initialize Bluetooth manager");
                return self.unavailable("Bluetooth unavailable").await;
            }
        };
        let Some(adapter) = adapter else {
            return self.unavailable("No Bluetooth adapter detected").await;
        };

        info!("Hue Bluetooth manager initialized");
        *self.inner.adapter.write().await = Some(adapter);
        *self.inner.availability_message.write().await = None;
        self.start_background_tasks().await;
    }

    async fn unavailable(&self, message: &str) {
        *self.inner.availability_message.write().await = Some(message.to_string());
    }

    /// The adapter's events, the periodic scan and the poll of connected lamps, each
    /// supervised: a panic is logged and the loop goes on.
    async fn start_background_tasks(&self) {
        let mut tasks = self.inner.tasks.lock().await;
        if let Some(adapter) = self.inner.adapter.read().await.clone() {
            let manager = self.clone();
            // a stream that ends (the stack restarted) is subscribed to again
            tasks.push(crate::every("hue adapter events", EVENTS_RETRY, move || {
                let (manager, adapter) = (manager.clone(), adapter.clone());
                async move { manager.run_adapter_events(adapter).await }
            }));
        }

        let manager = self.clone();
        tasks.push(crate::every("hue scan", SCAN_DURATION + SCAN_INTERVAL, move || {
            let manager = manager.clone();
            async move {
                if manager.running() {
                    if let Err(error) = manager.perform_scan(SCAN_DURATION).await {
                        debug!(error = %error, "Hue periodic scan failed");
                    }
                }
            }
        }));

        let manager = self.clone();
        tasks.push(crate::every("hue poll", POLL_INTERVAL, move || {
            let manager = manager.clone();
            async move {
                if manager.running() {
                    manager.poll_connected_lamps().await;
                }
            }
        }));
    }

    fn running(&self) -> bool {
        !self.inner.shutting_down.load(Ordering::SeqCst)
    }

    async fn perform_scan(&self, duration: Duration) -> Result<(), AppError> {
        let Some(adapter) = self.inner.adapter.read().await.clone() else {
            return Ok(());
        };

        // the lock around each call only: a command may reach a lamp while the radio listens
        {
            let _guard = self.inner.ble_lock.lock().await;
            ble(adapter.start_scan(ScanFilter::default()), "scan", IO_TIMEOUT).await?;
        }
        sleep(duration).await;
        let peripherals = {
            let _guard = self.inner.ble_lock.lock().await;
            if let Err(error) = ble(adapter.stop_scan(), "scan stop", IO_TIMEOUT).await {
                debug!(%error, "Hue scan did not stop cleanly");
            }
            ble(adapter.peripherals(), "scan", IO_TIMEOUT).await?
        };

        let mut discovered_identities = HashSet::new();
        let mut connect_queue = Vec::new();

        for peripheral in peripherals {
            let Ok(Some(properties)) = ble(peripheral.properties(), "properties", IO_TIMEOUT).await else {
                continue;
            };
            if !is_hue_lamp(&properties) {
                continue;
            }

            let stable_id = stable_lamp_id(&peripheral, &properties);
            let display_address = lamp_display_address(&peripheral, &properties);
            let blacklisted = {
                let blacklist = self.inner.blacklisted_addresses.read().await;
                blacklist.contains(&stable_id) || blacklist.contains(&address_key(&display_address))
            };
            if blacklisted {
                continue;
            }

            discovered_identities.insert(stable_id.clone());
            let name = properties
                .local_name
                .clone()
                .or(properties.advertisement_name.clone())
                .unwrap_or_else(|| fallback_lamp_name(&display_address));

            let mut lamps = self.inner.lamps.write().await;
            let matched_key = lamps
                .iter()
                .find(|(_, lamp)| lamp_matches_scan(&lamp.config, &stable_id, &display_address, &name))
                .map(|(id, _)| id.clone());
            let mut lamp = match matched_key.and_then(|key| lamps.remove(&key)) {
                Some(lamp) => lamp,
                None => LampRuntime::new(StoredLampConfig {
                    id: stable_id.clone(),
                    name,
                    address: display_address.clone(),
                    ..Default::default()
                }),
            };
            lamp.config.id = stable_id.clone();
            lamp.config.address = display_address;
            lamp.peripheral = Some(peripheral);
            lamp.reachable = true;
            lamp.last_seen = Some(Utc::now());
            if !lamp.connected && !lamp.connecting {
                connect_queue.push(stable_id.clone());
            }
            lamps.insert(stable_id, lamp);
        }

        for lamp in self.inner.lamps.write().await.values_mut() {
            if !discovered_identities.contains(&address_key(&lamp.config.id)) && !lamp.connected {
                lamp.reachable = false;
                lamp.peripheral = None;
            }
        }

        self.persist_state().await?;

        for lamp_id in connect_queue {
            let _ = self.connect_lamp(&lamp_id).await;
        }

        Ok(())
    }

    async fn poll_connected_lamps(&self) {
        let lamp_ids = self
            .inner
            .lamps
            .read()
            .await
            .iter()
            .filter_map(|(id, lamp)| lamp.connected.then_some(id.clone()))
            .collect::<Vec<_>>();

        for lamp_id in lamp_ids {
            if self.refresh_lamp_state(&lamp_id).await.is_err() {
                self.mark_runtime_disconnected(&lamp_id).await;
            }
        }
    }

    async fn run_adapter_events(&self, adapter: Adapter) {
        let mut events = match ble(adapter.events(), "event subscription", IO_TIMEOUT).await {
            Ok(stream) => stream,
            Err(error) => {
                warn!(error = %error, "Failed to subscribe to Bluetooth adapter events");
                return;
            }
        };

        while let Some(event) = events.next().await {
            if !self.running() {
                break;
            }
            self.handle_adapter_event(event).await;
        }
    }

    async fn handle_adapter_event(&self, event: CentralEvent) {
        match event {
            CentralEvent::DeviceConnected(id) => {
                let Some(lamp_key) = self.resolve_lamp_key(&id.to_string()).await else { return };
                if let Some(lamp) = self.inner.lamps.write().await.get_mut(&lamp_key) {
                    lamp.connected = true;
                    lamp.connecting = false;
                    lamp.reachable = true;
                    lamp.last_seen = Some(Utc::now());
                }
            }
            CentralEvent::DeviceDisconnected(id) => {
                self.mark_runtime_disconnected(&id.to_string()).await;
            }
            CentralEvent::DeviceUpdated(id)
            | CentralEvent::DeviceDiscovered(id)
            | CentralEvent::DeviceServicesModified(id)
            | CentralEvent::ManufacturerDataAdvertisement { id, .. }
            | CentralEvent::ServiceDataAdvertisement { id, .. }
            | CentralEvent::ServicesAdvertisement { id, .. }
            | CentralEvent::RssiUpdate { id, .. } => {
                let Some(lamp_key) = self.resolve_lamp_key(&id.to_string()).await else { return };
                if let Some(lamp) = self.inner.lamps.write().await.get_mut(&lamp_key) {
                    lamp.reachable = true;
                    lamp.last_seen = Some(Utc::now());
                }
            }
            CentralEvent::StateUpdate(_) => {}
        }
    }

    async fn establish_connection(&self, lamp_id: &str, peripheral: Peripheral) -> Result<ConnectionReady, AppError> {
        {
            let _guard = self.inner.ble_lock.lock().await;
            if !ble(peripheral.is_connected(), "connection check", IO_TIMEOUT).await? {
                ble(peripheral.connect(), "connection", CONNECT_TIMEOUT).await?;
            }
            ble(peripheral.discover_services(), "service discovery", IO_TIMEOUT).await?;
        }

        let characteristics = HueCharacteristics::from_peripheral(&peripheral);
        let info = self.read_device_info(&peripheral, &characteristics).await;
        let state = self.read_state(&peripheral, &characteristics).await?;
        self.start_notification_listener(lamp_id, &peripheral, &characteristics).await;

        Ok(ConnectionReady { peripheral, characteristics, info, state })
    }

    async fn finish_successful_connection(&self, lamp_id: &str, ready: ConnectionReady) -> Result<(), AppError> {
        {
            let mut lamps = self.inner.lamps.write().await;
            let Some(lamp) = lamps.get_mut(lamp_id) else {
                return Ok(());
            };

            lamp.connected = true;
            lamp.connecting = false;
            lamp.reachable = true;
            lamp.last_seen = Some(Utc::now());
            lamp.peripheral = Some(ready.peripheral);
            lamp.characteristics = ready.characteristics;
            lamp.info = ready.info;
            lamp.state = ready.state;
            lamp.connection_failures = 0;
            lamp.config.has_connected_once = true;
            if lamp.config.model.is_none() {
                lamp.config.model.clone_from(&lamp.info.model);
            }
            if lamp.state.temperature_min.is_some() {
                lamp.config.temperature_min = lamp.state.temperature_min;
            }
            if lamp.state.temperature_max.is_some() {
                lamp.config.temperature_max = lamp.state.temperature_max;
            }
        }
        self.persist_state().await
    }

    async fn mark_connect_failure(&self, lamp_id: &str) -> Result<(), AppError> {
        let should_blacklist = {
            let mut lamps = self.inner.lamps.write().await;
            let Some(lamp) = lamps.get_mut(lamp_id) else {
                return Ok(());
            };
            lamp.release();
            lamp.connection_failures = lamp.connection_failures.saturating_add(1);
            !lamp.config.has_connected_once && lamp.connection_failures >= MAX_NEW_LAMP_FAILURES
        };

        if should_blacklist {
            self.blacklist_lamp(lamp_id).await?;
        }
        Ok(())
    }

    /// The link dropped (an adapter event, a failed poll): the lamp waits for the next scan.
    async fn mark_runtime_disconnected(&self, lamp_id: &str) {
        let Some(lamp_key) = self.resolve_lamp_key(lamp_id).await else {
            return;
        };
        if let Some(lamp) = self.inner.lamps.write().await.get_mut(&lamp_key) {
            lamp.release();
            lamp.last_seen = Some(Utc::now());
        }
    }

    async fn connected_target(&self, lamp_id: &str) -> Result<(Peripheral, HueCharacteristics), AppError> {
        let lamp_key = self.lamp_key(lamp_id).await?;
        let lamps = self.inner.lamps.read().await;
        let lamp = lamps.get(&lamp_key).ok_or_else(lamp_not_found)?;
        if !lamp.connected {
            return Err(AppError::service_unavailable("Hue lamp is not connected"));
        }
        let peripheral = lamp.peripheral.clone().ok_or_else(|| AppError::service_unavailable("Hue lamp is not available"))?;
        Ok((peripheral, lamp.characteristics.clone()))
    }

    async fn write_characteristic(
        &self,
        peripheral: &Peripheral,
        characteristic: &Characteristic,
        payload: &[u8],
    ) -> Result<(), AppError> {
        let _guard = self.inner.ble_lock.lock().await;
        ble(peripheral.write(characteristic, payload, WriteType::WithoutResponse), "write", IO_TIMEOUT).await
    }

    async fn update_state_after_write<F>(&self, lamp_id: &str, update: F) -> Result<LampState, AppError>
    where
        F: FnOnce(&mut LampState),
    {
        let lamp_key = self.lamp_key(lamp_id).await?;
        let mut lamps = self.inner.lamps.write().await;
        let lamp = lamps.get_mut(&lamp_key).ok_or_else(lamp_not_found)?;
        update(&mut lamp.state);
        lamp.connected = true;
        lamp.reachable = true;
        Ok(lamp.state.clone())
    }

    async fn read_state(&self, peripheral: &Peripheral, characteristics: &HueCharacteristics) -> Result<LampState, AppError> {
        let _guard = self.inner.ble_lock.lock().await;
        let mut state = LampState {
            is_on: false,
            brightness: 100,
            temperature: None,
            temperature_min: Some(0),
            temperature_max: Some(100),
            colour: None,
        };

        if let Some(power) = characteristics.power.as_ref() {
            let bytes = ble(peripheral.read(power), "read", IO_TIMEOUT).await?;
            state.is_on = bytes.first().copied().unwrap_or_default() == 0x01;
        }

        if let Some(brightness) = characteristics.brightness.as_ref() {
            let bytes = ble(peripheral.read(brightness), "read", IO_TIMEOUT).await?;
            if let Some(raw) = bytes.first().copied() {
                state.brightness = parse_brightness(raw);
            }
        }

        if let Some(temperature) = characteristics.temperature.as_ref() {
            let bytes = ble(peripheral.read(temperature), "read", IO_TIMEOUT).await?;
            if let Some(raw) = bytes.first().copied() {
                state.temperature = Some(parse_temperature(raw));
            }
        } else {
            state.temperature_min = None;
            state.temperature_max = None;
        }

        Ok(state)
    }

    async fn read_device_info(&self, peripheral: &Peripheral, characteristics: &HueCharacteristics) -> LampInfo {
        let _guard = self.inner.ble_lock.lock().await;
        LampInfo {
            manufacturer: read_optional_string(peripheral, characteristics.manufacturer.as_ref())
                .await
                .unwrap_or_else(|| "Philips Hue".to_string()),
            firmware: read_optional_string(peripheral, characteristics.firmware.as_ref()).await,
            model: read_optional_string(peripheral, characteristics.model.as_ref()).await,
        }
    }

    async fn start_notification_listener(&self, lamp_id: &str, peripheral: &Peripheral, characteristics: &HueCharacteristics) {
        let subscribable = [
            &characteristics.power,
            &characteristics.brightness,
            &characteristics.temperature,
            &characteristics.control,
        ]
        .into_iter()
        .flatten()
        .filter(|c| c.properties.intersects(CharPropFlags::NOTIFY | CharPropFlags::INDICATE))
        .collect::<Vec<_>>();

        if subscribable.is_empty() {
            return;
        }

        let stream = match ble(peripheral.notifications(), "notifications", IO_TIMEOUT).await {
            Ok(stream) => stream,
            Err(error) => {
                debug!(lamp_id, error = %error, "Hue notifications unavailable");
                return;
            }
        };

        for characteristic in subscribable {
            if let Err(error) = ble(peripheral.subscribe(characteristic), "subscription", IO_TIMEOUT).await {
                debug!(lamp_id, %error, "Hue notification subscription failed");
            }
        }

        let manager = self.clone();
        let lamp_id_owned = lamp_id.to_string();
        // one lamp's listener: a panic is logged and the next poll or scan finds the lamp
        // again, rather than it staying silently deaf
        let task = crate::supervised("hue notifications", async move {
            let mut stream = stream;
            while let Some(notification) = stream.next().await {
                manager.handle_notification(&lamp_id_owned, notification).await;
            }
        });

        let Some(lamp_key) = self.resolve_lamp_key(lamp_id).await else {
            task.abort();
            return;
        };
        if let Some(lamp) = self.inner.lamps.write().await.get_mut(&lamp_key) {
            if let Some(previous) = lamp.notification_task.replace(task) {
                previous.abort();
            }
        }
    }

    async fn handle_notification(&self, lamp_id: &str, notification: ValueNotification) {
        // a scan may have re-keyed the lamp since this listener started
        let Some(lamp_key) = self.resolve_lamp_key(lamp_id).await else {
            return;
        };
        let mut refresh_full_state = false;
        {
            let mut lamps = self.inner.lamps.write().await;
            let Some(lamp) = lamps.get_mut(&lamp_key) else {
                return;
            };
            let uuid = uuid_key(notification.uuid);
            let first = notification.value.first().copied();
            if uuid == uuid_str_key(POWER_UUID) {
                lamp.state.is_on = first == Some(0x01);
            } else if uuid == uuid_str_key(BRIGHTNESS_UUID) {
                if let Some(raw) = first {
                    lamp.state.brightness = parse_brightness(raw);
                }
            } else if uuid == uuid_str_key(TEMPERATURE_UUID) {
                if let Some(raw) = first {
                    lamp.state.temperature = Some(parse_temperature(raw));
                }
            } else if uuid == uuid_str_key(CONTROL_UUID) {
                refresh_full_state = true;
            }
            lamp.last_seen = Some(Utc::now());
            lamp.connected = true;
            lamp.reachable = true;
        }

        if refresh_full_state {
            let _ = self.refresh_lamp_state(&lamp_key).await;
        }
    }

    /// Saves the names and the blacklist; a file is only rewritten when it changed (a scan
    /// every 15 s would otherwise write the SD card thousands of times a day). The snapshot
    /// is taken under the store's lock: the newest state is always the one written.
    async fn persist_state(&self) -> Result<(), AppError> {
        let mut store = self.inner.store.lock().await;
        let lamps = self.inner.lamps.read().await.values().map(|lamp| lamp.config.clone()).collect();
        let blacklist = self.inner.blacklisted_addresses.read().await.clone();
        store.save_lamps(lamps).await?;
        store.save_blacklist(&blacklist).await?;
        Ok(())
    }

    /// The key a lamp is kept under, from its key, id or address.
    async fn lamp_key(&self, lamp_id: &str) -> Result<String, AppError> {
        self.resolve_lamp_key(lamp_id).await.ok_or_else(lamp_not_found)
    }

    async fn resolve_lamp_key(&self, lamp_id: &str) -> Option<String> {
        let lamps = self.inner.lamps.read().await;
        if lamps.contains_key(lamp_id) {
            return Some(lamp_id.to_string());
        }
        let requested = address_key(lamp_id);
        lamps.iter().find_map(|(key, lamp)| {
            [key, &lamp.config.id, &lamp.config.address]
                .into_iter()
                .any(|known| address_key(known) == requested)
                .then(|| key.clone())
        })
    }
}

async fn read_optional_string(peripheral: &Peripheral, characteristic: Option<&Characteristic>) -> Option<String> {
    let value = ble(peripheral.read(characteristic?), "read", IO_TIMEOUT).await.ok()?;
    let parsed = String::from_utf8(value).ok()?;
    let trimmed = parsed.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn lamp_not_found() -> AppError {
    AppError::not_found("Hue lamp not found")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Config, store as files};
    use std::path::Path;

    /// A manager on `root` without Bluetooth, over one kept lamp.
    fn manager(root: &Path) -> HueManager {
        let lamp = StoredLampConfig {
            id: "aabbccddeeff".into(),
            name: "Hue Lamp ee:ff".into(),
            address: "aa:bb:cc:dd:ee:ff".into(),
            has_connected_once: true,
            ..Default::default()
        };
        files::write_json(&root.join("hue-lamps.json"), &vec![lamp], files::Access::Shared).unwrap();
        HueManager::new(&Config { disable_bluetooth: true, ..Config::defaults(root.to_path_buf()) }).unwrap()
    }

    fn kept(root: &Path) -> Vec<StoredLampConfig> {
        files::read_json(&root.join("hue-lamps.json"), files::Corrupt::Fail).unwrap()
    }

    #[tokio::test]
    async fn a_lamp_is_renamed_by_its_address_and_the_name_kept() {
        let dir = tempfile::tempdir().unwrap();
        let manager = manager(dir.path());
        manager.rename_lamp("AA:BB:CC:DD:EE:FF", "  Bureau ").await.unwrap();
        assert_eq!(kept(dir.path())[0].name, "Bureau");
        assert_eq!(manager.get_lamp("aabbccddeeff").await.unwrap().name, "Bureau");

        let long = "x".repeat(61);
        for bad in ["  ", "a\nb", long.as_str()] {
            assert!(manager.rename_lamp("aabbccddeeff", bad).await.is_err(), "{bad:?} accepted");
        }
        assert_eq!(kept(dir.path())[0].name, "Bureau");
        assert!(manager.rename_lamp("nope", "Salon").await.is_err());
    }

    #[tokio::test]
    async fn a_blacklisted_lamp_is_gone_for_good() {
        let dir = tempfile::tempdir().unwrap();
        let manager = manager(dir.path());
        assert!(manager.blacklist_lamp("aa:bb:cc:dd:ee:ff").await.unwrap());
        assert!(manager.list_lamps().await.is_empty());
        assert!(kept(dir.path()).is_empty());
        let blacklist: Vec<String> =
            files::read_json(&dir.path().join("hue-lamps-blacklist.json"), files::Corrupt::Fail).unwrap();
        assert_eq!(blacklist, vec!["aabbccddeeff".to_string()]);
        assert!(!manager.blacklist_lamp("aa:bb:cc:dd:ee:ff").await.unwrap(), "already gone");
    }

    /// Blacklists written with separators still keep those lamps out.
    #[tokio::test]
    async fn a_kept_blacklist_is_read_as_keys() {
        let dir = tempfile::tempdir().unwrap();
        files::write_json(&dir.path().join("hue-lamps-blacklist.json"), &vec!["AA:BB:CC:DD:EE:FF"], files::Access::Shared)
            .unwrap();
        let manager = manager(dir.path());
        assert!(manager.inner.blacklisted_addresses.read().await.contains("aabbccddeeff"));
    }

    #[tokio::test]
    async fn a_torn_names_file_stops_the_start_instead_of_forgetting_them() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("hue-lamps.json"), "[{\"id\":").unwrap();
        let config = Config { disable_bluetooth: true, ..Config::defaults(dir.path().to_path_buf()) };
        assert!(HueManager::new(&config).is_err());
    }

    /// A kept lamp starts off, at its kept temperature, until it connects.
    #[test]
    fn a_kept_lamp_starts_from_its_settings() {
        let lamp = LampRuntime::new(StoredLampConfig {
            id: "a".into(),
            model: Some("LWA001".into()),
            temperature_min: Some(10),
            last_temperature: Some(40),
            ..Default::default()
        });
        assert!(!lamp.state.is_on && !lamp.connected && !lamp.reachable);
        assert_eq!((lamp.state.brightness, lamp.state.temperature, lamp.state.temperature_min), (100, Some(40), Some(10)));
        assert_eq!(lamp.view().model.as_deref(), Some("LWA001"));
    }

    /// Releasing a lamp drops every trace of the connection, its listener included.
    #[tokio::test]
    async fn a_released_lamp_is_disconnected_and_its_listener_stopped() {
        let mut lamp = LampRuntime::new(StoredLampConfig::default());
        lamp.connected = true;
        lamp.connecting = true;
        lamp.reachable = true;
        let listener = tokio::spawn(std::future::pending::<()>());
        let watch = listener.abort_handle();
        lamp.notification_task = Some(listener);
        assert!(lamp.release().is_none());
        assert!(!lamp.connected && !lamp.connecting && !lamp.reachable);
        tokio::task::yield_now().await;
        assert!(watch.is_finished(), "the listener is aborted");
    }

    #[tokio::test]
    async fn a_failed_connection_counts_and_a_new_lamp_that_never_connects_goes() {
        let dir = tempfile::tempdir().unwrap();
        let manager = manager(dir.path());
        manager
            .inner
            .lamps
            .write()
            .await
            .insert("newlamp".into(), LampRuntime::new(StoredLampConfig { id: "newlamp".into(), ..Default::default() }));
        for _ in 0..MAX_NEW_LAMP_FAILURES {
            manager.mark_connect_failure("newlamp").await.unwrap();
        }
        assert!(manager.get_lamp("newlamp").await.is_none(), "blacklisted");
        for _ in 0..MAX_NEW_LAMP_FAILURES {
            manager.mark_connect_failure("aabbccddeeff").await.unwrap();
        }
        assert!(manager.get_lamp("aabbccddeeff").await.is_some(), "a lamp that once connected stays");
    }
}
