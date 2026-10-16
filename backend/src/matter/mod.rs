//! Matter window coverings (Sonoff Orb-RBS roller-shutter switch).
//!
//! Maison is its own Matter commissioner: it holds a fabric, brings a device
//! onto it from a pairing code, and drives it over CASE sessions — all local,
//! no cloud account. The Pi build has no Bluetooth, so devices are commissioned
//! **over IP**: the switch must already be on the Wi-Fi (paired once with the
//! eWeLink app or another Matter ecosystem) and have a commissioning window
//! open; the code shown by that app's "share / add to another ecosystem"
//! screen is what [`MatterManager::commission`] takes.
//!
//! State lives in its own directory (default `matter/`) because the
//! controller persists its fabric keys by temp-file + rename: the directory
//! itself must belong to the service user, unlike the root-owned app dir.
//!
//! Position semantics follow the cluster, inverted for the UI: Matter counts
//! *closure* in hundredths of a percent (0 = fully open, 10000 = closed),
//! while every view here reports `openPercent` (100 = fully open).

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use axum::http::StatusCode;
use matter_codec::Tag;
use matter_controller::{
    AttestationTrust, CommandPath, FabricConfig, FileStore, MatterController, MatterTime, ReadPath,
    Value,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};

use crate::{
    config::Config,
    error::AppError,
    store::{self, Access, Corrupt},
    sun::{self, Place},
};

mod schedule;

pub use schedule::SunSchedule;
use schedule::{clock_trusted, next_sun, offset, Look, NextSun, ScheduleState, MAX_SUN_OFFSET_MIN};

type Time = chrono::DateTime<chrono::Utc>;

const WINDOW_COVERING: u32 = 0x0102;
const CMD_UP_OR_OPEN: u32 = 0x00;
const CMD_DOWN_OR_CLOSE: u32 = 0x01;
const CMD_STOP_MOTION: u32 = 0x02;
const CMD_GO_TO_LIFT_PERCENTAGE: u32 = 0x05;
const ATTR_OPERATIONAL_STATUS: u32 = 0x000A;
const ATTR_TARGET_LIFT_PERCENT100THS: u32 = 0x000B;
const ATTR_CURRENT_LIFT_PERCENT100THS: u32 = 0x000E;
const ATTR_CLUSTER_REVISION: u32 = 0xFFFD;

const OPERATIONAL_CREDENTIALS: u32 = 0x003E;
const ATTR_CURRENT_FABRIC_INDEX: u32 = 0x0005;
const CMD_REMOVE_FABRIC: u32 = 0x0A;

/// PASE runs PBKDF2 with the device's iteration count and the device then
/// joins the operational network, which takes a while on a Pi 1; stays under
/// the router's global 180 s timeout.
const COMMISSION_TIMEOUT: Duration = Duration::from_secs(150);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);
/// Status reads fan out per device on every dashboard poll; an unreachable
/// switch must not hold the list hostage.
const READ_TIMEOUT: Duration = Duration::from_secs(6);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoverConfig {
    node_id: u64,
    name: String,
    endpoint: u16,
    vendor_id: Option<u16>,
    product_id: Option<u16>,
    #[serde(default)]
    schedule: SunSchedule,
    /// « Not tonight »: the one opening / closing skipped, by its scheduled time (only that
    /// occurrence: one that passed while the Pi was down or unsure of the time skips
    /// nothing else, and goes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    skip_open_at: Option<Time>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    skip_close_at: Option<Time>,
}

impl CoverConfig {
    /// The skip of a schedule event (`Open` at sunrise, `Close` at sunset).
    fn skip_slot(&mut self, event: CoverCommand) -> Option<&mut Option<Time>> {
        match event {
            CoverCommand::Open => Some(&mut self.skip_open_at),
            CoverCommand::Close => Some(&mut self.skip_close_at),
            CoverCommand::Stop | CoverCommand::OpenPercent(_) => None,
        }
    }

    /// Whether the schedule's `event` at `at` is the occurrence skipped.
    fn skips(&self, event: CoverCommand, at: Time) -> bool {
        let skipped = match event {
            CoverCommand::Open => self.skip_open_at,
            CoverCommand::Close => self.skip_close_at,
            CoverCommand::Stop | CoverCommand::OpenPercent(_) => None,
        };
        skipped.is_some_and(|s| schedule::same_event(s, at))
    }

    /// Whether a skip's occurrence is over at `now`.
    fn past_skip(&self, now: Time) -> bool {
        [self.skip_open_at, self.skip_close_at]
            .into_iter()
            .flatten()
            .any(|s| schedule::skip_over(s, now))
    }

    /// Forgets the skips whose occurrence is over.
    fn expire_skips(&mut self, now: Time) {
        for slot in [&mut self.skip_open_at, &mut self.skip_close_at] {
            if slot.is_some_and(|s| schedule::skip_over(s, now)) {
                *slot = None;
            }
        }
    }

    /// When the schedule will next open and close it (none for an event switched off).
    fn next_times(&self, next: NextSun) -> (Option<Time>, Option<Time>) {
        (
            next.sunrise
                .filter(|_| self.schedule.open_at_sunrise)
                .map(|at| at + offset(self.schedule.sunrise_offset_min)),
            next.sunset
                .filter(|_| self.schedule.close_at_sunset)
                .map(|at| at + offset(self.schedule.sunset_offset_min)),
        )
    }
}

pub use maison_api::matter::{Cover as CoverView, CoverMotion};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CoverCommand {
    Open,
    Close,
    Stop,
    /// 0 = closed, 100 = fully open.
    OpenPercent(u8),
}

#[derive(Clone)]
pub struct MatterManager {
    inner: Arc<Inner>,
}

struct Inner {
    state_dir: PathBuf,
    trust_dir: PathBuf,
    test_roots: bool,
    /// Built on first use: binding the socket and mDNS at startup would make a
    /// network hiccup at boot fatal for a feature that may never be used.
    controller: Mutex<Option<MatterController>>,
    covers: RwLock<BTreeMap<u64, CoverConfig>>,
    /// One commissioning at a time: a second would race for the same window.
    commissioning: Mutex<()>,
    /// Where the house is, for the sun schedule.
    place: RwLock<Option<Place>>,
    http: reqwest::Client,
    schedule: Mutex<ScheduleState>,
}

