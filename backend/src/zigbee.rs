//! Zigbee lamp manager backed by the native EZSP/EmberZNet driver
//! (`zigbee_native`). Persists known lamps to `zigbee-lamps.json` so they
//! survive coordinator and backend restarts.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Instant,
};

use serde::{Deserialize, Serialize};
use tokio::{
    sync::RwLock,
    task::JoinHandle,
    time::Duration,
};
use tracing::warn;

use crate::{
    config::Config,
    error::AppError,
    lamps::{LampColour, LampRecord, LampState, LampStats, LampStore},
    zigbee_native::{MIRED_COOL, MIRED_WARM, NativeKnownDevice, NativeZigbeeCommand, NativeZigbeeRuntime, ZigbeeDeviceType},
};

#[derive(Debug, Clone, Serialize)]
pub struct ZigbeeLampView {
    pub id: String,
    pub name: String,
    pub address: String,
    #[serde(rename = "friendlyName")]
    pub friendly_name: String,
    #[serde(rename = "interviewCompleted")]
    pub interview_completed: bool,
    pub model: Option<String>,
    pub manufacturer: String,
    pub connected: bool,
    pub reachable: bool,
    #[serde(rename = "supportsBrightness")]
    pub supports_brightness: bool,
    #[serde(rename = "supportsTemperature")]
    pub supports_temperature: bool,
    #[serde(rename = "supportsColor")]
    pub supports_color: bool,
    pub state: LampState,
    #[serde(rename = "lastSeen")]
    pub last_seen: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ZigbeePairingStatus {
    pub active: bool,
    #[serde(rename = "remainingSeconds")]
    pub remaining_seconds: u16,
    #[serde(rename = "permitJoinSeconds")]
    pub permit_join_seconds: u16,
    pub message: Option<String>,
}

/// What is kept of a lamp: everything but its live state, so a brightness change or a
/// lamp going out of reach does not rewrite the file.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredZigbeeLampConfig {
    id: String,
    name: String,
    friendly_name: String,
    ieee_address: String,
    node_id: Option<u16>,
    endpoint: Option<u8>,
    #[serde(default)]
    input_clusters: Vec<u16>,
    #[serde(default)]
    output_clusters: Vec<u16>,
    model: Option<String>,
    manufacturer: Option<String>,
    supports_brightness: bool,
    supports_temperature: bool,
    #[serde(default)]
    supports_color: bool,
    color_temp_min: Option<u16>,
    color_temp_max: Option<u16>,
    #[serde(default)]
    is_remote: bool,
}

impl LampRecord for StoredZigbeeLampConfig {
    fn id(&self) -> &str {
        &self.id
    }
}

#[derive(Clone)]
struct ZigbeeLampRuntime {
    config: StoredZigbeeLampConfig,
    state: LampState,
    connected: bool,
    reachable: bool,
    last_seen: Option<String>,
    interview_completed: bool,
}

impl ZigbeeLampRuntime {
    /// Shown and counted: a lamp (not a remote) whose interview is done.
    fn is_visible(&self) -> bool {
        !self.config.is_remote && self.interview_completed
    }
}

#[derive(Default)]
struct PairingRuntime {
    active: bool,
    deadline: Option<Instant>,
    message: Option<String>,
}

#[derive(Clone)]
pub struct ZigbeeManager {
    inner: Arc<ZigbeeManagerInner>,
}

struct ZigbeeManagerInner {
    store: LampStore<StoredZigbeeLampConfig>,
    lamps: RwLock<HashMap<String, ZigbeeLampRuntime>>,
    /// IEEE addresses that must never appear as lamps, enforced both at load
    /// time and on every runtime discovery sync (a blacklisted device that is
    /// still on the mesh keeps announcing itself).
    blacklisted_addresses: HashSet<String>,
    pairing: RwLock<PairingRuntime>,
    runtime: NativeZigbeeRuntime,
    permit_join_seconds: u16,
    persist_task: Mutex<Option<JoinHandle<()>>>,
}

