//! Zigbee lamps (and the Hue dimmer) on a native EZSP/EmberZNet driver: no zigbee2mqtt, no
//! MQTT. [`ZigbeeManager`] holds the lamps the API shows and keeps what is lasting of them
//! in `zigbee-lamps.json`, so they survive coordinator and backend restarts; the driver
//! (`driver.rs` and its modules) owns the dongle.
//!
//! - `driver.rs`: the task, its supervision, the event loop; `network.rs`: the link and the
//!   coordinator's network; `context.rs`: one live pipeline.
//! - `commands.rs`: what the manager asks; `callbacks.rs`: what the dongle tells;
//!   `discovery.rs`: interviews; `availability.rs`: pings and restores; `remotes.rs`: the
//!   dimmer; `touchlink.rs`: ZLL commissioning; `zcl.rs`: the frames.
//! - `device.rs`: a device's shapes; `error.rs`: the driver's errors; `config.rs`: `ZIGBEE_*`.

mod availability;
mod callbacks;
mod commands;
mod config;
mod context;
mod device;
mod discovery;
mod driver;
mod error;
mod network;
mod remotes;
mod touchlink;
mod zcl;

pub use config::ZigbeeConfig;

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex as StdMutex},
    time::Instant,
};

use serde::{Deserialize, Serialize};
use tokio::{
    sync::{watch, Mutex, RwLock},
    task::JoinHandle,
    time::Duration,
};
use tracing::warn;

use crate::{
    config::Config,
    error::AppError,
    lamps::{self, LampRecord, LampState, LampStats, LampStore, ZigbeeLamp},
    net::address_key,
    util::lock,
};
use commands::DriverCommand;
use device::{Light, ZigbeeDevice, ZigbeeDeviceInfo, ZigbeeDeviceType};
use driver::Driver;
use zcl::{cie, ZigbeeEffect};

/// A failed save is tried again after this (a change of the devices tries it at once).
const SAVE_RETRY: Duration = Duration::from_secs(30);
/// The save loop, should it end or panic, starts again after this.
const SAVE_LOOP_RESTART: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZigbeePairingStatus {
    pub active: bool,
    pub remaining_seconds: u16,
    pub permit_join_seconds: u16,
    pub message: Option<String>,
}

/// What is kept of a lamp: everything but its live state, so a brightness change or a
/// lamp going out of reach does not rewrite the file.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredZigbeeLamp {
    id: String,
    name: String,
    friendly_name: String,
    /// None for records from the zigbee2mqtt era: such a lamp cannot be driven until re-paired.
    node_id: Option<u16>,
    #[serde(flatten)]
    info: ZigbeeDeviceInfo,
}

impl LampRecord for StoredZigbeeLamp {
    fn id(&self) -> &str {
        &self.id
    }
}

#[derive(Clone)]
struct ZigbeeLampRuntime {
    stored: StoredZigbeeLamp,
    state: LampState,
    connected: bool,
    reachable: bool,
    last_seen: Option<String>,
    interview_completed: bool,
}

impl ZigbeeLampRuntime {
    /// A lamp from the file: not heard yet.
    fn kept(stored: StoredZigbeeLamp) -> Self {
        Self {
            state: lamp_state(&Light::default(), stored.info.supports_temperature),
            stored,
            connected: false,
            reachable: false,
            last_seen: None,
            interview_completed: false,
        }
    }

    /// A lamp the driver found: named after its address until renamed (its maker unknown
    /// until its Basic cluster answers).
    fn discovered(device: &ZigbeeDevice) -> Self {
        let stored = StoredZigbeeLamp {
            id: device.id.clone(),
            name: device.info.eui64.clone(),
            friendly_name: device.info.eui64.clone(),
            node_id: Some(device.node_id),
            info: device.info.clone(),
        };
        let mut lamp = Self::kept(stored);
        lamp.state = lamp_state(&device.light, device.info.supports_temperature);
        lamp.update(device);
        lamp
    }