impl MatterManager {
    pub fn new(config: &Config) -> Result<Self, AppError> {
        Self::with_paths(
            &config.matter_state_dir,
            &config.matter_trust_dir,
            config.matter_test_roots,
        )
    }

    pub fn with_paths(
        state_dir: &Path,
        trust_dir: &Path,
        test_roots: bool,
    ) -> Result<Self, AppError> {
        let covers = load_covers(&covers_path(state_dir))?;
        let place = store::read_json::<Option<Place>>(&place_path(state_dir), Corrupt::Fail)?;
        Ok(Self {
            inner: Arc::new(Inner {
                state_dir: state_dir.to_path_buf(),
                trust_dir: trust_dir.to_path_buf(),
                test_roots,
                controller: Mutex::new(None),
                covers: RwLock::new(covers),
                commissioning: Mutex::new(()),
                place: RwLock::new(place),
                http: crate::net::web_client(
                    concat!("Maison/", env!("CARGO_PKG_VERSION")),
                    Duration::from_secs(10),
                )?,
                schedule: Mutex::new(ScheduleState::default()),
            }),
        })
    }

    pub async fn list(&self) -> Vec<CoverView> {
        let covers: Vec<CoverConfig> = self.inner.covers.read().await.values().cloned().collect();
        let controller = match self.controller().await {
            Ok(controller) => Some(controller),
            Err(error) => {
                tracing::warn!(%error, "matter controller unavailable");
                None
            }
        };
        let next = self.next_events().await;
        let reads = covers.into_iter().map(|cover| {
            let controller = controller.clone();
            async move {
                match controller {
                    Some(controller) => read_cover(&controller, cover, next).await,
                    None => offline_view(&cover, next, "Matter controller unavailable".into()),
                }
            }
        });
        futures::future::join_all(reads).await
    }

    pub async fn get(&self, id: &str) -> Result<CoverView, AppError> {
        let cover = self.cover(id).await?;
        let controller = self.controller().await?;
        Ok(read_cover(&controller, cover, self.next_events().await).await)
    }

    pub async fn commission(&self, code: &str, name: &str) -> Result<CoverView, AppError> {
        let code = normalize_setup_code(code)?;
        let name = normalize_name(name)?;
        let Ok(_guard) = self.inner.commissioning.try_lock() else {
            return Err(AppError::http(
                StatusCode::CONFLICT,
                "A commissioning is already in progress",
            ));
        };

        let controller = self.controller().await?;
        ensure_fabric(&controller).await?;

        tracing::info!(%name, "commissioning matter device");
        let info = within(
            COMMISSION_TIMEOUT,
            controller.commission(&code, Some(name.clone())),
        )
        .await
        .map_err(commission_error)?;

        let node = controller.node(info.node_id);
        let endpoint = match find_covering_endpoint(&node).await {
            Ok(Some(endpoint)) => endpoint,
            outcome => {
                // Not a shutter (or it stopped answering): leave the device as
                // we found it rather than squatting one of its fabric slots.
                decommission(&controller, info.node_id).await;
                return Err(match outcome {
                    Ok(_) => AppError::http(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "This Matter device exposes no window covering",
                    ),
                    Err(error) => operation_error(error),
                });
            }
        };

        let cover = CoverConfig {
            node_id: info.node_id,
            name,
            endpoint,
            vendor_id: info.vendor_id,
            product_id: info.product_id,
            schedule: SunSchedule::default(),
            skip_open_at: None,
            skip_close_at: None,
        };
        tracing::info!(
            node_id = format_args!("{:016X}", info.node_id),
            endpoint,
            "matter cover commissioned"
        );
        {
            let mut covers = self.inner.covers.write().await;
            covers.insert(cover.node_id, cover.clone());
            self.persist(&covers).await?;
        }
        Ok(read_cover(&controller, cover, self.next_events().await).await)
    }

    /// A person's order: it also replaces any schedule order still being retried.
    pub async fn command(&self, id: &str, command: CoverCommand) -> Result<CoverView, AppError> {
        let cover = self.cover(id).await?;
        self.inner
            .schedule
            .lock()
            .await
            .pending
            .remove(&cover.node_id);
        let controller = self.controller().await?;
        invoke(&controller, &cover, command).await?;
        Ok(read_cover(&controller, cover, self.next_events().await).await)
    }

    pub async fn rename(&self, id: &str, name: &str) -> Result<CoverView, AppError> {
        let name = normalize_name(name)?;
        self.update(id, |cover| cover.name = name).await
    }

    pub async fn set_schedule(
        &self,
        id: &str,
        schedule: SunSchedule,
    ) -> Result<CoverView, AppError> {
        let within = -MAX_SUN_OFFSET_MIN..=MAX_SUN_OFFSET_MIN;
        if !within.contains(&schedule.sunrise_offset_min)
            || !within.contains(&schedule.sunset_offset_min)
        {
            return Err(AppError::bad_request("Offsets must stay within 3 hours"));
        }
        self.update(id, move |cover| {
            cover.schedule = schedule;
            // a skip kept on an event switched off would skip the first one after it is back
            if !schedule.open_at_sunrise {
                cover.skip_open_at = None;
            }
            if !schedule.close_at_sunset {
                cover.skip_close_at = None;
            }
        })
        .await
    }

    /// Skips the schedule's next `event` (`Open` or `Close`), or stops skipping it. Its
    /// order still being retried goes too: the person said not now.
    pub async fn set_skip(
        &self,
        id: &str,
        event: CoverCommand,
        skip: bool,
    ) -> Result<CoverView, AppError> {
        self.skip_at(id, event, skip, chrono::Utc::now()).await
    }

    /// `set_skip` as seen at `now` (the tests' clock).
    async fn skip_at(
        &self,
        id: &str,
        event: CoverCommand,
        skip: bool,
        now: Time,
    ) -> Result<CoverView, AppError> {
        let cover = self.cover(id).await?;
        let mut skipped = None;
        if skip {
            let (open, close) = cover.next_times(self.next_events_at(now).await);
            let next = if event == CoverCommand::Open {
                open
            } else {
                close
            };
            skipped = Some(next.ok_or_else(|| AppError::bad_request("Nothing scheduled to skip"))?);
            let mut schedule = self.inner.schedule.lock().await;
            if schedule
                .pending
                .get(&cover.node_id)
                .is_some_and(|order| order.command == event)
            {
                schedule.pending.remove(&cover.node_id);
            }
        }
        let cover = self
            .change(cover.node_id, move |cover| {
                if let Some(slot) = cover.skip_slot(event) {
                    *slot = skipped;
                }
            })
            .await?;
        Ok(self.view_at(cover, now).await)
    }