impl ZigbeeManager {
    pub fn new(config: &Config) -> Result<Self, AppError> {
        let adapter = crate::config::env_text("ZIGBEE_ADAPTER").unwrap_or_else(|| "ember".to_string());
        let serial_port = crate::config::env_text("ZIGBEE_SERIAL_PORT");
        let manager = Self::with_runtime(config, |known_devices| {
            NativeZigbeeRuntime::spawn(adapter, serial_port, known_devices)
        })?;
        manager.spawn_persist_task();
        Ok(manager)
    }

    /// The manager over the kept lamps, its driver made by `runtime` from them; no save
    /// loop yet (tests drive the syncs themselves).
    fn with_runtime(
        config: &Config,
        runtime: impl FnOnce(Vec<NativeKnownDevice>) -> NativeZigbeeRuntime,
    ) -> Result<Self, AppError> {
        let store: LampStore<StoredZigbeeLampConfig> = LampStore::new(&config.zigbee_lamps_path, &config.zigbee_lamps_blacklist_path);
        let blacklisted_addresses = store.load_blacklist()?;
        let lamps: HashMap<String, ZigbeeLampRuntime> = store
            .load_lamps()?
            .into_iter()
            .filter(|lamp| !blacklisted_addresses.contains(&lamp.ieee_address))
            .map(|lamp| {
                let state = LampState {
                    is_on: false,
                    brightness: 0,
                    temperature: None,
                    temperature_min: lamp.color_temp_min.map(|_| 0),
                    temperature_max: lamp.color_temp_max.map(|_| 100),
                    colour: Some(LampColour::default()),
                };

                (
                    lamp.id.clone(),
                    ZigbeeLampRuntime {
                        config: lamp,
                        state,
                        connected: false,
                        reachable: false,
                        last_seen: None,
                        interview_completed: false,
                    },
                )
            })
            .collect();

        // Records without a node_id cannot be seeded into the native driver
        // (typically leftovers from the removed zigbee2mqtt era). Surface
        // them loudly instead of leaving them silently invisible forever.
        let unseedable = lamps
            .values()
            .filter(|lamp| lamp.config.node_id.is_none())
            .map(|lamp| lamp.config.ieee_address.clone())
            .collect::<Vec<_>>();
        if !unseedable.is_empty() {
            warn!(
                addresses = ?unseedable,
                "zigbee lamps without a node_id cannot be driven natively; re-pair them (or remove them from zigbee-lamps.json)"
            );
        }

        let known_devices = lamps
            .values()
            .filter_map(|lamp| {
                Some(NativeKnownDevice {
                    node_id: lamp.config.node_id?,
                    eui64: lamp.config.ieee_address.clone(),
                    endpoint: lamp.config.endpoint,
                    input_clusters: lamp.config.input_clusters.clone(),
                    output_clusters: lamp.config.output_clusters.clone(),
                    model: lamp.config.model.clone(),
                    manufacturer: lamp.config.manufacturer.clone(),
                    supports_brightness: lamp.config.supports_brightness,
                    supports_temperature: lamp.config.supports_temperature,
                    device_type: if lamp.config.is_remote {
                        ZigbeeDeviceType::Remote
                    } else {
                        ZigbeeDeviceType::Lamp
                    },
                })
            })
            .collect();

        let runtime = runtime(known_devices);

        let manager = Self {
            inner: Arc::new(ZigbeeManagerInner {
                store,
                lamps: RwLock::new(lamps),
                blacklisted_addresses,
                pairing: RwLock::new(PairingRuntime::default()),
                runtime,
                permit_join_seconds: config.zigbee_permit_join_seconds,
                persist_task: Mutex::new(None),
            }),
        };

        Ok(manager)
    }