    /// What the driver says of it now. Its on/off and brightness are only believed while it
    /// is connected (a lamp out of reach keeps the last ones heard).
    fn update(&mut self, device: &ZigbeeDevice) {
        let stored = &mut self.stored;
        stored.node_id = Some(device.node_id);
        stored.info = stored.info.updated(&device.info);
        for name in [&mut stored.name, &mut stored.friendly_name] {
            if name.trim().is_empty() {
                name.clone_from(&device.info.eui64);
            }
        }
        let mut state = lamp_state(&device.light, device.info.supports_temperature);
        if !device.connected {
            state.is_on = self.state.is_on;
            state.brightness = self.state.brightness;
        }
        self.state = state;
        self.connected = device.connected;
        self.reachable = device.reachable;
        self.interview_completed = device.info.endpoint.is_some();
        if device.last_seen.is_some() {
            self.last_seen.clone_from(&device.last_seen);
        }
    }

    fn is_remote(&self) -> bool {
        self.stored.info.device_type == ZigbeeDeviceType::Remote
    }

    /// Shown and counted: a lamp (not a remote) whose interview is done.
    fn is_visible(&self) -> bool {
        !self.is_remote() && self.interview_completed
    }
}

/// A light as the API shows it: 0–100 temperature bounds when the lamp has one.
fn lamp_state(light: &Light, supports_temperature: bool) -> LampState {
    LampState {
        is_on: light.is_on,
        brightness: light.brightness,
        temperature: light.temperature,
        temperature_min: supports_temperature.then_some(0),
        temperature_max: supports_temperature.then_some(100),
        color: Some(light.color.clone()),
    }
}

impl From<&ZigbeeLampRuntime> for ZigbeeLamp {
    fn from(lamp: &ZigbeeLampRuntime) -> Self {
        let (stored, info) = (&lamp.stored, &lamp.stored.info);
        ZigbeeLamp {
            id: stored.id.clone(),
            name: stored.name.clone(),
            address: info.eui64.clone(),
            friendly_name: stored.friendly_name.clone(),
            interview_completed: lamp.interview_completed,
            model: info.model.clone(),
            manufacturer: info
                .manufacturer
                .clone()
                .unwrap_or_else(|| "Unknown".to_string()),
            connected: lamp.connected,
            reachable: lamp.reachable,
            supports_brightness: info.supports_brightness,
            supports_temperature: info.supports_temperature,
            supports_color: info.supports_color,
            state: lamp.state.clone(),
            last_seen: lamp.last_seen.clone(),
        }
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
    store: LampStore<StoredZigbeeLamp>,
    lamps: RwLock<HashMap<String, ZigbeeLampRuntime>>,
    /// IEEE addresses (as keys: `net::address_key`) that must never appear as lamps,
    /// enforced both at load time and on every sync (a blacklisted device still on the mesh
    /// keeps announcing itself).
    blacklisted_addresses: HashSet<String>,
    pairing: RwLock<PairingRuntime>,
    driver: Driver,
    /// The driver's devices as last synced: a sync only runs when they changed.
    devices: Mutex<watch::Receiver<Vec<ZigbeeDevice>>>,
    /// The visible lamps as the API shows them, sent whenever a sync changed them.
    shown: watch::Sender<Vec<ZigbeeLamp>>,
    permit_join_seconds: u16,
    save_loop: StdMutex<Option<JoinHandle<()>>>,
}

impl ZigbeeManager {
    pub fn new(config: &Config) -> Result<Self, AppError> {
        let radio = config.zigbee.clone();
        let manager = Self::with_driver(config, |known| Driver::new(radio, known))?;
        manager.spawn_save_loop();
        Ok(manager)
    }