    async fn update(
        &self,
        id: &str,
        change: impl FnOnce(&mut CoverConfig),
    ) -> Result<CoverView, AppError> {
        let cover = self.change(parse_id(id)?, change).await?;
        Ok(self.view(cover).await)
    }

    /// Changes a cover: saved first, then kept (a failed save leaves memory as on disk).
    async fn change(
        &self,
        node_id: u64,
        change: impl FnOnce(&mut CoverConfig),
    ) -> Result<CoverConfig, AppError> {
        let mut covers = self.inner.covers.write().await;
        let mut next = covers.clone();
        let cover = next.get_mut(&node_id).ok_or_else(unknown_shutter)?;
        change(cover);
        let cover = cover.clone();
        self.persist(&next).await?;
        *covers = next;
        Ok(cover)
    }

    /// The cover as its switch says, or as kept when the controller is down: a saved change
    /// is not reported as failed because the switch cannot be asked.
    async fn view(&self, cover: CoverConfig) -> CoverView {
        self.view_at(cover, chrono::Utc::now()).await
    }

    async fn view_at(&self, cover: CoverConfig, now: Time) -> CoverView {
        let next = self.next_events_at(now).await;
        match self.controller().await {
            Ok(controller) => read_cover(&controller, cover, next).await,
            Err(error) => offline_view(&cover, next, error.to_string()),
        }
    }

    // ── the house's place, and the sun schedule ──

    pub async fn place(&self) -> Option<Place> {
        self.inner.place.read().await.clone()
    }

    /// The next sunrise and sunset at the house, strictly after `now` (the schedule's own
    /// reckoning); `None` with no place set. Either is `None` in a polar day or night.
    pub async fn next_sun(&self, now: Time) -> Option<(Option<Time>, Option<Time>)> {
        let place = self.inner.place.read().await.clone()?;
        let next = next_sun(&place, now);
        Some((next.sunrise, next.sunset))
    }

    pub async fn set_place(&self, place: Place) -> Result<Place, AppError> {
        if !(-90.0..=90.0).contains(&place.latitude) || !(-180.0..=180.0).contains(&place.longitude)
        {
            return Err(AppError::bad_request("Invalid coordinates"));
        }
        store::private_dir(&self.inner.state_dir)?;
        store::write_json_async(
            &place_path(&self.inner.state_dir),
            &Some(&place),
            Access::Private,
        )
        .await?;
        *self.inner.place.write().await = Some(place.clone());
        Ok(place)
    }

    pub async fn search_places(&self, query: &str, language: &str) -> Result<Vec<Place>, AppError> {
        sun::search(&self.inner.http, query, language).await
    }

    /// The next sunrise and sunset after now, at the house (none without a place).
    async fn next_events(&self) -> NextSun {
        self.next_events_at(chrono::Utc::now()).await
    }

    async fn next_events_at(&self, now: Time) -> NextSun {
        let Some(place) = self.place().await else {
            return NextSun::default();
        };
        next_sun(&place, now)
    }

    /// One look of the scheduler (every 30 s): sends what fell due, and retries what failed.
    pub async fn run_schedule(&self) {
        self.schedule_tick(chrono::Utc::now(), clock_trusted(), |cover, command| {
            let manager = self.clone();
            async move {
                let controller = manager.controller().await?;
                invoke(&controller, &cover, command).await
            }
        })
        .await;
    }

    /// The scheduler's look at `now`, `send` doing the sending (a fake in the tests).
    async fn schedule_tick<F, Fut>(&self, now: Time, trusted: bool, send: F)
    where
        F: Fn(CoverConfig, CoverCommand) -> Fut,
        Fut: std::future::Future<Output = Result<(), AppError>>,
    {
        let place = self.place().await;
        let covers: Vec<CoverConfig> = self.inner.covers.read().await.values().cloned().collect();
        let Look { orders, skipped } =
            self.inner
                .schedule
                .lock()
                .await
                .tick(now, trusted, place.as_ref(), &covers);
        for (node_id, event) in skipped {
            let cleared = self.change(node_id, |cover| {
                if let Some(slot) = cover.skip_slot(event) {
                    *slot = None;
                }
            });
            match cleared.await {
                Ok(cover) => {
                    tracing::info!(cover = %cover.name, ?event, "sun schedule: skipped once")
                }
                Err(error) => {
                    tracing::warn!(%error, ?event, "sun schedule: skipped, but the skip could not be cleared")
                }
            }
        }
        // a skip whose time passed unseen (the Pi down, its clock unsure) goes too
        for cover in covers.iter().filter(|c| trusted && c.past_skip(now)) {
            if let Err(error) = self.change(cover.node_id, |c| c.expire_skips(now)).await {
                tracing::warn!(%error, cover = %cover.name, "sun schedule: a past skip could not be cleared");
            }
        }
        for (cover, order) in orders {
            tracing::info!(cover = %cover.name, command = ?order.command, at = %order.at, "sun schedule");
            match send(cover.clone(), order.command).await {
                Ok(()) => self.inner.schedule.lock().await.sent(cover.node_id, order),
                Err(error) => {
                    tracing::warn!(cover = %cover.name, %error, "sun schedule: command failed, retrying")
                }
            }
        }
    }

    /// Leave the device's fabric table (best effort: an unplugged switch can
    /// still be forgotten) and drop it locally.
    pub async fn remove(&self, id: &str) -> Result<(), AppError> {
        let cover = self.cover(id).await?;
        let controller = self.controller().await?;
        decommission(&controller, cover.node_id).await;
        let mut covers = self.inner.covers.write().await;
        covers.remove(&cover.node_id);
        self.persist(&covers).await
    }

    async fn cover(&self, id: &str) -> Result<CoverConfig, AppError> {
        let node_id = parse_id(id)?;
        self.inner
            .covers
            .read()
            .await
            .get(&node_id)
            .cloned()
            .ok_or_else(unknown_shutter)
    }