    pub async fn list_lamps(&self) -> Vec<ZigbeeLampView> {
        if let Err(error) = self.sync_from_runtime().await {
            warn!(error = %error, "failed to sync native zigbee lamps before listing");
        }
        let lamps = self.inner.lamps.read().await;
        let mut values = lamps
            .values()
            .filter(|lamp| lamp.is_visible())
            .map(to_view)
            .collect::<Vec<_>>();
        values.sort_by(|left, right| left.name.cmp(&right.name));
        values
    }

    pub async fn get_lamp(&self, lamp_id: &str) -> Option<ZigbeeLampView> {
        if let Err(error) = self.sync_from_runtime().await {
            warn!(lamp_id, error = %error, "failed to sync native zigbee lamp before loading it");
        }
        let lamps = self.inner.lamps.read().await;
        lamps.get(lamp_id).map(to_view)
    }

    pub async fn stats(&self) -> LampStats {
        if let Err(error) = self.sync_from_runtime().await {
            warn!(error = %error, "failed to sync native zigbee stats");
        }
        let lamps = self.inner.lamps.read().await;
        let visible = || lamps.values().filter(|lamp| lamp.is_visible());
        LampStats {
            total: visible().count(),
            connected: visible().filter(|lamp| lamp.connected).count(),
            reachable: visible().filter(|lamp| lamp.reachable).count(),
            disabled: false,
            message: self.inner.runtime.message().await,
        }
    }

    pub async fn pairing_status(&self) -> ZigbeePairingStatus {
        let mut pairing = self.inner.pairing.write().await;
        let remaining_seconds = remaining_seconds(&mut pairing);
        let fallback_message = self.inner.runtime.message().await;

        ZigbeePairingStatus {
            active: pairing.active,
            remaining_seconds,
            permit_join_seconds: self.inner.permit_join_seconds,
            message: pairing
                .message
                .clone()
                .or(fallback_message),
        }
    }

    pub async fn start_pairing(&self) -> Result<ZigbeePairingStatus, AppError> {
        let seconds = self.inner.permit_join_seconds;
        self.inner
            .runtime
            .send(NativeZigbeeCommand::PermitJoin { seconds })
            .await?;

        let mut pairing = self.inner.pairing.write().await;
        pairing.active = true;
        pairing.deadline = Some(Instant::now() + Duration::from_secs(u64::from(seconds)));
        pairing.message = Some("Native Zigbee pairing window requested".to_string());
        let remaining_seconds = remaining_seconds(&mut pairing);

        Ok(ZigbeePairingStatus {
            active: pairing.active,
            remaining_seconds,
            permit_join_seconds: seconds,
            message: pairing.message.clone(),
        })
    }

    pub async fn stop_pairing(&self) -> Result<ZigbeePairingStatus, AppError> {
        self.inner
            .runtime
            .send(NativeZigbeeCommand::PermitJoin { seconds: 0 })
            .await?;

        let mut pairing = self.inner.pairing.write().await;
        pairing.active = false;
        pairing.deadline = None;
        pairing.message = Some("Native Zigbee pairing window closed".to_string());

        Ok(ZigbeePairingStatus {
            active: false,
            remaining_seconds: 0,
            permit_join_seconds: self.inner.permit_join_seconds,
            message: pairing.message.clone(),
        })
    }

    /// Initiate a Touchlink (ZLL) scan to discover and commission factory-new
    /// ZLL devices that don't respond to standard permit-join.
    pub async fn touchlink_scan(&self) -> Result<(), AppError> {
        self.inner
            .runtime
            .send(NativeZigbeeCommand::TouchlinkScan)
            .await
    }

    pub async fn set_power(&self, lamp_id: &str, enabled: bool) -> Result<LampState, AppError> {
        self.apply_command(lamp_id, NativeZigbeeCommand::SetPower {
            lamp_id: lamp_id.to_string(),
            enabled,
        })
        .await
    }

    pub async fn set_brightness(&self, lamp_id: &str, brightness: u8) -> Result<LampState, AppError> {
        self.apply_command(lamp_id, NativeZigbeeCommand::SetBrightness {
            lamp_id: lamp_id.to_string(),
            brightness,
        })
        .await
    }