    /// The manager over the kept lamps, its driver made by `driver` from them; no save
    /// loop yet (tests drive the syncs themselves).
    fn with_driver(
        config: &Config,
        driver: impl FnOnce(Vec<(u16, ZigbeeDeviceInfo)>) -> Driver,
    ) -> Result<Self, AppError> {
        let store: LampStore<StoredZigbeeLamp> = LampStore::new(
            &config.zigbee_lamps_path,
            &config.zigbee_lamps_blacklist_path,
        );
        let blacklisted_addresses = store.load_blacklist()?;
        let lamps: HashMap<String, ZigbeeLampRuntime> = store
            .load_lamps()?
            .into_iter()
            .filter(|lamp| !blacklisted_addresses.contains(&address_key(&lamp.info.eui64)))
            .map(|lamp| (lamp.id.clone(), ZigbeeLampRuntime::kept(lamp)))
            .collect();

        // Records without a node_id (zigbee2mqtt leftovers) cannot be seeded into the
        // driver: said loudly rather than left silently invisible forever.
        let unseedable = lamps
            .values()
            .filter(|lamp| lamp.stored.node_id.is_none())
            .map(|lamp| lamp.stored.info.eui64.clone())
            .collect::<Vec<_>>();
        if !unseedable.is_empty() {
            warn!(addresses = ?unseedable, "zigbee lamps without a node_id cannot be driven natively; re-pair them (or remove them from zigbee-lamps.json)");
        }
        let known = lamps
            .values()
            .filter_map(|lamp| Some((lamp.stored.node_id?, lamp.stored.info.clone())))
            .collect();
        let driver = driver(known);
        let devices = Mutex::new(driver.subscribe());
        let shown = watch::Sender::new(visible(&lamps));

        Ok(Self {
            inner: Arc::new(ZigbeeManagerInner {
                store,
                lamps: RwLock::new(lamps),
                blacklisted_addresses,
                pairing: RwLock::new(PairingRuntime::default()),
                driver,
                devices,
                shown,
                permit_join_seconds: config.zigbee.permit_join_seconds,
                save_loop: StdMutex::new(None),
            }),
        })
    }

    /// The visible lamps as the API shows them, as they change (each sync that changed
    /// them, a rename): for whoever pushes them to the apps.
    pub fn subscribe(&self) -> watch::Receiver<Vec<ZigbeeLamp>> {
        self.inner.shown.subscribe()
    }

    pub async fn list_lamps(&self) -> Vec<ZigbeeLamp> {
        self.listing().await.0
    }

    /// The visible lamps by name, and their counts: one sync for both.
    pub async fn listing(&self) -> (Vec<ZigbeeLamp>, LampStats) {
        self.sync_logged("", "listing").await;
        let lamps = self.inner.lamps.read().await;
        let stats = self.stats_of(&lamps).await;
        (visible(&lamps), stats)
    }

    pub async fn get_lamp(&self, lamp_id: &str) -> Option<ZigbeeLamp> {
        self.sync_logged(lamp_id, "loading a lamp").await;
        self.inner
            .lamps
            .read()
            .await
            .get(lamp_id)
            .map(ZigbeeLamp::from)
    }

    pub async fn stats(&self) -> LampStats {
        self.sync_logged("", "stats").await;
        let lamps = self.inner.lamps.read().await;
        self.stats_of(&lamps).await
    }

    async fn stats_of(&self, lamps: &HashMap<String, ZigbeeLampRuntime>) -> LampStats {
        LampStats::of(
            lamps
                .values()
                .filter(|lamp| lamp.is_visible())
                .map(|lamp| (lamp.connected, lamp.reachable)),
            false,
            self.inner.driver.message().await,
        )
    }

    pub async fn pairing_status(&self) -> ZigbeePairingStatus {
        let message = self.inner.driver.message().await;
        let mut pairing = self.inner.pairing.write().await;
        self.status_of(&mut pairing, message)
    }

    pub async fn start_pairing(&self) -> Result<ZigbeePairingStatus, AppError> {
        let seconds = self.inner.permit_join_seconds;
        self.inner
            .driver
            .send(DriverCommand::PermitJoin { seconds })
            .await?;
        let mut pairing = self.inner.pairing.write().await;
        pairing.active = true;
        pairing.deadline = Some(Instant::now() + Duration::from_secs(u64::from(seconds)));
        pairing.message = Some("Zigbee pairing window open".to_string());
        Ok(self.status_of(&mut pairing, None))
    }

    pub async fn stop_pairing(&self) -> Result<ZigbeePairingStatus, AppError> {
        self.inner
            .driver
            .send(DriverCommand::PermitJoin { seconds: 0 })
            .await?;
        let mut pairing = self.inner.pairing.write().await;
        pairing.active = false;
        pairing.deadline = None;
        pairing.message = Some("Zigbee pairing window closed".to_string());
        Ok(self.status_of(&mut pairing, None))
    }