    async fn controller(&self) -> Result<MatterController, AppError> {
        let mut slot = self.inner.controller.lock().await;
        if let Some(controller) = slot.as_ref() {
            return Ok(controller.clone());
        }
        let controller = self.build_controller().await?;
        *slot = Some(controller.clone());
        Ok(controller)
    }

    async fn build_controller(&self) -> Result<MatterController, AppError> {
        let trust = if self.inner.test_roots {
            tracing::warn!("matter: trusting CSA test roots only (MATTER_TEST_ROOTS)");
            AttestationTrust::example_device_roots()
        } else {
            load_trust(&self.inner.trust_dir)?
        };
        store::private_dir(&self.inner.state_dir)?;
        let store = Arc::new(FileStore::new(self.inner.state_dir.join("controller.bin")));
        MatterController::builder(store)
            .attestation_trust(trust)
            .response_deadline(COMMAND_TIMEOUT)
            .build()
            .await
            .map_err(|error| {
                AppError::service_unavailable(format!("Matter controller failed to start: {error}"))
            })
    }

    async fn persist(&self, covers: &BTreeMap<u64, CoverConfig>) -> Result<(), AppError> {
        store::private_dir(&self.inner.state_dir)?;
        let list: Vec<&CoverConfig> = covers.values().collect();
        store::write_json_async(&covers_path(&self.inner.state_dir), &list, Access::Private).await
    }
}

fn covers_path(state_dir: &Path) -> PathBuf {
    state_dir.join("covers.json")
}

fn place_path(state_dir: &Path) -> PathBuf {
    state_dir.join("place.json")
}

/// The kept covers, without the skips whose time is over (the Pi was off then).
fn load_covers(path: &Path) -> Result<BTreeMap<u64, CoverConfig>, AppError> {
    let list: Vec<CoverConfig> = store::read_json(path, Corrupt::Fail)?;
    let now = chrono::Utc::now();
    Ok(list
        .into_iter()
        .map(|mut cover| {
            cover.expire_skips(now);
            (cover.node_id, cover)
        })
        .collect())
}

pub(crate) fn load_trust(trust_dir: &Path) -> Result<AttestationTrust, AppError> {
    AttestationTrust::from_dirs(&trust_dir.join("paa"), &trust_dir.join("cd")).map_err(|error| {
        AppError::http(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!(
                "Matter attestation roots unusable in {} ({error}); run scripts/update-matter-trust.sh",
                trust_dir.display()
            ),
        )
    })
}