    pub async fn set_temperature(&self, lamp_id: &str, temperature: u8) -> Result<LampState, AppError> {
        self.apply_command(lamp_id, NativeZigbeeCommand::SetTemperature {
            lamp_id: lamp_id.to_string(),
            temperature,
        })
        .await
    }

    pub async fn set_color(&self, lamp_id: &str, x: f32, y: f32) -> Result<LampState, AppError> {
        self.apply_command(lamp_id, NativeZigbeeCommand::SetColor {
            lamp_id: lamp_id.to_string(),
            x,
            y,
        })
        .await
    }

    pub async fn set_effect(&self, lamp_id: &str, effect: &str) -> Result<LampState, AppError> {
        self.apply_command(lamp_id, NativeZigbeeCommand::SetEffect {
            lamp_id: lamp_id.to_string(),
            effect: effect.to_string(),
        })
        .await
    }

    /// Send a state-changing command to the driver, then re-sync and return
    /// the lamp's refreshed state. The short delay gives the device time to
    /// apply the change before the state snapshot is taken.
    async fn apply_command(
        &self,
        lamp_id: &str,
        command: NativeZigbeeCommand,
    ) -> Result<LampState, AppError> {
        if let Err(error) = self.sync_from_runtime().await {
            warn!(lamp_id, error = %error, "failed to sync native zigbee state before command");
        }
        self.inner.runtime.send(command).await?;
        tokio::time::sleep(Duration::from_millis(250)).await;
        // The radio command already went out: a failed refresh (e.g. the
        // persistence write) must not turn the request into an error.
        if let Err(error) = self.sync_from_runtime().await {
            warn!(lamp_id, error = %error, "failed to sync native zigbee state after command");
        }
        self.current_state(lamp_id).await
    }

    pub async fn rename_lamp(&self, lamp_id: &str, name: &str) -> Result<(), AppError> {
        if let Err(error) = self.sync_from_runtime().await {
            warn!(lamp_id, error = %error, "failed to sync native zigbee lamp before rename");
        }
        let name = crate::people::clean_name(name)
            .ok_or_else(|| AppError::bad_request("A lamp name is 1 to 60 characters, without control characters"))?;
        {
            let mut lamps = self.inner.lamps.write().await;
            let lamp = lamps.get_mut(lamp_id).ok_or_else(lamp_not_found)?;
            lamp.config.name = name.to_string();
        }
        self.persist().await
    }

    /// Saves the lamps when what is kept of them changed. The snapshot is taken under the
    /// store's lock, so a slower save can never write an older copy over a rename.
    async fn persist(&self) -> Result<(), AppError> {
        let mut store = self.inner.store.lock().await;
        let lamps = self.inner.lamps.read().await.values().map(|lamp| lamp.config.clone()).collect();
        store.save_lamps(lamps).await?;
        Ok(())
    }

    pub async fn shutdown(&self) {
        if let Some(handle) = self.inner.persist_task.lock().expect("native persist task mutex").take() {
            handle.abort();
        }
        self.inner.runtime.shutdown().await;
    }

