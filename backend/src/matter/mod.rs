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
//! Each shutter's position and motion come from a subscription (the switch reports what
//! changes, and at least every few minutes): the list is served from it, a read only when
//! it is not established. [`MatterManager::subscribe`] says the list as it changes.
//!
//! State lives in its own directory (default `matter/`) because the
//! controller persists its fabric keys by temp-file + rename: the directory
//! itself must belong to the service user, unlike the root-owned app dir.
//!
//! Position semantics follow the cluster, inverted for the UI: Matter counts
//! *closure* in hundredths of a percent (0 = fully open, 10000 = closed),
//! while every view here reports `openPercent` (100 = fully open).

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex as StdMutex, Weak},
    time::Duration,
};

use matter_codec::Tag;
use matter_controller::{
    AttestationTrust, CommandPath, FabricConfig, FileStore, MatterController, MatterTime, ReadPath,
    SubscriptionEvent, Value,
};
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{watch, Mutex, RwLock},
    task::JoinHandle,
};

use crate::{
    config::Config,
    error::{AppError, Refusal},
    json_config::{Checked, JsonConfig},
    store::{self, Access, Corrupt},
    sun::{self, NextSun, Place, SunTimes},
    util::{capped_backoff, lock},
};

mod schedule;

pub use maison_api::matter::{Cover, CoverMotion, SunSchedule};
use schedule::{clock_trusted, offset, Look, ScheduleState, MAX_SUN_OFFSET_MIN};

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
/// A read (when no subscription is established) fans out per device; an unreachable switch
/// must not hold the list hostage.
const READ_TIMEOUT: Duration = Duration::from_secs(6);
/// The client the geocoding asks with.
const WEB_TIMEOUT: Duration = Duration::from_secs(10);
/// A subscription's report interval asked of the switch: at most once a second while the
/// motor runs, and at least every five minutes (its heartbeat: a switch silent past it is
/// subscribed to again by the controller).
const REPORT_MIN_S: u16 = 1;
const REPORT_MAX_S: u16 = 300;
/// A subscription that would not be made is asked again after this, doubled each time.
const SUBSCRIBE_RETRY: Duration = Duration::from_secs(5);
const MAX_SUBSCRIBE_RETRY: Duration = Duration::from_secs(300);
/// A cover's follower, should it end or panic, starts again after this.
const FOLLOW_RESTART: Duration = Duration::from_secs(5);

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
    fn next_times(&self, next: SunTimes) -> (Option<Time>, Option<Time>) {
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

/// The covers' file (`covers.json`), kept in memory and on disk together.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
struct Covers(Vec<CoverConfig>);

impl Checked for Covers {}

impl Covers {
    /// The kept covers, the skips whose time is over forgotten (the Pi was off then), on
    /// disk too.
    fn load(path: &Path) -> Result<JsonConfig<Self>, AppError> {
        let mut covers = store::read_json::<Self>(path, Corrupt::Fail)?;
        let now = chrono::Utc::now();
        if covers.0.iter().any(|cover| cover.past_skip(now)) {
            for cover in &mut covers.0 {
                cover.expire_skips(now);
            }
            store::write_json(path, &covers, Access::Private)?;
        }
        JsonConfig::load_as(path, Access::Private)
    }

    fn get(&self, node_id: u64) -> Option<&CoverConfig> {
        self.0.iter().find(|cover| cover.node_id == node_id)
    }

    fn get_mut(&mut self, node_id: u64) -> Option<&mut CoverConfig> {
        self.0.iter_mut().find(|cover| cover.node_id == node_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CoverCommand {
    Open,
    Close,
    Stop,
    /// 0 = closed, 100 = fully open.
    OpenPercent(u8),
}

/// What a cover's switch said last: from its subscription, or from a read.
#[derive(Debug, Clone, Default, PartialEq)]
struct Reading {
    /// The subscription is established (the values are the switch's own, kept up to date).
    online: bool,
    open_percent: Option<u8>,
    target_open_percent: Option<u8>,
    motion: Option<CoverMotion>,
}

impl Reading {
    /// One attribute the switch reported (an unknown one, or a value of another type, is
    /// not ours to read).
    fn apply(&mut self, attribute: u32, value: &Value) {
        match (attribute, value) {
            (ATTR_CURRENT_LIFT_PERCENT100THS, Value::Uint(closure)) => {
                self.open_percent = Some(closure_100ths_to_open(*closure));
            }
            (ATTR_TARGET_LIFT_PERCENT100THS, Value::Uint(closure)) => {
                self.target_open_percent = Some(closure_100ths_to_open(*closure));
            }
            (ATTR_OPERATIONAL_STATUS, Value::Uint(status)) => {
                self.motion = Some(motion_from_status(*status));
            }
            _ => {}
        }
    }
}

/// A cover followed by its subscription: what it says, and the task that listens.
struct Live {
    reading: watch::Receiver<Reading>,
    task: JoinHandle<()>,
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
    covers: JsonConfig<Covers>,
    /// One commissioning at a time: a second would race for the same window.
    commissioning: Mutex<()>,
    /// Where the house is, for the sun schedule.
    place: RwLock<Option<Place>>,
    http: reqwest::Client,
    geocoding_url: String,
    schedule: Mutex<ScheduleState>,
    /// The covers followed by a subscription, by node id.
    live: StdMutex<HashMap<u64, Live>>,
    /// The covers as the API shows them, sent when a report or a change moved them.
    shown: watch::Sender<Vec<Cover>>,
}

/// The followers hold the manager weakly: dropped, it stops them.
impl Drop for Inner {
    fn drop(&mut self) {
        for live in lock(&self.live).values() {
            live.task.abort();
        }
    }
}

impl MatterManager {
    pub fn new(config: &Config) -> Result<Self, AppError> {
        Self::with_paths(
            &config.matter_state_dir,
            &config.matter_trust_dir,
            config.matter_test_roots,
            &config.geocoding_url,
        )
    }

    pub fn with_paths(
        state_dir: &Path,
        trust_dir: &Path,
        test_roots: bool,
        geocoding_url: &str,
    ) -> Result<Self, AppError> {
        let covers = Covers::load(&covers_path(state_dir))?;
        let place = store::read_json::<Option<Place>>(&place_path(state_dir), Corrupt::Fail)?;
        Ok(Self {
            inner: Arc::new(Inner {
                state_dir: state_dir.to_path_buf(),
                trust_dir: trust_dir.to_path_buf(),
                test_roots,
                controller: Mutex::new(None),
                covers,
                commissioning: Mutex::new(()),
                place: RwLock::new(place),
                http: crate::net::web_client(crate::net::USER_AGENT, WEB_TIMEOUT)?,
                geocoding_url: geocoding_url.to_string(),
                schedule: Mutex::new(ScheduleState::default()),
                live: StdMutex::new(HashMap::new()),
                shown: watch::Sender::new(Vec::new()),
            }),
        })
    }

    /// The covers as the API shows them, as they change (a switch's report, a rename, a
    /// schedule): for whoever pushes them to the apps.
    pub fn subscribe(&self) -> watch::Receiver<Vec<Cover>> {
        self.inner.shown.subscribe()
    }

    /// Every cover: from its subscription when it is established, else read.
    pub async fn list(&self) -> Vec<Cover> {
        self.follow_covers().await;
        let covers = self.inner.covers.get().await.0;
        let next = self.sun_times(chrono::Utc::now()).await;
        let views = covers.into_iter().map(|cover| self.current(cover, next));
        futures::future::join_all(views).await
    }

    pub async fn get(&self, id: &str) -> Result<Cover, AppError> {
        let cover = self.cover(id).await?;
        self.follow_covers().await;
        Ok(self
            .current(cover, self.sun_times(chrono::Utc::now()).await)
            .await)
    }

    /// A cover as its subscription says it, or as a read says it when none is established.
    async fn current(&self, cover: CoverConfig, next: SunTimes) -> Cover {
        let reading = self.reading(cover.node_id);
        if reading.online {
            return view_of(&cover, next, &reading);
        }
        self.view_at_once(cover, next).await
    }

    /// What the subscription of `node_id` said last (offline when there is none).
    fn reading(&self, node_id: u64) -> Reading {
        lock(&self.inner.live)
            .get(&node_id)
            .map(|live| live.reading.borrow().clone())
            .unwrap_or_default()
    }

    pub async fn commission(&self, code: &str, name: &str) -> Result<Cover, AppError> {
        let code = normalize_setup_code(code)?;
        let name = normalize_name(name)?;
        let Ok(_guard) = self.inner.commissioning.try_lock() else {
            return Err(Refusal::Busy.into());
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
                    Ok(_) => {
                        tracing::warn!("the Matter device commissioned exposes no window covering");
                        Refusal::Unsupported.into()
                    }
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
            node_id = %node_text(info.node_id),
            endpoint,
            "matter cover commissioned"
        );
        store::private_dir(&self.inner.state_dir)?;
        let kept = cover.clone();
        self.inner
            .covers
            .update(move |covers| {
                covers.0.retain(|c| c.node_id != kept.node_id);
                covers.0.push(kept);
                Ok(())
            })
            .await?;
        self.follow_covers().await;
        let next = self.sun_times(chrono::Utc::now()).await;
        let view = read_cover(&controller, cover, next).await;
        self.publish().await;
        Ok(view)
    }

    /// A person's order: it also replaces any schedule order still being retried.
    pub async fn command(&self, id: &str, command: CoverCommand) -> Result<Cover, AppError> {
        let cover = self.cover(id).await?;
        self.inner
            .schedule
            .lock()
            .await
            .pending
            .remove(&cover.node_id);
        let controller = self.controller().await?;
        invoke(&controller, &cover, command).await?;
        // a fresh read: the order's target, at once, whatever the subscription says yet
        let next = self.sun_times(chrono::Utc::now()).await;
        Ok(read_cover(&controller, cover, next).await)
    }

    pub async fn rename(&self, id: &str, name: &str) -> Result<Cover, AppError> {
        let name = normalize_name(name)?;
        self.update(id, |cover| cover.name = name).await
    }

    pub async fn set_schedule(&self, id: &str, schedule: SunSchedule) -> Result<Cover, AppError> {
        let within = -MAX_SUN_OFFSET_MIN..=MAX_SUN_OFFSET_MIN;
        if !within.contains(&schedule.sunrise_offset_min)
            || !within.contains(&schedule.sunset_offset_min)
        {
            return Err(Refusal::Invalid.into());
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
    ) -> Result<Cover, AppError> {
        self.skip_at(id, event, skip, chrono::Utc::now()).await
    }

    /// `set_skip` as seen at `now` (the tests' clock).
    async fn skip_at(
        &self,
        id: &str,
        event: CoverCommand,
        skip: bool,
        now: Time,
    ) -> Result<Cover, AppError> {
        let cover = self.cover(id).await?;
        let mut skipped = None;
        if skip {
            let (open, close) = cover.next_times(self.sun_times(now).await);
            let next = if event == CoverCommand::Open {
                open
            } else {
                close
            };
            // nothing scheduled: that event is not set up
            skipped = Some(next.ok_or(Refusal::NotConfigured)?);
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
    ) -> Result<Cover, AppError> {
        let cover = self.change(parse_id(id)?, change).await?;
        Ok(self.view(cover).await)
    }

    /// Changes a cover: saved first, then kept (`JsonConfig::update`: a failed save leaves
    /// memory as on disk); then said to the subscribers.
    async fn change(
        &self,
        node_id: u64,
        change: impl FnOnce(&mut CoverConfig),
    ) -> Result<CoverConfig, AppError> {
        store::private_dir(&self.inner.state_dir)?;
        let changed = self
            .inner
            .covers
            .update(|covers| {
                let cover = covers.get_mut(node_id).ok_or(Refusal::NotFound)?;
                change(cover);
                Ok(cover.clone())
            })
            .await?;
        self.publish().await;
        Ok(changed)
    }

    /// The cover as its switch says, or as kept when the controller is down: a saved change
    /// is not reported as failed because the switch cannot be asked.
    async fn view(&self, cover: CoverConfig) -> Cover {
        self.view_at(cover, chrono::Utc::now()).await
    }

    async fn view_at(&self, cover: CoverConfig, now: Time) -> Cover {
        self.current(cover, self.sun_times(now).await).await
    }

    /// The cover read now (no subscription to go by), or as kept when the controller is
    /// down (said `internal`: ours, the detail in the log).
    async fn view_at_once(&self, cover: CoverConfig, next: SunTimes) -> Cover {
        match self.controller().await {
            Ok(controller) => read_cover(&controller, cover, next).await,
            Err(error) => {
                tracing::debug!(%error, "matter controller unavailable: the cover as kept");
                offline_view(&cover, next, Refusal::Internal)
            }
        }
    }

    // ── the house's place, and the sun ──

    pub async fn place(&self) -> Option<Place> {
        self.inner.place.read().await.clone()
    }

    /// When the sun next rises and sets at the house, and the side it says now; `None`
    /// without a place.
    pub async fn sun_now(&self) -> Option<NextSun> {
        let place = self.place().await?;
        Some(sun::next(&place, chrono::Utc::now()).api())
    }

    pub async fn set_place(&self, place: Place) -> Result<Place, AppError> {
        if !(-90.0..=90.0).contains(&place.latitude) || !(-180.0..=180.0).contains(&place.longitude)
        {
            return Err(Refusal::Invalid.into());
        }
        store::private_dir(&self.inner.state_dir)?;
        store::write_json_async(
            &place_path(&self.inner.state_dir),
            &Some(&place),
            Access::Private,
        )
        .await?;
        *self.inner.place.write().await = Some(place.clone());
        self.publish().await;
        Ok(place)
    }

    pub async fn search_places(&self, query: &str, language: &str) -> Result<Vec<Place>, AppError> {
        sun::search(&self.inner.http, &self.inner.geocoding_url, query, language).await
    }

    /// The next sunrise and sunset after `now`, at the house (none without a place).
    async fn sun_times(&self, now: Time) -> SunTimes {
        match self.place().await {
            Some(place) => sun::next(&place, now),
            None => SunTimes::default(),
        }
    }

    /// One look of the scheduler (every 30 s): sends what fell due, and retries what failed.
    /// The covers' subscriptions are kept up on the way.
    pub async fn run_schedule(&self) {
        self.follow_covers().await;
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
        let covers = self.inner.covers.get().await.0;
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
        if let Some(live) = lock(&self.inner.live).remove(&cover.node_id) {
            live.task.abort();
        }
        decommission(&controller, cover.node_id).await;
        store::private_dir(&self.inner.state_dir)?;
        self.inner
            .covers
            .update(|covers| {
                covers.0.retain(|c| c.node_id != cover.node_id);
                Ok(())
            })
            .await?;
        self.publish().await;
        Ok(())
    }

    async fn cover(&self, id: &str) -> Result<CoverConfig, AppError> {
        let node_id = parse_id(id)?;
        self.inner
            .covers
            .read(|covers| covers.get(node_id).cloned())
            .await
            .ok_or_else(|| Refusal::NotFound.into())
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
                tracing::error!(%error, "Matter controller failed to start");
                Refusal::Internal.into()
            })
    }

    // ── the subscriptions ──

    /// Follows every kept cover that is not followed yet (the controller is built for it,
    /// not at boot); a follower whose cover is gone is stopped.
    async fn follow_covers(&self) {
        let covers = self.inner.covers.get().await.0;
        let unfollowed = {
            let mut live = lock(&self.inner.live);
            live.retain(|node_id, followed| {
                let kept = covers.iter().any(|cover| cover.node_id == *node_id);
                if !kept {
                    followed.task.abort();
                }
                kept
            });
            covers
                .iter()
                .filter(|cover| !live.contains_key(&cover.node_id))
                .map(|cover| (cover.node_id, cover.endpoint))
                .collect::<Vec<_>>()
        };
        if unfollowed.is_empty() {
            return;
        }
        if let Err(error) = self.controller().await {
            tracing::debug!(%error, "matter controller unavailable: no subscription yet");
            return;
        }
        let mut live = lock(&self.inner.live);
        for (node_id, endpoint) in unfollowed {
            let (sender, reading) = watch::channel(Reading::default());
            let sender = Arc::new(sender);
            let weak = Arc::downgrade(&self.inner);
            let task = crate::every("matter cover subscription", FOLLOW_RESTART, move || {
                follow(weak.clone(), node_id, endpoint, sender.clone())
            });
            live.insert(node_id, Live { reading, task });
        }
    }

    /// Sends the covers as they are now to [`Self::subscribe`]rs, from what was kept and
    /// heard (no network), when they changed.
    async fn publish(&self) {
        let covers = self.inner.covers.get().await.0;
        let next = self.sun_times(chrono::Utc::now()).await;
        let views = covers
            .iter()
            .map(|cover| {
                let reading = self.reading(cover.node_id);
                if reading.online {
                    view_of(cover, next, &reading)
                } else {
                    offline_view(cover, next, Refusal::DeviceUnreachable)
                }
            })
            .collect::<Vec<_>>();
        self.inner.shown.send_if_modified(|shown| {
            let changed = *shown != views;
            if changed {
                *shown = views;
            }
            changed
        });
    }
}

/// One cover's follower: subscribes to its position, target and motion, and keeps what the
/// switch reports in `reading`; the controller re-subscribes by itself when the switch goes
/// quiet, this subscribes again (with a growing pause) when the subscription cannot be made
/// or ends. It holds the manager weakly: once the manager is gone, it ends.
async fn follow(
    manager: Weak<Inner>,
    node_id: u64,
    endpoint: u16,
    reading: Arc<watch::Sender<Reading>>,
) {
    let paths = cover_paths(endpoint);
    let mut attempts: u32 = 0;
    loop {
        let Some(controller) = controller_of(&manager).await else {
            return;
        };
        let subscribed = within(
            COMMAND_TIMEOUT,
            controller
                .node(node_id)
                .subscribe(&paths, &[], REPORT_MIN_S, REPORT_MAX_S),
        )
        .await;
        match subscribed {
            Ok(mut subscription) => {
                attempts = 0;
                while let Some(event) = subscription.next().await {
                    let changed = match event {
                        SubscriptionEvent::Report(report) => reading.send_if_modified(|r| {
                            let before = r.clone();
                            r.apply(report.path.attribute, &report.value);
                            *r != before
                        }),
                        SubscriptionEvent::Established { max_interval, .. } => {
                            tracing::info!(node_id = %node_text(node_id), max_interval, "matter cover subscribed");
                            reading.send_if_modified(|r| !std::mem::replace(&mut r.online, true))
                        }
                        SubscriptionEvent::Resubscribing { cause } => {
                            tracing::info!(node_id = %node_text(node_id), %cause, "matter cover subscription lost: re-subscribing");
                            reading.send_if_modified(|r| std::mem::replace(&mut r.online, false))
                        }
                        SubscriptionEvent::Lagged { dropped } => {
                            // the reports dropped are recovered by a read
                            tracing::warn!(node_id = %node_text(node_id), dropped, "matter cover reports dropped: read again");
                            match within(READ_TIMEOUT, controller.node(node_id).read(&paths)).await
                            {
                                Ok(reports) => reading.send_if_modified(|r| {
                                    let before = r.clone();
                                    for (path, value) in &reports {
                                        r.apply(path.attribute, value);
                                    }
                                    *r != before
                                }),
                                Err(error) => {
                                    tracing::warn!(node_id = %node_text(node_id), %error, "matter cover read after lost reports failed");
                                    false
                                }
                            }
                        }
                        _ => false,
                    };
                    if changed {
                        let Some(inner) = manager.upgrade() else {
                            return;
                        };
                        MatterManager { inner }.publish().await;
                    }
                }
                tracing::info!(node_id = %node_text(node_id), "matter cover subscription ended");
            }
            Err(error) => {
                attempts = attempts.saturating_add(1);
                tracing::warn!(node_id = %node_text(node_id), %error, attempts, "matter cover subscription refused");
            }
        }
        if reading.send_if_modified(|r| std::mem::replace(&mut r.online, false)) {
            if let Some(inner) = manager.upgrade() {
                MatterManager { inner }.publish().await;
            }
        }
        tokio::time::sleep(capped_backoff(
            SUBSCRIBE_RETRY,
            attempts.max(1),
            MAX_SUBSCRIBE_RETRY,
        ))
        .await;
    }
}

/// The controller, if the manager is still there (held only for the call).
async fn controller_of(manager: &Weak<Inner>) -> Option<MatterController> {
    let inner = manager.upgrade()?;
    match (MatterManager { inner }).controller().await {
        Ok(controller) => Some(controller),
        Err(error) => {
            tracing::debug!(%error, "matter controller unavailable for a subscription");
            None
        }
    }
}

/// What a cover is read and subscribed to: its position, its target, its motion.
fn cover_paths(endpoint: u16) -> [ReadPath; 3] {
    [
        ReadPath::concrete(endpoint, WINDOW_COVERING, ATTR_CURRENT_LIFT_PERCENT100THS),
        ReadPath::concrete(endpoint, WINDOW_COVERING, ATTR_TARGET_LIFT_PERCENT100THS),
        ReadPath::concrete(endpoint, WINDOW_COVERING, ATTR_OPERATIONAL_STATUS),
    ]
}

fn covers_path(state_dir: &Path) -> PathBuf {
    state_dir.join("covers.json")
}

fn place_path(state_dir: &Path) -> PathBuf {
    state_dir.join("place.json")
}

/// The attestation roots; unusable ones are our failure (`internal`: the detail, and the
/// fix, in the log).
pub(crate) fn load_trust(trust_dir: &Path) -> Result<AttestationTrust, AppError> {
    AttestationTrust::from_dirs(&trust_dir.join("paa"), &trust_dir.join("cd")).map_err(|error| {
        tracing::error!(
            trust_dir = %trust_dir.display(),
            %error,
            "Matter attestation roots unusable; run scripts/update-matter-trust.sh"
        );
        Refusal::Internal.into()
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
    let now = u64::try_from(crate::util::now_ms() / 1000).unwrap_or_default();
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
                    node_id = %node_text(node_id),
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

/// The cover read now; offline (`device_unreachable`, the detail in the log) when the
/// switch does not answer.
async fn read_cover(controller: &MatterController, cover: CoverConfig, next: SunTimes) -> Cover {
    let paths = cover_paths(cover.endpoint);
    let reports = match within(READ_TIMEOUT, controller.node(cover.node_id).read(&paths)).await {
        Ok(reports) => reports,
        Err(error) => {
            tracing::debug!(cover = %cover.name, %error, "matter cover read failed");
            return offline_view(&cover, next, Refusal::DeviceUnreachable);
        }
    };
    let mut reading = Reading {
        online: true,
        ..Reading::default()
    };
    for (path, value) in &reports {
        reading.apply(path.attribute, value);
    }
    view_of(&cover, next, &reading)
}

/// A cover as the API shows it: as kept, and what its switch said (`reading`).
fn view_of(cover: &CoverConfig, next: SunTimes, reading: &Reading) -> Cover {
    let (next_open, next_close) = cover.next_times(next);
    // what the web strikes through: the next scheduled time, when it is the skipped one
    let skipped = |event, at: Option<Time>| at.is_some_and(|at| cover.skips(event, at));
    Cover {
        id: format_id(cover.node_id),
        name: cover.name.clone(),
        endpoint: cover.endpoint,
        online: reading.online,
        open_percent: reading.open_percent,
        target_open_percent: reading.target_open_percent,
        motion: reading.motion,
        vendor_id: cover.vendor_id,
        product_id: cover.product_id,
        schedule: cover.schedule,
        next_open,
        next_close,
        skip_next_open: skipped(CoverCommand::Open, next_open),
        skip_next_close: skipped(CoverCommand::Close, next_close),
        error: None,
    }
}

/// A cover nobody could ask: as kept, `why` it is offline coded in `error`.
fn offline_view(cover: &CoverConfig, next: SunTimes, why: Refusal) -> Cover {
    Cover {
        error: Some(why.as_str().to_string()),
        ..view_of(cover, next, &Reading::default())
    }
}

/// A switch's IM statuses: any failure is the command refused (`unsupported`: the switch
/// would not; its status in the log).
fn rejected(
    cover: &CoverConfig,
    statuses: impl IntoIterator<Item = matter_controller::ImStatus>,
) -> Result<(), AppError> {
    match statuses.into_iter().find_map(|s| match s {
        matter_controller::ImStatus::Failure(code) => Some(code),
        _ => None,
    }) {
        Some(code) => {
            tracing::warn!(cover = %cover.name, status = format_args!("{code:#04x}"), "the shutter rejected the command");
            Err(Refusal::Unsupported.into())
        }
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

/// A cover's id in the API: its node id in 16 hex digits.
fn format_id(node_id: u64) -> String {
    format!("{node_id:016x}")
}

/// A node id as the logs say it.
fn node_text(node_id: u64) -> String {
    format!("{node_id:016X}")
}

fn parse_id(id: &str) -> Result<u64, AppError> {
    if id.len() != 16 {
        return Err(Refusal::NotFound.into());
    }
    u64::from_str_radix(id, 16).map_err(|_| Refusal::NotFound.into())
}

fn normalize_name(name: &str) -> Result<String, AppError> {
    crate::util::name(name).map(str::to_string)
}

/// Accept a QR payload (`MT:…`) as-is, and a manual pairing code with the
/// spaces/dashes apps print it with (`3497-011-2332` → `34970112332`); anything else is the
/// coded `invalid`.
fn normalize_setup_code(code: &str) -> Result<String, AppError> {
    let code = code.trim();
    if code.starts_with("MT:") {
        return Ok(code.to_string());
    }
    let digits: String = code.chars().filter(|c| !matches!(c, ' ' | '-')).collect();
    if (digits.len() == 11 || digits.len() == 21) && digits.bytes().all(|b| b.is_ascii_digit()) {
        Ok(digits)
    } else {
        Err(Refusal::Invalid.into())
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

/// A commissioning that failed, coded (the detail in the log): a code that does not read is
/// `invalid`, a clock NTP has not set yet `service_unreachable` (try again in a minute), a
/// device that did not answer or refused `device_unreachable`.
fn commission_error(error: CallError) -> AppError {
    use matter_controller::Error;
    tracing::warn!(%error, "matter commissioning failed");
    match error {
        CallError::Matter(Error::SetupCode(_)) => Refusal::Invalid.into(),
        CallError::Matter(Error::SystemClockUnset(_)) => Refusal::ServiceUnreachable.into(),
        CallError::NoAnswer | CallError::Matter(_) => Refusal::DeviceUnreachable.into(),
    }
}

/// A shutter that did not answer, or answered nonsense: `net::unreachable`.
fn operation_error(error: impl Into<CallError>) -> AppError {
    crate::net::unreachable("Matter shutter", error.into())
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
        let manager =
            MatterManager::with_paths(dir.path(), dir.path(), false, NO_GEOCODING).unwrap();
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
        manager
            .inner
            .covers
            .update(|covers| {
                covers.0.push(cover);
                Ok(())
            })
            .await
            .unwrap();
        *manager.inner.place.write().await = Some(paris());
        (dir, manager)
    }

    /// Nothing listens there: a test's manager never asks the internet.
    const NO_GEOCODING: &str = "http://127.0.0.1:9";

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
        let reloaded = MatterManager::with_paths(
            &manager.inner.state_dir,
            &manager.inner.state_dir,
            false,
            NO_GEOCODING,
        )
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
    #[tokio::test]
    async fn a_past_skip_is_forgotten_on_load() {
        let dir = crate::util::test_dir();
        let long_ago = chrono::Utc::now() - chrono::TimeDelta::days(2);
        let soon = chrono::Utc::now() + chrono::TimeDelta::hours(2);
        let covers = serde_json::json!([{
            "nodeId": 1, "name": "Salon", "endpoint": 1, "vendorId": null, "productId": null,
            "skipOpenAt": long_ago, "skipCloseAt": soon,
        }]);
        store::write_json(&covers_path(dir.path()), &covers, Access::Private).unwrap();
        let manager =
            MatterManager::with_paths(dir.path(), dir.path(), false, NO_GEOCODING).unwrap();
        let loaded = kept(&manager).await;
        assert_eq!(
            (loaded.skip_open_at, loaded.skip_close_at),
            (None, Some(soon))
        );
    }

    /// What a switch reports (by subscription or read) makes the cover's view; an unknown
    /// attribute or type changes nothing.
    #[test]
    fn a_reading_is_made_of_the_switch_s_reports() {
        let mut reading = Reading::default();
        reading.apply(ATTR_CURRENT_LIFT_PERCENT100THS, &Value::Uint(2_500));
        reading.apply(ATTR_TARGET_LIFT_PERCENT100THS, &Value::Uint(0));
        reading.apply(ATTR_OPERATIONAL_STATUS, &Value::Uint(0b01));
        reading.apply(ATTR_CLUSTER_REVISION, &Value::Uint(5));
        reading.apply(ATTR_CURRENT_LIFT_PERCENT100THS, &Value::Bool(true));
        assert_eq!(
            reading,
            Reading {
                online: false,
                open_percent: Some(75),
                target_open_percent: Some(100),
                motion: Some(CoverMotion::Opening),
            }
        );
    }

    /// An offline cover says why by a code, never by a library's words.
    #[tokio::test]
    async fn an_offline_cover_says_why_by_a_code() {
        let (_dir, manager) = manager_with_one_cover(BOTH).await;
        let covers = manager.list().await;
        assert_eq!(covers.len(), 1);
        assert!(!covers[0].online);
        assert_eq!(
            covers[0].error.as_deref(),
            Some("internal"),
            "no controller here"
        );
        let cover = kept(&manager).await;
        let view = offline_view(&cover, SunTimes::default(), Refusal::DeviceUnreachable);
        assert_eq!(view.error.as_deref(), Some("device_unreachable"));
        let online = view_of(
            &cover,
            SunTimes::default(),
            &Reading {
                online: true,
                ..Reading::default()
            },
        );
        assert!(online.online && online.error.is_none());
    }

    /// Whoever pushes the covers hears a change once: a rename, a schedule.
    #[tokio::test]
    async fn the_covers_are_announced_when_they_change() {
        let (_dir, manager) = manager_with_one_cover(BOTH).await;
        let mut shown = manager.subscribe();
        manager.rename(&format_id(1), "Bureau").await.unwrap();
        assert!(shown.has_changed().unwrap());
        assert_eq!(shown.borrow_and_update()[0].name, "Bureau");
        manager.rename(&format_id(1), "Bureau").await.unwrap();
        assert!(!shown.has_changed().unwrap(), "the same name is no news");
        manager
            .set_schedule(
                &format_id(1),
                SunSchedule {
                    close_at_sunset: false,
                    ..BOTH
                },
            )
            .await
            .unwrap();
        assert!(shown.borrow_and_update()[0].next_close.is_none());
    }

    #[tokio::test]
    async fn refusals_are_coded() {
        let (_dir, manager) = manager_with_one_cover(BOTH).await;
        let code = |error: AppError| match error {
            AppError::Coded { code, .. } => code,
            other => panic!("uncoded: {other}"),
        };
        assert_eq!(
            code(manager.get("00000000000000ff").await.unwrap_err()),
            "not_found"
        );
        assert_eq!(code(manager.get("nope").await.unwrap_err()), "not_found");
        let late = SunSchedule {
            sunset_offset_min: MAX_SUN_OFFSET_MIN + 1,
            ..BOTH
        };
        assert_eq!(
            code(manager.set_schedule(&format_id(1), late).await.unwrap_err()),
            "invalid"
        );
        let nowhere = Place {
            name: "x".into(),
            latitude: 91.0,
            longitude: 0.0,
        };
        assert_eq!(
            code(manager.set_place(nowhere).await.unwrap_err()),
            "invalid"
        );
        assert_eq!(
            code(manager.commission("1234", "Salon").await.unwrap_err()),
            "invalid"
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
        assert!(MatterManager::with_paths(dir.path(), dir.path(), true, NO_GEOCODING).is_err());
    }

    /// Every vendored root must parse: `from_dirs` fails as a whole on a
    /// single bad certificate, which would block all commissioning.
    #[test]
    fn vendored_attestation_roots_load() {
        let trust_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../matter-trust");
        load_trust(&trust_dir).expect("matter-trust/ must load");
    }
}