    /// The pairing window as it is now (closed once its time is over); its own message
    /// first, else `fallback` (the radio's).
    fn status_of(
        &self,
        pairing: &mut PairingRuntime,
        fallback: Option<String>,
    ) -> ZigbeePairingStatus {
        let remaining_seconds = remaining_seconds(pairing);
        ZigbeePairingStatus {
            active: pairing.active,
            remaining_seconds,
            permit_join_seconds: self.inner.permit_join_seconds,
            message: pairing.message.clone().or(fallback),
        }
    }

    /// A Touchlink (ZLL) scan, for factory-new ZLL lamps that ignore the permit-join.
    pub async fn touchlink_scan(&self) -> Result<(), AppError> {
        self.inner.driver.send(DriverCommand::TouchlinkScan).await
    }

    pub async fn set_power(&self, lamp_id: &str, enabled: bool) -> Result<LampState, AppError> {
        self.apply_command(
            lamp_id,
            DriverCommand::SetPower {
                lamp_id: lamp_id.to_string(),
                enabled,
            },
        )
        .await
    }

    /// `brightness` as [`lamps::brightness`] takes it.
    pub async fn set_brightness(
        &self,
        lamp_id: &str,
        brightness: u8,
    ) -> Result<LampState, AppError> {
        self.apply_command(
            lamp_id,
            DriverCommand::SetBrightness {
                lamp_id: lamp_id.to_string(),
                brightness: lamps::brightness(brightness),
            },
        )
        .await
    }

    /// `temperature` as [`lamps::temperature`] takes it.
    pub async fn set_temperature(
        &self,
        lamp_id: &str,
        temperature: u8,
    ) -> Result<LampState, AppError> {
        self.apply_command(
            lamp_id,
            DriverCommand::SetTemperature {
                lamp_id: lamp_id.to_string(),
                temperature: lamps::temperature(temperature),
            },
        )
        .await
    }

    /// A CIE 1931 point, each coordinate as [`cie`] takes it.
    pub async fn set_color(&self, lamp_id: &str, x: f32, y: f32) -> Result<LampState, AppError> {
        self.apply_command(
            lamp_id,
            DriverCommand::SetColor {
                lamp_id: lamp_id.to_string(),
                x: cie(x),
                y: cie(y),
            },
        )
        .await
    }

    /// `effect` by its API name (any case); an unknown one is the coded `invalid`, with
    /// the known ones as its detail.
    pub async fn set_effect(&self, lamp_id: &str, effect: &str) -> Result<LampState, AppError> {
        let effect = effect.parse::<ZigbeeEffect>()?;
        self.apply_command(
            lamp_id,
            DriverCommand::SetEffect {
                lamp_id: lamp_id.to_string(),
                effect,
            },
        )
        .await
    }

    /// Sends a command for a known lamp (`not_found` otherwise: an unknown id never reaches
    /// the radio), then returns the lamp's state: the driver has published the light it set
    /// before answering, so the sync after it has it.
    async fn apply_command(
        &self,
        lamp_id: &str,
        command: DriverCommand,
    ) -> Result<LampState, AppError> {
        self.sync_logged(lamp_id, "before a command").await;
        if !self.inner.lamps.read().await.contains_key(lamp_id) {
            return Err(lamps::not_found());
        }
        self.inner.driver.send(command).await?;
        // the radio command went out: a failed refresh (the save) must not make it an error
        self.sync_logged(lamp_id, "after a command").await;
        self.current_state(lamp_id).await
    }

    pub async fn rename_lamp(&self, lamp_id: &str, name: &str) -> Result<(), AppError> {
        let name = crate::util::name(name)?;
        self.sync_logged(lamp_id, "before a rename").await;
        {
            let mut lamps = self.inner.lamps.write().await;
            let lamp = lamps.get_mut(lamp_id).ok_or_else(lamps::not_found)?;
            lamp.stored.name = name.to_string();
            self.show(&lamps);
        }
        self.persist().await
    }