    async fn sync_from_runtime(&self) -> Result<(), AppError> {
        self.inner.runtime.ensure_initialized().await;
        let discovered = self.inner.runtime.snapshot_devices().await;

        let mut lamps = self.inner.lamps.write().await;
        let mut seen = HashSet::new();

        for device in discovered {
            if self.inner.blacklisted_addresses.contains(&device.eui64) {
                continue;
            }

            // Skip devices that haven't completed their interview or discovery yet (no
            // endpoint means we don't know what the device is — it could be a
            // sleepy remote still being discovered).
            // Remotes are included so they get persisted to disk and survive
            // coordinator reboots.  They are filtered out at API/display time
            // in list_lamps() and stats() via the is_remote flag.
            if device.endpoint.is_none() {
                continue;
            }

            let id = device.id.clone();
            let runtime = lamps.entry(id.clone()).or_insert_with(|| ZigbeeLampRuntime {
                config: StoredZigbeeLampConfig {
                    id: id.clone(),
                    name: device.eui64.clone(),
                    friendly_name: device.eui64.clone(),
                    ieee_address: device.eui64.clone(),
                    node_id: Some(device.node_id),
                    endpoint: device.endpoint,
                    input_clusters: device.input_clusters.clone(),
                    output_clusters: device.output_clusters.clone(),
                    model: device.model.clone(),
                    manufacturer: device.manufacturer.clone().or_else(|| Some("Native EZSP".to_string())),
                    ..Default::default()
                },
                state: LampState {
                    is_on: device.is_on,
                    brightness: device.brightness,
                    temperature: None,
                    temperature_min: None,
                    temperature_max: None,
                    colour: None,
                },
                connected: device.connected,
                reachable: device.reachable,
                last_seen: device.last_seen.clone(),
                interview_completed: device.endpoint.is_some(),
            });

            runtime.config.ieee_address = device.eui64.clone();
            runtime.config.node_id = Some(device.node_id);
            runtime.config.endpoint = device.endpoint;
            runtime.config.input_clusters = device.input_clusters.clone();
            runtime.config.output_clusters = device.output_clusters.clone();
            runtime.config.model = device.model.clone().or(runtime.config.model.clone());
            runtime.config.manufacturer = device.manufacturer.clone().or(runtime.config.manufacturer.clone());
            runtime.config.supports_brightness = device.supports_brightness;
            runtime.config.supports_temperature = device.supports_temperature;
            runtime.config.supports_color = device.supports_color;
            runtime.config.color_temp_min = device.supports_temperature.then_some(MIRED_COOL);
            runtime.config.color_temp_max = device.supports_temperature.then_some(MIRED_WARM);
            runtime.config.is_remote = device.device_type == ZigbeeDeviceType::Remote;
            runtime.connected = device.connected;
            runtime.reachable = device.reachable;
            runtime.interview_completed = device.endpoint.is_some();
            if device.last_seen.is_some() {
                runtime.last_seen = device.last_seen.clone();
            }
            if device.connected {
                runtime.state.is_on = device.is_on;
                runtime.state.brightness = device.brightness;
            }
            runtime.state.temperature = device.temperature;
            runtime.state.temperature_min = device.supports_temperature.then_some(0);
            runtime.state.temperature_max = device.supports_temperature.then_some(100);
            runtime.state.colour = Some(LampColour {
                color_x: device.color_x,
                color_y: device.color_y,
                color_mode: device.color_mode,
            });
            if runtime.config.name.trim().is_empty() {
                runtime.config.name = device.eui64.clone();
            }
            if runtime.config.friendly_name.trim().is_empty() {
                runtime.config.friendly_name = device.eui64.clone();
            }

            seen.insert(id);
        }

        for lamp in lamps.values_mut() {
            if !seen.contains(&lamp.config.id) {
                // Don't mark remotes as disconnected — they are sleepy end
                // devices and may not appear in every snapshot.
                if lamp.config.is_remote {
                    continue;
                }
                lamp.connected = false;
                lamp.reachable = false;
            }
        }

        drop(lamps);
        self.persist().await
    }

    fn spawn_persist_task(&self) {
        if self.inner.persist_task.lock().expect("native persist task mutex").is_some() {
            return;
        }

        let manager = self.clone();
        let handle = tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(2));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                tick.tick().await;
                if let Err(error) = manager.sync_from_runtime().await {
                    warn!(error = %error, "failed to persist native zigbee runtime state");
                }
            }
        });

        *self.inner.persist_task.lock().expect("native persist task mutex") = Some(handle);
    }

    async fn current_state(&self, lamp_id: &str) -> Result<LampState, AppError> {
        let lamps = self.inner.lamps.read().await;
        let lamp = lamps
            .get(lamp_id)
            .ok_or_else(lamp_not_found)?;
        Ok(lamp.state.clone())
    }
}