/// One fabric per Maison install, created on the first commissioning rather
/// than at boot: the Pi has no RTC, and certificates minted before NTP has
/// set the clock would be dated 1970.
async fn ensure_fabric(controller: &MatterController) -> Result<(), AppError> {
    let fabrics = controller.fabrics().await.map_err(operation_error)?;
    if !fabrics.is_empty() {
        return Ok(());
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    // Backdated an hour so a device whose clock lags slightly still accepts
    // the certificates.
    let validity = (
        MatterTime::from_unix_secs(now.saturating_sub(3600)),
        MatterTime::NO_EXPIRY,
    );
    let fabric_id = rand::random::<u64>().max(1);
    controller
        .create_fabric(FabricConfig::new(fabric_id, 1, 1, validity))
        .await
        .map_err(operation_error)?;
    tracing::info!(
        fabric_id = format_args!("{fabric_id:016X}"),
        "matter fabric created"
    );
    Ok(())
}

async fn find_covering_endpoint(node: &matter_controller::Node) -> Result<Option<u16>, CallError> {
    let path = ReadPath::new(None, Some(WINDOW_COVERING), Some(ATTR_CLUSTER_REVISION));
    let reports = within(COMMAND_TIMEOUT, node.read(&[path])).await?;
    Ok(reports.iter().map(|(path, _)| path.endpoint).min())
}

async fn decommission(controller: &MatterController, node_id: u64) {
    let node = controller.node(node_id);
    let index = within(
        COMMAND_TIMEOUT,
        node.read(&[ReadPath::concrete(
            0,
            OPERATIONAL_CREDENTIALS,
            ATTR_CURRENT_FABRIC_INDEX,
        )]),
    )
    .await;
    match index {
        Ok(reports) => {
            if let Some((_, Value::Uint(index))) = reports.first() {
                let path = CommandPath {
                    endpoint: 0,
                    cluster: OPERATIONAL_CREDENTIALS,
                    command: CMD_REMOVE_FABRIC,
                };
                let fields = Value::Structure(vec![(Tag::Context(0), Value::Uint(*index))]);
                // The device drops our session as it removes the fabric, so
                // the response may never arrive: a timeout here is expected.
                let outcome = within(COMMAND_TIMEOUT, node.invoke(path, fields)).await;
                tracing::info!(
                    node_id = format_args!("{node_id:016X}"),
                    ?outcome,
                    "matter fabric removal"
                );
            }
        }
        Err(error) => {
            tracing::warn!(%error, "matter: cannot read fabric index, forgetting locally")
        }
    }
    if let Err(error) = controller.forget_node(node_id).await {
        tracing::warn!(%error, "matter: forget_node failed");
    }
}

/// Sends `command` to the cover's switch; a refusal is an error.
async fn invoke(
    controller: &MatterController,
    cover: &CoverConfig,
    command: CoverCommand,
) -> Result<(), AppError> {
    let (command_id, fields) = match command {
        CoverCommand::Open => (CMD_UP_OR_OPEN, Value::Structure(Vec::new())),
        CoverCommand::Close => (CMD_DOWN_OR_CLOSE, Value::Structure(Vec::new())),
        CoverCommand::Stop => (CMD_STOP_MOTION, Value::Structure(Vec::new())),
        CoverCommand::OpenPercent(open) => (
            CMD_GO_TO_LIFT_PERCENTAGE,
            Value::Structure(vec![(
                Tag::Context(0),
                Value::Uint(u64::from(open_to_closure_100ths(open))),
            )]),
        ),
    };
    let path = CommandPath {
        endpoint: cover.endpoint,
        cluster: WINDOW_COVERING,
        command: command_id,
    };
    let result = within(
        COMMAND_TIMEOUT,
        controller.node(cover.node_id).invoke(path, fields),
    )
    .await
    .map_err(operation_error)?;
    if let matter_controller::InvokeResult::Status(status) = result {
        rejected(cover, [status])?;
    }
    Ok(())
}

async fn read_cover(controller: &MatterController, cover: CoverConfig, next: NextSun) -> CoverView {
    let paths = [
        ReadPath::concrete(
            cover.endpoint,
            WINDOW_COVERING,
            ATTR_CURRENT_LIFT_PERCENT100THS,
        ),
        ReadPath::concrete(
            cover.endpoint,
            WINDOW_COVERING,
            ATTR_TARGET_LIFT_PERCENT100THS,
        ),
        ReadPath::concrete(cover.endpoint, WINDOW_COVERING, ATTR_OPERATIONAL_STATUS),
    ];
    let reports = match within(READ_TIMEOUT, controller.node(cover.node_id).read(&paths)).await {
        Ok(reports) => reports,
        Err(error) => return offline_view(&cover, next, error.to_string()),
    };

    let mut view = offline_view(&cover, next, String::new());
    view.online = true;
    view.error = None;
    for (path, value) in reports {
        match (path.attribute, value) {
            (ATTR_CURRENT_LIFT_PERCENT100THS, Value::Uint(closure)) => {
                view.open_percent = Some(closure_100ths_to_open(closure));
            }
            (ATTR_TARGET_LIFT_PERCENT100THS, Value::Uint(closure)) => {
                view.target_open_percent = Some(closure_100ths_to_open(closure));
            }
            (ATTR_OPERATIONAL_STATUS, Value::Uint(status)) => {
                view.motion = Some(motion_from_status(status));
            }
            _ => {}
        }
    }
    view
}

fn offline_view(cover: &CoverConfig, next: NextSun, error: String) -> CoverView {
    let (next_open, next_close) = cover.next_times(next);
    // what the web strikes through: the next scheduled time, when it is the skipped one
    let skipped = |event, at: Option<Time>| at.is_some_and(|at| cover.skips(event, at));
    CoverView {
        id: format_id(cover.node_id),
        name: cover.name.clone(),
        endpoint: cover.endpoint,
        online: false,
        open_percent: None,
        target_open_percent: None,
        motion: None,
        vendor_id: cover.vendor_id,
        product_id: cover.product_id,
        schedule: cover.schedule,
        next_open,
        next_close,
        skip_next_open: skipped(CoverCommand::Open, next_open),
        skip_next_close: skipped(CoverCommand::Close, next_close),
        error: Some(error),
    }
}

/// A switch's IM statuses: any failure is the command refused, said with the cover's name.
fn rejected(
    cover: &CoverConfig,
    statuses: impl IntoIterator<Item = matter_controller::ImStatus>,
) -> Result<(), AppError> {
    match statuses.into_iter().find_map(|s| match s {
        matter_controller::ImStatus::Failure(code) => Some(code),
        _ => None,
    }) {
        Some(code) => Err(AppError::http(
            StatusCode::BAD_GATEWAY,
            format!("{} rejected the command (status {code:#04x})", cover.name),
        )),
        None => Ok(()),
    }
}

/// OperationalStatus bits 0–1 are the global movement state.
fn motion_from_status(status: u64) -> CoverMotion {
    match status & 0b11 {
        1 => CoverMotion::Opening,
        2 => CoverMotion::Closing,
        _ => CoverMotion::Stopped,
    }
}

fn closure_100ths_to_open(closure: u64) -> u8 {
    let closure = closure.min(10_000);
    // Rounded to the nearest percent.
    100 - u8::try_from((closure + 50) / 100).unwrap_or(100)
}

fn open_to_closure_100ths(open: u8) -> u16 {
    (100 - u16::from(open.min(100))) * 100
}

fn format_id(node_id: u64) -> String {
    format!("{node_id:016x}")
}

fn parse_id(id: &str) -> Result<u64, AppError> {
    if id.len() != 16 {
        return Err(unknown_shutter());
    }
    u64::from_str_radix(id, 16).map_err(|_| unknown_shutter())
}

fn unknown_shutter() -> AppError {
    AppError::not_found("Unknown shutter")
}

fn normalize_name(name: &str) -> Result<String, AppError> {
    crate::util::name(name).map(str::to_string)
}

/// Accept a QR payload (`MT:…`) as-is, and a manual pairing code with the
/// spaces/dashes apps print it with (`3497-011-2332` → `34970112332`).
fn normalize_setup_code(code: &str) -> Result<String, AppError> {
    let code = code.trim();
    if code.starts_with("MT:") {
        return Ok(code.to_string());
    }
    let digits: String = code.chars().filter(|c| !matches!(c, ' ' | '-')).collect();
    if (digits.len() == 11 || digits.len() == 21) && digits.bytes().all(|b| b.is_ascii_digit()) {
        Ok(digits)
    } else {
        Err(AppError::bad_request(
            "Expected an 11- or 21-digit pairing code, or an MT: QR payload",
        ))
    }
}

/// Why a Matter call failed: no answer in time, or the controller or the device said no.
#[derive(Debug, thiserror::Error)]
enum CallError {
    #[error("no answer")]
    NoAnswer,
    #[error(transparent)]
    Matter(#[from] matter_controller::Error),
}

/// Every Matter call goes through here: bounded in time, so a silent device is one more error.
async fn within<T>(
    limit: Duration,
    call: impl std::future::Future<Output = Result<T, matter_controller::Error>>,
) -> Result<T, CallError> {
    match tokio::time::timeout(limit, call).await {
        Ok(result) => Ok(result?),
        Err(_) => Err(CallError::NoAnswer),
    }
}

fn commission_error(error: CallError) -> AppError {
    use matter_controller::Error;
    match error {
        CallError::NoAnswer => AppError::http(
            StatusCode::GATEWAY_TIMEOUT,
            "Commissioning timed out: is the device on the Wi-Fi with its pairing window open?",
        ),
        CallError::Matter(Error::SetupCode(detail)) => {
            AppError::bad_request(format!("Invalid pairing code: {detail}"))
        }
        CallError::Matter(Error::SystemClockUnset(_)) => AppError::service_unavailable(
            "The system clock is not set yet (waiting for NTP); retry in a minute",
        ),
        other => AppError::http(
            StatusCode::BAD_GATEWAY,
            format!("Commissioning failed: {other}"),
        ),
    }
}

fn operation_error(error: impl Into<CallError>) -> AppError {
    let error = error.into();
    AppError::service_unavailable(format!("Matter: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use schedule::{
        tests::{paris, utc, BOTH},
        Order, RETRY_FOR,
    };

    #[test]
    fn position_conversions_invert_matter_closure() {
        assert_eq!(closure_100ths_to_open(0), 100);
        assert_eq!(closure_100ths_to_open(10_000), 0);
        assert_eq!(closure_100ths_to_open(2_549), 75);
        assert_eq!(closure_100ths_to_open(2_550), 74);
        assert_eq!(closure_100ths_to_open(65_535), 0);
        assert_eq!(open_to_closure_100ths(100), 0);
        assert_eq!(open_to_closure_100ths(0), 10_000);
        assert_eq!(open_to_closure_100ths(30), 7_000);
        assert_eq!(open_to_closure_100ths(250), 0);
    }

    #[test]
    fn motion_reads_the_global_bits_only() {
        assert_eq!(motion_from_status(0b00_00_01), CoverMotion::Opening);
        assert_eq!(motion_from_status(0b00_10_10), CoverMotion::Closing);
        assert_eq!(motion_from_status(0b00_01_00), CoverMotion::Stopped);
    }

    #[test]
    fn setup_codes_are_normalized() {
        assert_eq!(
            normalize_setup_code(" 3497-011-2332 ").unwrap(),
            "34970112332"
        );
        assert_eq!(
            normalize_setup_code("MT:Y.K90AFN00KA0648G00").unwrap(),
            "MT:Y.K90AFN00KA0648G00"
        );
        assert!(normalize_setup_code("1234").is_err());
        assert!(normalize_setup_code("3497O112332").is_err());
    }

    #[test]
    fn ids_round_trip() {
        let id = format_id(0x00AB_CDEF_0123_4567);
        assert_eq!(parse_id(&id).unwrap(), 0x00AB_CDEF_0123_4567);
        assert!(parse_id("abc").is_err());
    }

    // ── the scheduler: retries, give-up, supersession, clock trust ──

    /// A switch that answers or not, and what it was sent.
    #[derive(Default)]
    struct FakeSwitch {
        sent: std::sync::Mutex<Vec<CoverCommand>>,
        offline: std::sync::atomic::AtomicBool,
    }

    impl FakeSwitch {
        fn sent(&self) -> Vec<CoverCommand> {
            self.sent.lock().unwrap().clone()
        }

        fn set_offline(&self, offline: bool) {
            self.offline
                .store(offline, std::sync::atomic::Ordering::SeqCst);
        }
    }

    async fn manager_with_one_cover(schedule: SunSchedule) -> (tempfile::TempDir, MatterManager) {
        let dir = crate::util::test_dir();
        // no attestation roots there: the controller fails at once, views are the kept ones
        let manager = MatterManager::with_paths(dir.path(), dir.path(), false).unwrap();
        let cover = CoverConfig {
            node_id: 1,
            name: "Salon".into(),
            endpoint: 1,
            vendor_id: None,
            product_id: None,
            schedule,
            skip_open_at: None,
            skip_close_at: None,
        };
        manager.inner.covers.write().await.insert(1, cover);
        *manager.inner.place.write().await = Some(paris());
        (dir, manager)
    }

    async fn look(
        manager: &MatterManager,
        now: chrono::DateTime<chrono::Utc>,
        trusted: bool,
        switch: &FakeSwitch,
    ) {
        manager
            .schedule_tick(now, trusted, |_, command| {
                switch.sent.lock().unwrap().push(command);
                let offline = switch.offline.load(std::sync::atomic::Ordering::SeqCst);
                async move {
                    if offline {
                        Err(AppError::service_unavailable("offline"))
                    } else {
                        Ok(())
                    }
                }
            })
            .await;
    }

    /// Looks every 30 s over `from..to`.
    async fn looks(
        manager: &MatterManager,
        from: chrono::DateTime<chrono::Utc>,
        to: chrono::DateTime<chrono::Utc>,
        switch: &FakeSwitch,
    ) {
        let mut now = from;
        while now <= to {
            look(manager, now, true, switch).await;
            now += chrono::TimeDelta::seconds(30);
        }
    }

    fn sunset(day: chrono::NaiveDate) -> chrono::DateTime<chrono::Utc> {
        sun::sun_times(&paris(), day).1.unwrap()
    }

    #[tokio::test]
    async fn a_failed_order_is_retried_until_it_goes_through() {
        let (_dir, manager) = manager_with_one_cover(BOTH).await;
        let switch = FakeSwitch::default();
        let set = sunset(chrono::NaiveDate::from_ymd_opt(2026, 10, 2).unwrap());
        switch.set_offline(true);
        looks(
            &manager,
            set - chrono::TimeDelta::minutes(1),
            set + chrono::TimeDelta::minutes(10),
            &switch,
        )
        .await;
        let tries = switch.sent().len();
        assert!(tries >= 19, "retried every look: {tries}");
        assert!(switch.sent().iter().all(|c| *c == CoverCommand::Close));
        switch.set_offline(false);
        looks(
            &manager,
            set + chrono::TimeDelta::minutes(11),
            set + chrono::TimeDelta::minutes(20),
            &switch,
        )
        .await;
        assert_eq!(switch.sent().len(), tries + 1, "sent once more, then done");
    }

    #[tokio::test]
    async fn a_failed_order_is_given_up_after_half_an_hour() {
        let (_dir, manager) = manager_with_one_cover(BOTH).await;
        let switch = FakeSwitch::default();
        let set = sunset(chrono::NaiveDate::from_ymd_opt(2026, 10, 2).unwrap());
        switch.set_offline(true);
        looks(
            &manager,
            set - chrono::TimeDelta::minutes(1),
            set + RETRY_FOR,
            &switch,
        )
        .await;
        let tries = switch.sent().len();
        switch.set_offline(false);
        looks(
            &manager,
            set + RETRY_FOR + chrono::TimeDelta::seconds(30),
            set + chrono::TimeDelta::hours(2),
            &switch,
        )
        .await;
        assert_eq!(
            switch.sent().len(),
            tries,
            "nothing after 30 min: the shutter must not close hours late"
        );
    }

    #[tokio::test]
    async fn a_newer_event_or_a_person_replaces_a_pending_order() {
        let (_dir, manager) = manager_with_one_cover(BOTH).await;
        let set = sunset(chrono::NaiveDate::from_ymd_opt(2026, 10, 2).unwrap());
        let old = Order {
            command: CoverCommand::Open,
            at: set - chrono::TimeDelta::minutes(10),
        };
        manager.inner.schedule.lock().await.pending.insert(1, old);
        let switch = FakeSwitch::default();
        switch.set_offline(true);
        looks(
            &manager,
            set - chrono::TimeDelta::minutes(1),
            set + chrono::TimeDelta::minutes(1),
            &switch,
        )
        .await;
        assert_eq!(
            switch.sent().first(),
            Some(&CoverCommand::Open),
            "retried while it was the latest"
        );
        assert_eq!(
            switch.sent().last(),
            Some(&CoverCommand::Close),
            "then the sunset replaced it"
        );
        let mut state = manager.inner.schedule.lock().await;
        state.sent(1, old);
        assert!(
            state.pending.contains_key(&1),
            "the old order's success does not clear the new one"
        );
        drop(state);
        // a person's order wins over the schedule's retries (the command fails here: no switch)
        let _ = manager.command(&format_id(1), CoverCommand::Open).await;
        assert!(manager.inner.schedule.lock().await.pending.is_empty());
    }

    #[tokio::test]
    async fn nothing_fires_before_the_clock_is_trusted() {
        let (_dir, manager) = manager_with_one_cover(BOTH).await;
        let switch = FakeSwitch::default();
        let set = sunset(chrono::NaiveDate::from_ymd_opt(2026, 10, 2).unwrap());
        // swclock's stale time around a sunset, then NTP: neither is replayed
        let mut now = set - chrono::TimeDelta::minutes(2);
        while now <= set + chrono::TimeDelta::minutes(2) {
            look(&manager, now, false, &switch).await;
            now += chrono::TimeDelta::seconds(30);
        }
        look(&manager, set + chrono::TimeDelta::minutes(3), true, &switch).await;
        look(&manager, set + chrono::TimeDelta::minutes(4), true, &switch).await;
        assert!(switch.sent().is_empty(), "{:?}", switch.sent());
        // trusted from then on: the next event fires
        let next = sunset(chrono::NaiveDate::from_ymd_opt(2026, 10, 3).unwrap());
        looks(
            &manager,
            next - chrono::TimeDelta::minutes(1),
            next + chrono::TimeDelta::minutes(1),
            &switch,
        )
        .await;
        assert_eq!(switch.sent(), vec![CoverCommand::Close]);
    }

    #[tokio::test]
    async fn the_days_the_clocks_change_open_and_close_once_each() {
        for (from, to) in [
            ("2026-10-24T12:00:00Z", "2026-10-26T12:00:00Z"),
            ("2026-03-28T12:00:00Z", "2026-03-30T12:00:00Z"),
        ] {
            let (_dir, manager) = manager_with_one_cover(BOTH).await;
            let switch = FakeSwitch::default();
            looks(&manager, utc(from), utc(to), &switch).await;
            let sent = switch.sent();
            let opens = sent.iter().filter(|c| **c == CoverCommand::Open).count();
            let closes = sent.iter().filter(|c| **c == CoverCommand::Close).count();
            assert_eq!((opens, closes), (2, 2), "{from}: {sent:?}");
        }
    }

    fn day(d: u32) -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(2026, 10, d).unwrap()
    }

    fn sunrise(day: chrono::NaiveDate) -> chrono::DateTime<chrono::Utc> {
        sun::sun_times(&paris(), day).0.unwrap()
    }

    async fn kept(manager: &MatterManager) -> CoverConfig {
        manager.cover(&format_id(1)).await.unwrap()
    }

    #[tokio::test]
    async fn a_skipped_closing_does_not_happen_and_the_skip_clears() {
        let (_dir, manager) = manager_with_one_cover(BOTH).await;
        let minute = chrono::TimeDelta::minutes(1);
        let set = sunset(day(2));
        let view = manager
            .skip_at(
                &format_id(1),
                CoverCommand::Close,
                true,
                set - chrono::TimeDelta::hours(2),
            )
            .await
            .unwrap();
        assert!(
            view.skip_next_close && view.next_close == Some(set),
            "tonight's, struck: {view:?}"
        );
        assert_eq!(
            kept(&manager).await.skip_close_at,
            Some(set),
            "the occurrence is named"
        );
        let switch = FakeSwitch::default();
        looks(&manager, set - minute, set + minute * 10, &switch).await;
        assert!(switch.sent().is_empty(), "{:?}", switch.sent());
        assert!(
            kept(&manager).await.skip_close_at.is_none(),
            "cleared once its time passed"
        );
        let reloaded =
            MatterManager::with_paths(&manager.inner.state_dir, &manager.inner.state_dir, false)
                .unwrap();
        assert!(
            reloaded
                .cover(&format_id(1))
                .await
                .unwrap()
                .skip_close_at
                .is_none(),
            "cleared on disk too"
        );
        // only that one: the next morning opens, the next evening closes
        looks(
            &manager,
            sunrise(day(3)) - minute,
            sunrise(day(3)) + minute,
            &switch,
        )
        .await;
        looks(
            &manager,
            sunset(day(3)) - minute,
            sunset(day(3)) + minute,
            &switch,
        )
        .await;
        assert_eq!(switch.sent(), [CoverCommand::Open, CoverCommand::Close]);
    }

    #[tokio::test]
    async fn a_skip_set_back_to_false_lets_the_event_happen() {
        let (_dir, manager) = manager_with_one_cover(BOTH).await;
        let rise = sunrise(day(3));
        let before = rise - chrono::TimeDelta::hours(3);
        manager
            .skip_at(&format_id(1), CoverCommand::Open, true, before)
            .await
            .unwrap();
        let view = manager
            .skip_at(&format_id(1), CoverCommand::Open, false, before)
            .await
            .unwrap();
        assert!(!view.skip_next_open);
        let switch = FakeSwitch::default();
        looks(
            &manager,
            rise - chrono::TimeDelta::minutes(1),
            rise + chrono::TimeDelta::minutes(1),
            &switch,
        )
        .await;
        assert_eq!(switch.sent(), [CoverCommand::Open]);
    }

    /// A skipped event leaves nothing to retry: an older order still pending for the
    /// cover goes too, and a person's skip drops the retries of that event.
    #[tokio::test]
    async fn a_skip_drops_the_pending_retries() {
        let (_dir, manager) = manager_with_one_cover(BOTH).await;
        let set = sunset(day(2));
        let stale = Order {
            command: CoverCommand::Close,
            at: set - chrono::TimeDelta::minutes(5),
        };
        manager.inner.schedule.lock().await.pending.insert(1, stale);
        manager
            .change(1, move |cover| cover.skip_close_at = Some(set))
            .await
            .unwrap();
        let switch = FakeSwitch::default();
        switch.set_offline(true);
        look(
            &manager,
            set - chrono::TimeDelta::seconds(10),
            true,
            &switch,
        )
        .await;
        assert_eq!(
            switch.sent(),
            [CoverCommand::Close],
            "the old order is retried"
        );
        switch.sent.lock().unwrap().clear();
        // then the skipped sunset: nothing sent, nothing left to retry
        looks(
            &manager,
            set + chrono::TimeDelta::seconds(20),
            set + chrono::TimeDelta::minutes(5),
            &switch,
        )
        .await;
        assert!(switch.sent().is_empty(), "{:?}", switch.sent());
        assert!(manager.inner.schedule.lock().await.pending.is_empty());

        // a closing being retried (the switch was off at sunset), then « not tonight »
        let next = sunset(day(3));
        looks(
            &manager,
            next - chrono::TimeDelta::minutes(1),
            next + chrono::TimeDelta::minutes(1),
            &switch,
        )
        .await;
        assert!(manager.inner.schedule.lock().await.pending.contains_key(&1));
        let view = manager
            .skip_at(
                &format_id(1),
                CoverCommand::Close,
                true,
                next + chrono::TimeDelta::minutes(1),
            )
            .await
            .unwrap();
        assert!(!view.skip_next_open);
        assert_eq!(
            kept(&manager).await.skip_close_at,
            Some(sunset(day(4))),
            "the next one: tonight's has passed"
        );
        assert!(
            manager.inner.schedule.lock().await.pending.is_empty(),
            "the retry is dropped"
        );
    }

    #[tokio::test]
    async fn only_a_scheduled_event_can_be_skipped() {
        let (_dir, manager) = manager_with_one_cover(SunSchedule {
            close_at_sunset: false,
            ..BOTH
        })
        .await;
        assert!(manager
            .set_skip(&format_id(1), CoverCommand::Close, true)
            .await
            .is_err());
        assert!(
            manager
                .set_skip(&format_id(1), CoverCommand::Close, false)
                .await
                .is_ok(),
            "unskipping always works"
        );
        assert!(
            manager
                .set_skip(&format_id(1), CoverCommand::Open, true)
                .await
                .unwrap()
                .skip_next_open
        );
        // switching the event off forgets its skip
        manager
            .set_schedule(
                &format_id(1),
                SunSchedule {
                    open_at_sunrise: false,
                    ..BOTH
                },
            )
            .await
            .unwrap();
        assert!(kept(&manager).await.skip_open_at.is_none());
    }

    /// The Pi off (or unsure of the time) when the skipped closing passes: the next
    /// evening still closes, and the stale skip goes.
    #[tokio::test]
    async fn a_skip_whose_time_passed_unseen_skips_nothing_else() {
        let minute = chrono::TimeDelta::minutes(1);
        for clock in ["down", "untrusted"] {
            let (_dir, manager) = manager_with_one_cover(BOTH).await;
            let tonight = sunset(day(2));
            manager
                .skip_at(
                    &format_id(1),
                    CoverCommand::Close,
                    true,
                    tonight - chrono::TimeDelta::hours(2),
                )
                .await
                .unwrap();
            let switch = FakeSwitch::default();
            if clock == "untrusted" {
                let mut now = tonight - minute;
                while now <= tonight + minute * 5 {
                    look(&manager, now, false, &switch).await;
                    now += chrono::TimeDelta::seconds(30);
                }
            }
            // back (and sure of the time) the next morning
            looks(
                &manager,
                sunrise(day(3)) - minute,
                sunrise(day(3)) + minute,
                &switch,
            )
            .await;
            assert!(
                kept(&manager).await.skip_close_at.is_none(),
                "{clock}: the past skip goes"
            );
            let view = manager
                .view_at(kept(&manager).await, sunrise(day(3)) + minute)
                .await;
            assert!(
                !view.skip_next_close,
                "{clock}: tomorrow's closing is not struck"
            );
            looks(
                &manager,
                sunset(day(3)) - minute,
                sunset(day(3)) + minute,
                &switch,
            )
            .await;
            assert_eq!(
                switch.sent(),
                [CoverCommand::Open, CoverCommand::Close],
                "{clock}"
            );
        }
    }

    /// Restarted after the skipped event: the skip is gone from the start.
    #[test]
    fn a_past_skip_is_forgotten_on_load() {
        let dir = crate::util::test_dir();
        let long_ago = chrono::Utc::now() - chrono::TimeDelta::days(2);
        let soon = chrono::Utc::now() + chrono::TimeDelta::hours(2);
        let covers = serde_json::json!([{
            "nodeId": 1, "name": "Salon", "endpoint": 1, "vendorId": null, "productId": null,
            "skipOpenAt": long_ago, "skipCloseAt": soon,
        }]);
        store::write_json(&covers_path(dir.path()), &covers, Access::Private).unwrap();
        let loaded = load_covers(&covers_path(dir.path())).unwrap();
        assert_eq!(
            (loaded[&1].skip_open_at, loaded[&1].skip_close_at),
            (None, Some(soon))
        );
    }

    #[test]
    fn names_follow_the_people_rule() {
        assert_eq!(normalize_name("  Volet salon ").unwrap(), "Volet salon");
        assert!(normalize_name("   ").is_err());
        assert!(normalize_name(&"x".repeat(crate::people::MAX_NAME + 1)).is_err());
    }

    #[test]
    fn an_empty_covers_file_is_an_error_not_no_shutters() {
        let dir = crate::util::test_dir();
        std::fs::write(covers_path(dir.path()), "").unwrap();
        assert!(MatterManager::with_paths(dir.path(), dir.path(), true).is_err());
    }

    /// Every vendored root must parse: `from_dirs` fails as a whole on a
    /// single bad certificate, which would block all commissioning.
    #[test]
    fn vendored_attestation_roots_load() {
        let trust_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../matter-trust");
        load_trust(&trust_dir).expect("matter-trust/ must load");
    }
}