    /// Saves the lamps when what is kept of them changed; the snapshot is taken under the
    /// store's lock ([`LampStore::save`]).
    async fn persist(&self) -> Result<(), AppError> {
        let snapshot = async {
            let lamps = self.inner.lamps.read().await;
            (
                lamps.values().map(|lamp| lamp.stored.clone()).collect(),
                None,
            )
        };
        self.inner.store.save(snapshot).await
    }

    pub async fn shutdown(&self) {
        if let Some(handle) = lock(&self.inner.save_loop).take() {
            handle.abort();
        }
        self.inner.driver.shutdown();
    }

    async fn sync_logged(&self, lamp_id: &str, when: &str) {
        if let Err(error) = self.sync_from_runtime().await {
            warn!(lamp_id, when, %error, "failed to sync native zigbee lamps");
        }
    }

    /// Takes in what the driver says of its devices, when it said something new, and saves
    /// what is kept of them. A failed save is tried again on the next sync.
    async fn sync_from_runtime(&self) -> Result<(), AppError> {
        self.inner.driver.start();
        let mut seen = self.inner.devices.lock().await;
        if !seen.has_changed().unwrap_or(true) {
            return Ok(());
        }
        let devices = seen.borrow_and_update().clone();
        self.take_in(&devices).await;
        let saved = self.persist().await;
        if saved.is_err() {
            seen.mark_changed();
        }
        saved
    }

    async fn take_in(&self, devices: &[ZigbeeDevice]) {
        let mut lamps = self.inner.lamps.write().await;
        let mut seen = HashSet::new();
        // remotes are kept too (so they survive a coordinator reboot), only never shown;
        // a device without an endpoint is not known yet (perhaps a sleepy remote)
        let known = devices.iter().filter(|device| {
            device.info.endpoint.is_some()
                && !self
                    .inner
                    .blacklisted_addresses
                    .contains(&address_key(&device.info.eui64))
        });
        for device in known {
            lamps
                .entry(device.id.clone())
                .and_modify(|lamp| lamp.update(device))
                .or_insert_with(|| ZigbeeLampRuntime::discovered(device));
            seen.insert(device.id.as_str());
        }
        // a remote is a sleepy end device: missing from a snapshot is not being gone
        for lamp in lamps
            .values_mut()
            .filter(|lamp| !seen.contains(lamp.stored.id.as_str()) && !lamp.is_remote())
        {
            lamp.connected = false;
            lamp.reachable = false;
        }
        self.show(&lamps);
    }

    /// Sends the visible lamps to [`Self::subscribe`]rs, when they changed.
    fn show(&self, lamps: &HashMap<String, ZigbeeLampRuntime>) {
        let now = visible(lamps);
        self.inner.shown.send_if_modified(|shown| {
            let changed = *shown != now;
            if changed {
                *shown = now;
            }
            changed
        });
    }

    /// The save loop: a sync each time the driver's devices change (a failed save tried
    /// again after [`SAVE_RETRY`]); restarted should it end or panic.
    fn spawn_save_loop(&self) {
        let mut save_loop = lock(&self.inner.save_loop);
        if save_loop.is_some() {
            return;
        }
        let manager = self.clone();
        *save_loop = Some(crate::every("zigbee save", SAVE_LOOP_RESTART, move || {
            let manager = manager.clone();
            async move {
                let mut changes = manager.inner.driver.subscribe();
                loop {
                    changes.borrow_and_update();
                    match manager.sync_from_runtime().await {
                        Ok(()) => {
                            if changes.changed().await.is_err() {
                                return;
                            }
                        }
                        Err(error) => {
                            warn!(%error, "failed to persist native zigbee runtime state");
                            tokio::time::sleep(SAVE_RETRY).await;
                        }
                    }
                }
            }
        }));
    }