fn to_view(lamp: &ZigbeeLampRuntime) -> ZigbeeLampView {
    ZigbeeLampView {
        id: lamp.config.id.clone(),
        name: lamp.config.name.clone(),
        address: lamp.config.ieee_address.clone(),
        friendly_name: lamp.config.friendly_name.clone(),
        interview_completed: lamp.interview_completed,
        model: lamp.config.model.clone(),
        manufacturer: lamp
            .config
            .manufacturer
            .clone()
            .unwrap_or_else(|| "Unknown".to_string()),
        connected: lamp.connected,
        reachable: lamp.reachable,
        supports_brightness: lamp.config.supports_brightness,
        supports_temperature: lamp.config.supports_temperature,
        supports_color: lamp.config.supports_color,
        state: lamp.state.clone(),
        last_seen: lamp.last_seen.clone(),
    }
}

fn remaining_seconds(pairing: &mut PairingRuntime) -> u16 {
    if !pairing.active {
        pairing.deadline = None;
        return 0;
    }

    let Some(deadline) = pairing.deadline else {
        pairing.active = false;
        return 0;
    };

    let now = Instant::now();
    if deadline <= now {
        pairing.active = false;
        pairing.deadline = None;
        return 0;
    }

    deadline.saturating_duration_since(now).as_secs().min(u16::MAX as u64) as u16
}

fn lamp_not_found() -> AppError {
    AppError::not_found("Zigbee lamp not found")
}

#[cfg(test)]
mod tests {
    use super::{StoredZigbeeLampConfig, ZigbeeManager};
    use crate::{
        config::Config,
        zigbee_native::{DriverLifecycle, NativeDiscoveredDevice, NativeZigbeeRuntime, ZigbeeDeviceType},
    };
    use std::path::Path;

    const ID: &str = "4b8ec60801881700";

    fn device() -> NativeDiscoveredDevice {
        NativeDiscoveredDevice {
            id: ID.to_string(),
            node_id: 0x2e34,
            eui64: "4b:8e:c6:08:01:88:17:00".to_string(),
            endpoint: Some(11),
            input_clusters: vec![0, 3, 4, 5, 6, 8],
            output_clusters: vec![25],
            supports_brightness: true,
            supports_temperature: false,
            supports_color: false,
            device_type: ZigbeeDeviceType::Lamp,
            connected: true,
            reachable: true,
            is_on: true,
            brightness: 100,
            temperature: None,
            color_x: None,
            color_y: None,
            color_mode: None,
            model: Some("LTG002".to_string()),
            manufacturer: Some("Signify Netherlands B.V.".to_string()),
            last_seen: None,
        }
    }

    /// A manager on `root` whose driver never starts: no serial port, no save loop.
    async fn manager(root: &Path) -> ZigbeeManager {
        let config = Config::defaults(root.to_path_buf());
        let manager = ZigbeeManager::with_runtime(&config, |known| {
            NativeZigbeeRuntime::spawn("ember".to_string(), None, known)
        })
        .expect("native manager");
        manager.inner.runtime.test_detach().await;
        manager
    }

    fn kept(root: &Path) -> Vec<StoredZigbeeLampConfig> {
        crate::store::read_json(&root.join("zigbee-lamps.json"), crate::store::Corrupt::Fail).unwrap()
    }

    #[tokio::test]
    async fn native_set_power_reaches_runtime_send_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        crate::store::write_json(
            &root.join("zigbee-lamps.json"),
            &vec![StoredZigbeeLampConfig {
                id: ID.to_string(),
                name: "Test Lamp".to_string(),
                friendly_name: ID.to_string(),
                ieee_address: "4b:8e:c6:08:01:88:17:00".to_string(),
                node_id: Some(0x2e34),
                endpoint: Some(11),
                input_clusters: vec![0, 3, 4, 5, 6, 8],
                output_clusters: vec![25],
                model: Some("LTG002".to_string()),
                manufacturer: Some("Signify Netherlands B.V.".to_string()),
                supports_brightness: true,
                ..Default::default()
            }],
            crate::store::Access::Shared,
        )
        .expect("zigbee lamps");