    async fn current_state(&self, lamp_id: &str) -> Result<LampState, AppError> {
        let lamps = self.inner.lamps.read().await;
        Ok(lamps
            .get(lamp_id)
            .ok_or_else(lamps::not_found)?
            .state
            .clone())
    }
}

/// The lamps shown (no remote, interviewed), by name.
fn visible(lamps: &HashMap<String, ZigbeeLampRuntime>) -> Vec<ZigbeeLamp> {
    let mut shown = lamps
        .values()
        .filter(|lamp| lamp.is_visible())
        .map(ZigbeeLamp::from)
        .collect::<Vec<_>>();
    shown.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
    shown
}

fn remaining_seconds(pairing: &mut PairingRuntime) -> u16 {
    let now = Instant::now();
    match pairing
        .deadline
        .filter(|deadline| pairing.active && *deadline > now)
    {
        Some(deadline) => deadline
            .saturating_duration_since(now)
            .as_secs()
            .min(u64::from(u16::MAX)) as u16,
        None => {
            pairing.active = false;
            pairing.deadline = None;
            0
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::zigbee::driver::DriverLifecycle;
    use std::path::Path;

    const ID: &str = "4b8ec60801881700";

    fn device() -> ZigbeeDevice {
        ZigbeeDevice {
            id: ID.to_string(),
            node_id: 0x2e34,
            info: ZigbeeDeviceInfo {
                eui64: "4b:8e:c6:08:01:88:17:00".to_string(),
                endpoint: Some(11),
                input_clusters: vec![0, 3, 4, 5, 6, 8],
                output_clusters: vec![25],
                model: Some("LTG002".to_string()),
                manufacturer: Some("Signify Netherlands B.V.".to_string()),
                supports_brightness: true,
                device_type: ZigbeeDeviceType::Lamp,
                ..ZigbeeDeviceInfo::default()
            },
            light: Light {
                is_on: true,
                brightness: 100,
                ..Light::default()
            },
            connected: true,
            reachable: true,
            last_seen: None,
        }
    }

    /// A manager on `root` whose driver never starts: no serial port, no save loop.
    fn manager(root: &Path) -> ZigbeeManager {
        let config = Config::defaults(root.to_path_buf());
        let manager = ZigbeeManager::with_driver(&config, |known| {
            Driver::new(ZigbeeConfig::default(), known)
        })
        .expect("native manager");
        manager.inner.driver.test_detach();
        manager
    }

    fn kept(root: &Path) -> Vec<StoredZigbeeLamp> {
        crate::store::read_json(&root.join("zigbee-lamps.json"), crate::store::Corrupt::Fail)
            .unwrap()
    }

    fn seed(manager: &ZigbeeManager, devices: Vec<ZigbeeDevice>) {
        manager.inner.driver.test_seed_devices(devices);
    }

    #[tokio::test]
    async fn a_command_reaches_the_driver_for_a_known_lamp() {
        let dir = crate::util::test_dir();
        let manager = manager(dir.path());
        seed(&manager, vec![device()]);
        manager
            .inner
            .driver
            .test_set_lifecycle(DriverLifecycle::Failed("boom"))
            .await;
        let error = manager
            .set_power(ID, false)
            .await
            .expect_err("the driver's refusal comes back");
        assert_eq!(code(&error), "device_unreachable", "{error}");
        assert!(
            !error.to_string().contains("boom"),
            "the reason is the log's"
        );
    }

    fn code(error: &AppError) -> &'static str {
        match error {
            AppError::Coded { code, .. } => code,
            other => panic!("uncoded: {other}"),
        }
    }

    #[tokio::test]
    async fn an_unknown_lamp_is_404_and_never_reaches_the_driver() {
        let dir = crate::util::test_dir();
        let manager = manager(dir.path());
        // a driver that would refuse with « boom »: a 404 proves it was never asked
        manager
            .inner
            .driver
            .test_set_lifecycle(DriverLifecycle::Failed("boom"))
            .await;
        for _ in 0..6 {
            let error = manager.set_power("nope", true).await.unwrap_err();
            assert_eq!(code(&error), "not_found");
        }
        let error = manager.set_effect(ID, "disco").await.unwrap_err();
        assert_eq!(code(&error), "invalid");
        let error = manager.set_effect("nope", "candle").await.unwrap_err();
        assert_eq!(code(&error), "not_found");
    }

    /// A blacklist kept with capitals or separators keeps the device out all the same.
    #[tokio::test]
    async fn a_blacklisted_device_on_the_mesh_never_shows() {
        let dir = crate::util::test_dir();
        crate::store::write_json(
            &dir.path().join("zigbee-lamps-blacklist.json"),
            &["4B:8E:C6:08:01:88:17:00"],
            crate::store::Access::Shared,
        )
        .unwrap();
        let manager = manager(dir.path());
        seed(&manager, vec![device()]);
        assert!(manager.list_lamps().await.is_empty());
        assert!(manager.get_lamp(ID).await.is_none());
    }

    /// Whoever pushes the lamps hears each change once, a rename included.
    #[tokio::test]
    async fn the_shown_lamps_are_announced_when_they_change() {
        let dir = crate::util::test_dir();
        let manager = manager(dir.path());
        let mut shown = manager.subscribe();
        assert!(shown.borrow_and_update().is_empty());
        seed(&manager, vec![device()]);
        manager.sync_from_runtime().await.unwrap();
        assert!(shown.has_changed().unwrap());
        assert_eq!(shown.borrow_and_update()[0].state.brightness, 100);
        manager.sync_from_runtime().await.unwrap();
        assert!(!shown.has_changed().unwrap(), "nothing new, nothing said");
        manager.rename_lamp(ID, "Bureau").await.unwrap();
        assert_eq!(shown.borrow_and_update()[0].name, "Bureau");
    }

    #[tokio::test]
    async fn the_save_loop_writes_only_what_is_kept() {
        let dir = crate::util::test_dir();
        let root = dir.path();
        let file = root.join("zigbee-lamps.json");
        let manager = manager(root);
        seed(&manager, vec![device()]);
        manager.sync_from_runtime().await.unwrap();
        assert_eq!(kept(root).len(), 1, "a new lamp is kept");

        // light, reachability, last seen: live state, never written
        std::fs::remove_file(&file).unwrap();
        let mut live = device();
        live.light = Light {
            is_on: false,
            brightness: 20,
            ..Light::default()
        };
        live.reachable = false;
        live.connected = false;
        live.last_seen = Some("2026-10-03T08:00:00+00:00".to_string());
        seed(&manager, vec![live.clone()]);
        manager.sync_from_runtime().await.unwrap();
        assert!(!file.exists(), "a runtime-only change rewrote the file");

        live.info.model = Some("LTG003".to_string());
        seed(&manager, vec![live]);
        manager.sync_from_runtime().await.unwrap();
        assert_eq!(
            kept(root)[0].info.model.as_deref(),
            Some("LTG003"),
            "a kept field is written"
        );
    }

    #[tokio::test]
    async fn the_lamps_sync_only_when_the_driver_changed_them() {
        let dir = crate::util::test_dir();
        let manager = manager(dir.path());
        seed(&manager, vec![device()]);
        manager.sync_from_runtime().await.unwrap();
        assert!(manager.inner.lamps.read().await[ID].connected);

        // the manager's copy is left alone while the driver says nothing new
        manager
            .inner
            .lamps
            .write()
            .await
            .get_mut(ID)
            .unwrap()
            .connected = false;
        manager.sync_from_runtime().await.unwrap();
        let (lamps, stats) = manager.listing().await;
        assert!(
            !lamps[0].connected,
            "an unchanged snapshot was applied again"
        );
        assert_eq!(
            stats.connected, 0,
            "the list and its counts come from one sync"
        );

        // a change is taken in at once
        let mut dimmed = device();
        dimmed.light.brightness = 40;
        seed(&manager, vec![dimmed]);
        let (lamps, stats) = manager.listing().await;
        assert_eq!(
            (
                lamps[0].state.brightness,
                lamps[0].connected,
                stats.connected
            ),
            (40, true, 1)
        );
    }

    #[tokio::test]
    async fn a_rename_is_never_overwritten_by_the_save_loop() {
        let dir = crate::util::test_dir();
        let root = dir.path();
        let manager = manager(root);
        seed(&manager, vec![device()]);
        manager.sync_from_runtime().await.unwrap();

        let syncs = (0..16)
            .map(|round| {
                let manager = manager.clone();
                let mut changed = device();
                changed.light.brightness = round;
                seed(&manager, vec![changed]);
                tokio::spawn(async move { manager.sync_from_runtime().await })
            })
            .collect::<Vec<_>>();
        manager.rename_lamp(ID, "  Salon  ").await.unwrap();
        for sync in syncs {
            sync.await.expect("sync task").expect("sync");
        }
        manager.sync_from_runtime().await.unwrap();
        assert_eq!(kept(root)[0].name, "Salon");
        assert_eq!(manager.get_lamp(ID).await.unwrap().name, "Salon");
    }

    #[tokio::test]
    async fn a_lamp_name_is_checked() {
        let dir = crate::util::test_dir();
        let manager = manager(dir.path());
        seed(&manager, vec![device()]);
        let long = "x".repeat(61);
        for bad in ["   ", "a\u{7}b", long.as_str()] {
            assert!(
                manager.rename_lamp(ID, bad).await.is_err(),
                "{bad:?} accepted"
            );
        }
        assert!(manager.rename_lamp("unknown", "Salon").await.is_err());
    }

    #[tokio::test]
    async fn remotes_and_uninterviewed_devices_are_not_shown_nor_counted() {
        let dir = crate::util::test_dir();
        let manager = manager(dir.path());
        let mut remote = device();
        remote.id = "remote".into();
        remote.info.eui64 = "00:17:88:01:08:0c:00:0b".into();
        remote.info.device_type = ZigbeeDeviceType::Remote;
        let mut uninterviewed = device();
        uninterviewed.id = "new".into();
        uninterviewed.info.eui64 = "00:17:88:01:08:0c:00:0c".into();
        uninterviewed.info.endpoint = None;
        seed(&manager, vec![device(), remote, uninterviewed]);
        let (lamps, stats) = manager.listing().await;
        assert_eq!(
            lamps
                .iter()
                .map(|lamp| lamp.id.as_str())
                .collect::<Vec<_>>(),
            vec![ID]
        );
        assert_eq!((stats.total, stats.connected, stats.reachable), (1, 1, 1));
        assert_eq!(manager.list_lamps().await.len(), 1);
    }

    #[tokio::test]
    async fn kept_lamps_seed_the_driver_and_a_blacklisted_one_is_dropped() {
        let dir = crate::util::test_dir();
        let root = dir.path();
        let lamp = |id: &str, eui64: &str, node_id| StoredZigbeeLamp {
            id: id.into(),
            name: "Salon".into(),
            friendly_name: id.into(),
            node_id,
            info: ZigbeeDeviceInfo {
                eui64: eui64.into(),
                endpoint: Some(11),
                ..device().info
            },
        };
        let lamps = vec![
            lamp("a", "aa", Some(1)),
            lamp("b", "bb", None),
            lamp("c", "cc", Some(3)),
        ];
        crate::store::write_json(
            &root.join("zigbee-lamps.json"),
            &lamps,
            crate::store::Access::Shared,
        )
        .unwrap();
        crate::store::write_json(
            &root.join("zigbee-lamps-blacklist.json"),
            &["cc"],
            crate::store::Access::Shared,
        )
        .unwrap();

        let config = Config::defaults(root.to_path_buf());
        let mut seeded = Vec::new();
        let manager = ZigbeeManager::with_driver(&config, |known| {
            seeded = known
                .iter()
                .map(|(node_id, info)| (*node_id, info.eui64.clone()))
                .collect();
            Driver::new(ZigbeeConfig::default(), known)
        })
        .unwrap();
        manager.inner.driver.test_detach();
        assert_eq!(
            seeded,
            vec![(1, "aa".to_string())],
            "no node id, no seed; blacklisted, no lamp"
        );
        assert_eq!(manager.inner.lamps.read().await.len(), 2);
    }

    #[test]
    fn a_pairing_window_counts_down_then_closes() {
        let mut pairing = PairingRuntime {
            active: true,
            deadline: Some(Instant::now() + Duration::from_secs(30)),
            message: None,
        };
        assert!((29..=30).contains(&remaining_seconds(&mut pairing)));
        pairing.deadline = Some(Instant::now());
        assert_eq!(remaining_seconds(&mut pairing), 0);
        assert!(!pairing.active && pairing.deadline.is_none());
    }
}