        let manager = manager(root).await;
        manager.inner.runtime.test_seed_devices(vec![device()]).await;
        manager.inner.runtime.test_set_lifecycle(DriverLifecycle::Failed("boom".to_string())).await;
        manager.inner.runtime.test_set_network_state("joined").await;

        let error = manager
            .set_power(ID, false)
            .await
            .expect_err("power change should surface runtime failure");

        assert!(error.to_string().contains("boom"), "unexpected error: {error}");
    }

    #[tokio::test]
    async fn the_save_loop_writes_only_what_is_kept() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let file = root.join("zigbee-lamps.json");
        let manager = manager(root).await;
        manager.inner.runtime.test_seed_devices(vec![device()]).await;
        manager.sync_from_runtime().await.unwrap();
        assert_eq!(kept(root).len(), 1, "a new lamp is kept");

        // light, reachability, last seen: live state, never written
        std::fs::remove_file(&file).unwrap();
        let mut live = device();
        live.is_on = false;
        live.brightness = 20;
        live.reachable = false;
        live.connected = false;
        live.last_seen = Some("2026-10-03T08:00:00Z".to_string());
        manager.inner.runtime.test_seed_devices(vec![live.clone()]).await;
        manager.sync_from_runtime().await.unwrap();
        assert!(!file.exists(), "a runtime-only change rewrote the file");

        live.model = Some("LTG003".to_string());
        manager.inner.runtime.test_seed_devices(vec![live]).await;
        manager.sync_from_runtime().await.unwrap();
        assert_eq!(kept(root)[0].model.as_deref(), Some("LTG003"), "a kept field is written");
    }

    #[tokio::test]
    async fn a_rename_is_never_overwritten_by_the_save_loop() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let manager = manager(root).await;
        manager.inner.runtime.test_seed_devices(vec![device()]).await;
        manager.sync_from_runtime().await.unwrap();

        let syncs = (0..16)
            .map(|_| {
                let manager = manager.clone();
                tokio::spawn(async move { manager.sync_from_runtime().await.unwrap() })
            })
            .collect::<Vec<_>>();
        manager.rename_lamp(ID, "  Salon  ").await.unwrap();
        for sync in syncs {
            sync.await.unwrap();
        }
        manager.sync_from_runtime().await.unwrap();
        assert_eq!(kept(root)[0].name, "Salon");
        assert_eq!(manager.get_lamp(ID).await.unwrap().name, "Salon");
    }

    #[tokio::test]
    async fn a_lamp_name_is_checked() {
        let dir = tempfile::tempdir().expect("tempdir");
        let manager = manager(dir.path()).await;
        manager.inner.runtime.test_seed_devices(vec![device()]).await;
        let long = "x".repeat(61);
        for bad in ["   ", "a\u{7}b", long.as_str()] {
            assert!(manager.rename_lamp(ID, bad).await.is_err(), "{bad:?} accepted");
        }
        assert!(manager.rename_lamp("unknown", "Salon").await.is_err());
    }

    #[tokio::test]
    async fn remotes_and_uninterviewed_devices_are_not_shown_nor_counted() {
        let dir = tempfile::tempdir().expect("tempdir");
        let manager = manager(dir.path()).await;
        let mut remote = device();
        remote.id = "remote".into();
        remote.eui64 = "00:17:88:01:08:0c:00:0b".into();
        remote.device_type = ZigbeeDeviceType::Remote;
        manager.inner.runtime.test_seed_devices(vec![device(), remote]).await;
        let lamps = manager.list_lamps().await;
        assert_eq!(lamps.iter().map(|lamp| lamp.id.as_str()).collect::<Vec<_>>(), vec![ID]);
        let stats = manager.stats().await;
        assert_eq!((stats.total, stats.connected, stats.reachable), (1, 1, 1));
    }
}
