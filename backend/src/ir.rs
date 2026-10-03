//! IR keymap: maps remote keycodes to device actions.
//!
//! The AirTies STB decodes the Ruwido remote in hardware and its `kird`
//! daemon POSTs every key event to `/api/ir/key` (authenticated with the
//! `IR_API_TOKEN` machine token). This module owns the keycode -> action
//! table: loaded from `ir-keymap.json` at startup, edited at runtime through
//! the `/api/ir` routes (the frontend configurator), persisted on every
//! change. It also keeps a small ring buffer of the last received events so
//! the configurator can capture a key by asking the user to press it.
//!
//! Keymap file format (JSON object, keycodes as string keys, one binding =
//! a label + a LIST of actions fired in order):
//! ```json
//! {
//!   "207": {
//!     "label": "OK — taichi + lumière",
//!     "actions": [
//!       { "action": "nabaztag", "command": "chor taichi" },
//!       { "action": "zigbee_power", "lamp": "17ff040901881700", "on": false }
//!     ],
//!     "repeat": false
//!   }
//! }
//! ```

use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex, RwLock};

use crate::{
    error::AppError,
    lamps::HueLampView,
    matter::CoverCommand,
    mitsubishi_ir::{self, ClimateSettings},
    routes::lamps::LampBackend,
    store::{self, Access, Corrupt},
    tv,
    zigbee::ZigbeeLampView,
    AppState,
};

const RECENT_EVENTS_CAP: usize = 50;

/// Presses waiting behind a key's running one, at most: a held autorepeat key sends a frame
/// every ~100 ms and a binding may take 20 s or more (a TV waking up), so past this the
/// presses are dropped rather than replayed for minutes.
const KEY_QUEUE: usize = 4;

/// Presses of the same key closer than this are phantoms: when IR reception
/// drops a repeat frame mid-hold, the STB driver's release timer expires and
/// the next frame arrives as a fresh press — observed as double toggles
/// ~200-600 ms apart (and later when the button is held long under marginal
/// reception). A deliberate human re-press comes later than this.
const PRESS_DEBOUNCE: Duration = Duration::from_millis(1200);

/// Navigation keys are pressed in quick, deliberate succession, so the
/// phantom filter has to be short enough not to eat real presses. Bindings
/// opt into it with `debounceMs`.
const MIN_PRESS_DEBOUNCE: Duration = Duration::from_millis(50);

/// How long a climate action waits before sending: the remote keeps emitting repeat frames
/// while its button is held, and from parts of the room those reach the AC's receiver too,
/// colliding with the blaster's frame and corrupting it.
const CLIMATE_KEY_RELEASE: Duration = Duration::from_millis(1200);

/// What a `cover` action asks of a shutter (Matter's own commands, said as the API says them).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverOrder {
    Open,
    Close,
    Stop,
    /// To the action's `position`, an open percentage (0 closed, 100 open).
    Position,
}

/// What a switch-like action does to the device: force a state, or flip
/// whatever the current state is — the remote-control default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwitchState {
    On,
    Off,
    #[default]
    Toggle,
}

impl SwitchState {
    /// Whether the device should end up on. `current` (is it on now?) is only asked for a
    /// toggle, so forcing a state costs no round trip.
    pub async fn resolve<F>(self, current: impl FnOnce() -> F) -> Result<bool, AppError>
    where
        F: Future<Output = Result<bool, AppError>>,
    {
        match self {
            Self::On => Ok(true),
            Self::Off => Ok(false),
            Self::Toggle => Ok(!current().await?),
        }
    }
}

/// The `action` values are snake_case, the fields camelCase like the rest of the API. The
/// keymap and scenes written before kept snake_case fields: each renamed one reads its old
/// name too.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum IrAction {
    Nabaztag {
        command: String,
    },
    ZigbeePower {
        lamp: String,
        #[serde(default)]
        state: SwitchState,
    },
    ZigbeeBrightness {
        lamp: String,
        brightness: u8,
    },
    /// A Hue Bluetooth lamp, as `zigbee_power` drives a Zigbee one.
    HuePower {
        lamp: String,
        #[serde(default)]
        state: SwitchState,
    },
    /// A Hue Bluetooth lamp's brightness (1–100).
    HueBrightness {
        lamp: String,
        brightness: u8,
    },
    /// A Matter shutter: opened, closed, stopped or sent to `position` (an open percentage,
    /// 0 closed, 100 open; only with `position`).
    Cover {
        cover: String,
        command: CoverOrder,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        position: Option<u8>,
    },
    BroadlinkCode {
        host: String,
        #[serde(alias = "code_id")]
        code_id: String,
    },
    MerossPower {
        device: String,
        #[serde(default)]
        state: SwitchState,
    },
    /// Powers the Philips TV through JointSPACE. `switchToBox` also routes
    /// the set to the Android box's HDMI input on power-on, since the TV
    /// otherwise comes back on whatever source it was last left on.
    TvPower {
        #[serde(default)]
        state: SwitchState,
        #[serde(default = "crate::util::default_true", alias = "switch_to_box")]
        switch_to_box: bool,
    },
    /// Sends one remote-control key to the TV.
    TvKey {
        key: crate::tv::TvKey,
    },
    /// Absolute volume on the TV, preferred over repeated volume keys.
    TvVolume {
        level: u8,
    },
    TvAmbilight {
        #[serde(default)]
        state: SwitchState,
    },
    /// Launches an app on the Android TV box, powering the television on and
    /// routing it to the box first — a remote button that lights up a dark
    /// room's screen is the whole point.
    #[serde(rename = "androidtv_app")]
    AndroidTvApp {
        package: String,
        #[serde(default = "crate::util::default_true", alias = "ensure_tv_on")]
        ensure_tv_on: bool,
    },
    /// Sends one key to the Android TV box (D-pad, media, volume).
    #[serde(rename = "androidtv_key")]
    AndroidTvKey {
        key: crate::androidtv::AndroidKey,
    },
    /// Toggles the Mitsubishi AC through the Broadlink blaster: if the last
    /// commanded state left it on, sends `state-off`; otherwise sends
    /// `onCommand` (a structured `state-…` command, e.g.
    /// `state-cool-16-fan-4-vane-swing`).
    /// Same reversibility for the AC, driven by the last commanded state
    /// (IR is one-way, so that is the best approximation available).
    ClimateToggle {
        host: String,
        #[serde(alias = "on_command")]
        on_command: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
    /// Switches the Mitsubishi AC off (`state-off`), recorded as off like any order.
    ClimateOff {
        host: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
    /// Switches the Mitsubishi AC back on with the last settings it was given (from its page,
    /// a toggle or a binding), without their sleep timer: that was for then, not now.
    ClimateOn {
        host: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
    /// Runs a saved scene's actions (`scenes.rs`). Only from a key or a test: a scene may
    /// not hold another scene.
    Scene {
        scene: String,
    },
}

/// Config-time validation so a typo'd binding fails at save, not at keypress.
pub fn validate_actions(actions: &[IrAction]) -> Result<(), String> {
    for action in actions {
        if let IrAction::AndroidTvApp { package, .. } = action {
            if crate::androidtv::validate_package(package).is_err() {
                return Err(format!(
                    "invalid Android package {package:?} (expected e.g. org.smarttube.beta)"
                ));
            }
        }
        if let IrAction::ClimateToggle { on_command, .. } = action {
            if crate::mitsubishi_ir::parse_climate_settings(on_command).is_none() {
                return Err(format!(
                    "invalid climate onCommand {on_command:?} (expected e.g. \
                     state-cool-16-fan-4-vane-swing, and not state-off)"
                ));
            }
        }
        if let IrAction::Scene { scene } = action {
            if !crate::scenes::valid_id(scene) {
                return Err(format!("invalid scene id {scene:?}"));
            }
        }
        if let IrAction::Cover { command, position, .. } = action {
            cover_command(*command, *position)?;
        }
        if let IrAction::HueBrightness { brightness, .. } = action {
            if !(1..=100).contains(brightness) {
                return Err(format!("invalid Hue brightness {brightness} (expected 1 to 100)"));
            }
        }
    }
    Ok(())
}

/// Runs every action in order and says how each went (« ok: … » / « failed: … »); a
/// failing action never stops the others. The one engine behind a remote key, the
/// configurator's test and a scene: a `scene` action runs that scene's actions here, one
/// level deep (a scene inside a scene is refused at save, and fails here if hand-written).
pub async fn run_actions(state: &AppState, actions: &[IrAction]) -> Vec<String> {
    let mut results = Vec::with_capacity(actions.len());
    for action in actions {
        match action {
            IrAction::Scene { scene } => match state.scenes.get(scene).await {
                Some(found) => {
                    for inner in &found.actions {
                        results.push(outcome(execute(state, inner).await));
                    }
                }
                None => results.push(outcome(Err(AppError::not_found(format!("Unknown scene {scene}"))))),
            },
            action => results.push(outcome(execute(state, action).await)),
        }
    }
    results
}

fn outcome(result: Result<String, AppError>) -> String {
    match result {
        Ok(message) => format!("ok: {message}"),
        Err(error) => format!("failed: {error}"),
    }
}

/// The Matter command a `cover` action stands for, or why it stands for none.
fn cover_command(order: CoverOrder, position: Option<u8>) -> Result<CoverCommand, String> {
    match (order, position) {
        (CoverOrder::Position, Some(percent @ 0..=100)) => Ok(CoverCommand::OpenPercent(percent)),
        (CoverOrder::Position, Some(percent)) => Err(format!("invalid cover position {percent} (expected 0 to 100)")),
        (CoverOrder::Position, None) => Err("a cover position needs its position (0 to 100)".to_string()),
        (_, Some(_)) => Err("only a cover position takes a position".to_string()),
        (CoverOrder::Open, None) => Ok(CoverCommand::Open),
        (CoverOrder::Close, None) => Ok(CoverCommand::Close),
        (CoverOrder::Stop, None) => Ok(CoverCommand::Stop),
    }
}

/// A lamp as the power action reads it: is it on now?
trait LampView {
    fn is_on(&self) -> bool;
}

impl LampView for ZigbeeLampView {
    fn is_on(&self) -> bool {
        self.state.is_on
    }
}

impl LampView for HueLampView {
    fn is_on(&self) -> bool {
        self.state.is_on
    }
}

/// `zigbee_power` and `hue_power`: one lamp of either family on, off or flipped.
async fn lamp_power<B>(lamps: &B, lamp: &str, switch: SwitchState) -> Result<String, AppError>
where
    B: LampBackend,
    B::View: LampView,
{
    let on = switch
        .resolve(|| async {
            let view = lamps.get(lamp).await;
            Ok(view.ok_or_else(|| AppError::not_found(format!("Unknown {} {lamp}", B::NAME)))?.is_on())
        })
        .await?;
    lamps.set_power(lamp, on).await?;
    Ok(format!("{} {lamp}: power {}", B::NAME, if on { "on" } else { "off" }))
}

/// `zigbee_brightness` and `hue_brightness`.
async fn lamp_brightness<B: LampBackend>(lamps: &B, lamp: &str, brightness: u8) -> Result<String, AppError> {
    lamps.set_brightness(lamp, brightness).await?;
    Ok(format!("{} {lamp}: brightness {brightness}", B::NAME))
}

/// The settings `climate_on` sends: the last ones given, without their sleep timer.
fn resumed(last: Option<ClimateSettings>) -> Result<ClimateSettings, AppError> {
    let last = last.ok_or_else(|| AppError::not_found("No climate settings yet: set the air conditioner once from its page"))?;
    Ok(ClimateSettings { stop_in_minutes: None, ..last })
}

/// Sends one Mitsubishi command once the remote's own frames are over; the Broadlink
/// manager records the AC's new state (on with these settings, or off).
async fn climate(state: &AppState, host: &str, command: String, model: &Option<String>) -> Result<String, AppError> {
    tokio::time::sleep(CLIMATE_KEY_RELEASE).await;
    let sent = state.broadlink.send_mitsubishi_command(host.to_string(), None, command, model.clone()).await?;
    Ok(format!("Climate {host}: {}", sent.command.unwrap_or_default()))
}

/// One device action.
async fn execute(state: &AppState, action: &IrAction) -> Result<String, AppError> {
    match action {
        IrAction::Nabaztag { command } => {
            state.nabaztag.send_command(command).await?;
            Ok(format!("Nabaztag: {command}"))
        }
        IrAction::ZigbeePower { lamp, state: switch } => lamp_power(&state.zigbee, lamp, *switch).await,
        IrAction::ZigbeeBrightness { lamp, brightness } => lamp_brightness(&state.zigbee, lamp, *brightness).await,
        IrAction::HuePower { lamp, state: switch } => lamp_power(&state.hue, lamp, *switch).await,
        IrAction::HueBrightness { lamp, brightness } => lamp_brightness(&state.hue, lamp, *brightness).await,
        IrAction::Cover { cover, command, position } => {
            let order = cover_command(*command, *position).map_err(AppError::bad_request)?;
            state.matter.command(cover, order).await?;
            let done = match order {
                CoverCommand::Open => "open".to_string(),
                CoverCommand::Close => "close".to_string(),
                CoverCommand::Stop => "stop".to_string(),
                CoverCommand::OpenPercent(percent) => format!("open {percent} %"),
            };
            Ok(format!("Cover {cover}: {done}"))
        }
        IrAction::BroadlinkCode { host, code_id } => {
            state
                .broadlink
                .send_saved_code(host.clone(), None, code_id.clone())
                .await?;
            Ok(format!("Broadlink {host}: {code_id}"))
        }
        IrAction::MerossPower { device, state: switch } => {
            let on = switch.resolve(|| state.meross.is_on(device)).await?;
            state.meross.toggle(device, on).await?;
            Ok(format!("Meross {device}: {}", if on { "on" } else { "off" }))
        }
        IrAction::TvPower {
            state: switch,
            switch_to_box,
        } => {
            let power = tv::tv_power(state, *switch, *switch_to_box).await?;
            Ok(format!("TV: {power:?}"))
        }
        IrAction::TvKey { key } => {
            state.tv.send_key(*key).await?;
            Ok(format!("TV key: {key:?}"))
        }
        IrAction::TvVolume { level } => {
            let volume = state.tv.set_volume(*level, None).await?;
            Ok(format!("TV volume: {}", volume.current))
        }
        IrAction::TvAmbilight { state: switch } => {
            let on = switch.resolve(|| async { Ok(state.tv.ambilight().await?.power) }).await?;
            state.tv.set_ambilight_power(on).await?;
            Ok(format!("TV Ambilight: {}", if on { "on" } else { "off" }))
        }
        IrAction::AndroidTvApp {
            package,
            ensure_tv_on,
        } => {
            crate::androidtv::launch_with_tv(state, package, *ensure_tv_on).await?;
            Ok(format!("Android TV: launched {package}"))
        }
        IrAction::AndroidTvKey { key } => {
            state.androidtv.send_key(*key).await?;
            Ok(format!("Android TV key: {key:?}"))
        }
        IrAction::ClimateToggle {
            host,
            on_command,
            model,
        } => {
            // IR is one-way: the stored state is the last commanded one.
            let is_on = state
                .broadlink
                .climate_state()
                .await
                .map(|s| s.power)
                .unwrap_or(false);
            let command = if is_on { mitsubishi_ir::OFF_COMMAND } else { on_command.as_str() };
            climate(state, host, command.to_string(), model).await
        }
        IrAction::ClimateOff { host, model } => climate(state, host, mitsubishi_ir::OFF_COMMAND.to_string(), model).await,
        IrAction::ClimateOn { host, model } => {
            let settings = resumed(state.broadlink.climate_state().await.and_then(|s| s.settings))?;
            climate(state, host, settings.command().map_err(AppError::bad_request)?, model).await
        }
        IrAction::Scene { scene } => Err(AppError::bad_request(format!("Scene {scene} is inside a scene"))),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IrBinding {
    /// Fired in order; one failing action does not stop the others.
    pub actions: Vec<IrAction>,
    /// Optional label shown in the configurator (e.g. "OK button").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Also fire on kernel autorepeat events (value == 2) — for held keys
    /// like dim up/down. Presses (value == 1) always fire; releases never do.
    #[serde(default)]
    pub repeat: bool,
    /// Phantom-double window for this key, in milliseconds. Defaults to
    /// [`PRESS_DEBOUNCE`], which is right for toggles; navigation keys want
    /// something far shorter so quick repeated presses get through.
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "debounce_ms")]
    pub debounce_ms: Option<u64>,
}

impl IrBinding {
    /// Clamped so a zero or a typo cannot disable the phantom filter outright.
    fn debounce(&self) -> Duration {
        match self.debounce_ms {
            Some(ms) => Duration::from_millis(ms).max(MIN_PRESS_DEBOUNCE),
            None => PRESS_DEBOUNCE,
        }
    }
}

/// One received key event, kept for the configurator's capture flow.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IrEventLog {
    /// Increases by one with each event (from 1 at each start): what the configurator's
    /// capture compares, never the clock (the Pi's may step back when NTP syncs).
    pub seq: u64,
    pub code: u16,
    pub value: i32,
    pub mapped: bool,
    pub received_at: DateTime<Utc>,
}

/// Work queued for one key, run after the key's earlier presses.
type KeyJob = BoxFuture<'static, ()>;

#[derive(Clone)]
pub struct IrManager {
    path: Arc<PathBuf>,
    keymap: Arc<RwLock<HashMap<u16, IrBinding>>>,
    recent: Arc<Mutex<VecDeque<IrEventLog>>>,
    /// The last event's `seq`.
    seq: Arc<std::sync::atomic::AtomicU64>,
    last_press: Arc<Mutex<HashMap<u16, Instant>>>,
    /// One bounded queue per key, each drained by its own task, in order.
    queues: Arc<std::sync::Mutex<HashMap<u16, mpsc::Sender<KeyJob>>>>,
}

impl IrManager {
    /// Missing file = empty keymap (bindings are created from the UI).
    /// A present but invalid file is a startup error: a corrupt keymap
    /// should not silently disable the remote.
    pub fn new(path: &Path) -> Result<Self, AppError> {
        let raw: HashMap<String, IrBinding> = store::read_json(path, Corrupt::Fail)?;
        let keymap = numeric_keys(raw).map_err(|error| {
            AppError::http(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Invalid IR keymap {}: {error}", path.display()),
            )
        })?;
        Ok(Self {
            path: Arc::new(path.to_path_buf()),
            keymap: Arc::new(RwLock::new(keymap)),
            recent: Arc::new(Mutex::new(VecDeque::with_capacity(RECENT_EVENTS_CAP))),
            seq: Arc::default(),
            last_press: Arc::new(Mutex::new(HashMap::new())),
            queues: Arc::default(),
        })
    }

    /// Runs `job` in the background, after every job queued earlier for the same key: a
    /// binding's actions finish even when the caller hangs up, and two presses of one
    /// key never interleave. Other keys run alongside. `false`: dropped, the key already
    /// has [`KEY_QUEUE`] presses waiting.
    pub fn run_in_order(&self, code: u16, job: impl Future<Output = ()> + Send + 'static) -> bool {
        let mut queues = self.queues.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut job: KeyJob = Box::pin(job);
        if let Some(queue) = queues.get(&code) {
            match queue.try_send(job) {
                Ok(()) => return true,
                Err(mpsc::error::TrySendError::Full(_)) => {
                    tracing::warn!(code, "IR key busy: press dropped");
                    return false;
                }
                // the key's task is gone (a job panicked): start a new one
                Err(mpsc::error::TrySendError::Closed(back)) => job = back,
            }
        }
        let (queue, mut jobs) = mpsc::channel::<KeyJob>(KEY_QUEUE);
        let _ = queue.try_send(job);
        queues.insert(code, queue);
        tokio::spawn(async move {
            while let Some(job) = jobs.recv().await {
                job.await;
            }
        });
        true
    }

    /// Returns `false` when this press is a phantom double (see
    /// [`PRESS_DEBOUNCE`]); an accepted press starts the next window.
    /// Autorepeat events are not debounced — they are the point of `repeat`.
    ///
    /// The window is per binding: 1.2 s suits a toggle, where a phantom
    /// double cancels the action outright, but it would swallow the second
    /// press of a D-pad being used to navigate.
    pub async fn accept_press(&self, code: u16) -> bool {
        let window = match self.keymap.read().await.get(&code) {
            Some(binding) => binding.debounce(),
            None => PRESS_DEBOUNCE,
        };
        let mut map = self.last_press.lock().await;
        let now = Instant::now();
        match map.get(&code) {
            Some(previous) if now.duration_since(*previous) < window => false,
            _ => {
                map.insert(code, now);
                true
            }
        }
    }

    pub async fn binding(&self, code: u16) -> Option<IrBinding> {
        self.keymap.read().await.get(&code).cloned()
    }

    pub async fn keymap(&self) -> HashMap<u16, IrBinding> {
        self.keymap.read().await.clone()
    }

    /// Saved first, then kept: a failed save leaves memory as it is on disk.
    pub async fn set_binding(&self, code: u16, binding: IrBinding) -> Result<(), AppError> {
        let mut map = self.keymap.write().await;
        let mut next = map.clone();
        next.insert(code, binding);
        self.persist(&next).await?;
        *map = next;
        Ok(())
    }

    /// Returns `true` when a binding existed and was removed.
    pub async fn remove_binding(&self, code: u16) -> Result<bool, AppError> {
        let mut map = self.keymap.write().await;
        if !map.contains_key(&code) {
            return Ok(false);
        }
        let mut next = map.clone();
        next.remove(&code);
        self.persist(&next).await?;
        *map = next;
        Ok(true)
    }

    pub async fn record_event(&self, code: u16, value: i32, mapped: bool) {
        let mut recent = self.recent.lock().await;
        if recent.len() == RECENT_EVENTS_CAP {
            recent.pop_front();
        }
        recent.push_back(IrEventLog {
            seq: self.seq.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1,
            code,
            value,
            mapped,
            received_at: Utc::now(),
        });
    }

    /// Most recent first.
    pub async fn recent_events(&self) -> Vec<IrEventLog> {
        self.recent.lock().await.iter().rev().cloned().collect()
    }

    async fn persist(&self, map: &HashMap<u16, IrBinding>) -> Result<(), AppError> {
        // String keys so the file round-trips through parse_keymap.
        let as_strings: HashMap<String, &IrBinding> = map
            .iter()
            .map(|(code, binding)| (code.to_string(), binding))
            .collect();
        store::write_json_async(&self.path, &as_strings, Access::Shared).await
    }
}

/// A binding as the configurator reads it: as kept, plus each climate toggle's `settings`
/// (its `onCommand` read back), so the web needs no parser of its own.
pub fn described(binding: &IrBinding) -> serde_json::Value {
    let mut value = serde_json::to_value(binding).unwrap_or_default();
    let actions = value.get_mut("actions").and_then(serde_json::Value::as_array_mut);
    for (json, action) in actions.into_iter().flatten().zip(&binding.actions) {
        if let IrAction::ClimateToggle { on_command, .. } = action {
            let settings = crate::mitsubishi_ir::parse_climate_settings(on_command);
            json["settings"] = serde_json::to_value(settings).unwrap_or_default();
        }
    }
    value
}

/// A keymap file's text (the `keymap_file_check` test checks a hand-written one).
pub fn parse_keymap(content: &str) -> Result<HashMap<u16, IrBinding>, String> {
    numeric_keys(serde_json::from_str(content.trim()).map_err(|error| error.to_string())?)
}

/// JSON keys are strings; keycodes are numbers.
fn numeric_keys(raw: HashMap<String, IrBinding>) -> Result<HashMap<u16, IrBinding>, String> {
    raw.into_iter()
        .map(|(key, binding)| {
            key.parse::<u16>()
                .map(|code| (code, binding))
                .map_err(|_| format!("keycode {key:?} is not a u16"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn events_are_numbered_in_order_whatever_the_clock_says() {
        let dir = crate::util::test_dir();
        let manager = IrManager::new(&dir.path().join("ir-keymap.json")).unwrap();
        for code in [10, 11, 12] {
            manager.record_event(code, 1, false).await;
        }
        let seqs: Vec<(u64, u16)> = manager.recent_events().await.iter().map(|e| (e.seq, e.code)).collect();
        assert_eq!(seqs, [(3, 12), (2, 11), (1, 10)], "newest first, one more each time");
    }

    #[test]
    fn parses_all_action_kinds_and_multi_action_bindings() {
        let map = parse_keymap(
            r#"{
                "207": { "label": "OK", "actions": [
                    { "action": "nabaztag", "command": "chor taichi" },
                    { "action": "zigbee_power", "lamp": "abc" }
                ]},
                "115": { "actions": [
                    { "action": "zigbee_brightness", "lamp": "abc", "brightness": 128 }
                ], "repeat": true },
                "2": { "actions": [
                    { "action": "broadlink_code", "host": "192.168.1.2", "code_id": "tv-power" },
                    { "action": "meross_power", "device": "192.168.1.3", "state": "off" }
                ]}
            }"#,
        )
        .expect("valid keymap");
        assert_eq!(map.len(), 3);
        assert_eq!(map[&207].actions.len(), 2);
        assert_eq!(
            map[&207].actions[0],
            IrAction::Nabaztag {
                command: "chor taichi".to_string()
            }
        );
        assert_eq!(
            map[&207].actions[1],
            IrAction::ZigbeePower {
                lamp: "abc".to_string(),
                state: SwitchState::Toggle
            },
            "omitted state defaults to toggle"
        );
        assert_eq!(
            map[&2].actions[1],
            IrAction::MerossPower {
                device: "192.168.1.3".to_string(),
                state: SwitchState::Off
            }
        );
        assert_eq!(map[&207].label.as_deref(), Some("OK"));
        assert!(!map[&207].repeat, "repeat defaults to false");
        assert!(map[&115].repeat);
    }

    #[test]
    fn rejects_non_numeric_keycode() {
        let error = parse_keymap(
            r#"{ "power": { "actions": [{ "action": "nabaztag", "command": "ping" }] } }"#,
        )
        .expect_err("keycode must be numeric");
        assert!(error.contains("power"));
    }

    #[test]
    fn parses_and_validates_climate_toggle() {
        let map = parse_keymap(
            r#"{ "353": { "actions": [{ "action": "climate_toggle",
                "host": "192.168.1.2", "on_command": "state-cool-16-fan-4-vane-swing" }] } }"#,
        )
        .expect("valid climate binding");
        assert!(validate_actions(&map[&353].actions).is_ok());

        // state-off as on_command makes the toggle a no-op: refused at save.
        let off = vec![IrAction::ClimateToggle {
            host: "h".to_string(),
            on_command: mitsubishi_ir::OFF_COMMAND.to_string(),
            model: None,
        }];
        assert!(validate_actions(&off).is_err());

        let garbage = vec![IrAction::ClimateToggle {
            host: "h".to_string(),
            on_command: "state-cool-99-fan-4-vane-swing".to_string(),
            model: None,
        }];
        assert!(validate_actions(&garbage).is_err(), "temp out of range");
    }

    /// The TV bindings are the ones a user hand-writes most often, so pin
    /// their JSON shape: camelCase fields, snake_case action and key values.
    #[test]
    fn parses_tv_actions() {
        let keymap = parse_keymap(
            r#"{
                "9": { "actions": [
                    { "action": "tv_power", "state": "toggle", "switchToBox": true },
                    { "action": "tv_key", "key": "play_pause" },
                    { "action": "tv_volume", "level": 22 },
                    { "action": "tv_ambilight", "state": "off" }
                ] }
            }"#,
        )
        .expect("TV actions should parse");

        let actions = &keymap.get(&9).expect("binding 9").actions;
        assert_eq!(
            actions[0],
            IrAction::TvPower {
                state: SwitchState::Toggle,
                switch_to_box: true,
            }
        );
        assert_eq!(
            actions[1],
            IrAction::TvKey {
                key: crate::tv::TvKey::PlayPause,
            }
        );
        assert_eq!(actions[2], IrAction::TvVolume { level: 22 });
        assert_eq!(
            actions[3],
            IrAction::TvAmbilight {
                state: SwitchState::Off,
            }
        );
    }

    /// Switching to the box input is the sane default: the set otherwise
    /// wakes on whatever source it was last left on.
    #[test]
    fn tv_power_defaults_to_switching_to_the_box() {
        let keymap =
            parse_keymap(r#"{ "9": { "actions": [{ "action": "tv_power" }] } }"#).expect("parses");
        assert_eq!(
            keymap.get(&9).expect("binding").actions[0],
            IrAction::TvPower {
                state: SwitchState::Toggle,
                switch_to_box: true,
            }
        );
    }

    /// The keymap (and scenes) written before the API went camelCase keep loading: every
    /// renamed field reads its old snake_case name too — asserted with the NON-default
    /// value, or the test would pass on the default alone.
    #[test]
    fn both_field_spellings_are_honoured() {
        for body in [
            r#"{ "9": { "actions": [{ "action": "tv_power", "switchToBox": false }] } }"#,
            r#"{ "9": { "actions": [{ "action": "tv_power", "switch_to_box": false }] } }"#,
        ] {
            let keymap = parse_keymap(body).expect("parses");
            assert_eq!(
                keymap.get(&9).expect("binding").actions[0],
                IrAction::TvPower {
                    state: SwitchState::Toggle,
                    switch_to_box: false,
                },
                "spelling was ignored in {body}"
            );
        }

        for body in [
            r#"{ "9": { "actions": [{ "action": "androidtv_app", "package": "a.b", "ensureTvOn": false }] } }"#,
            r#"{ "9": { "actions": [{ "action": "androidtv_app", "package": "a.b", "ensure_tv_on": false }] } }"#,
        ] {
            let keymap = parse_keymap(body).expect("parses");
            assert_eq!(
                keymap.get(&9).expect("binding").actions[0],
                IrAction::AndroidTvApp {
                    package: "a.b".to_string(),
                    ensure_tv_on: false,
                },
                "spelling was ignored in {body}"
            );
        }
    }

    /// What « Je pars » holds: shutters, Hue lamps and the AC, pinned in their JSON shape.
    #[test]
    fn parses_cover_hue_and_climate_actions() {
        let keymap = parse_keymap(
            r#"{ "5": { "actions": [
                { "action": "cover", "cover": "12", "command": "close" },
                { "action": "cover", "cover": "12", "command": "position", "position": 40 },
                { "action": "hue_power", "lamp": "aa", "state": "off" },
                { "action": "hue_power", "lamp": "aa" },
                { "action": "hue_brightness", "lamp": "aa", "brightness": 30 },
                { "action": "climate_off", "host": "192.168.1.60" },
                { "action": "climate_on", "host": "192.168.1.60", "model": "msz" }
            ] } }"#,
        )
        .expect("parses");
        let actions = &keymap[&5].actions;
        assert_eq!(actions[0], IrAction::Cover { cover: "12".into(), command: CoverOrder::Close, position: None });
        assert_eq!(actions[1], IrAction::Cover { cover: "12".into(), command: CoverOrder::Position, position: Some(40) });
        assert_eq!(actions[2], IrAction::HuePower { lamp: "aa".into(), state: SwitchState::Off });
        assert_eq!(actions[3], IrAction::HuePower { lamp: "aa".into(), state: SwitchState::Toggle }, "toggle by default");
        assert_eq!(actions[4], IrAction::HueBrightness { lamp: "aa".into(), brightness: 30 });
        assert_eq!(actions[5], IrAction::ClimateOff { host: "192.168.1.60".into(), model: None });
        assert_eq!(actions[6], IrAction::ClimateOn { host: "192.168.1.60".into(), model: Some("msz".into()) });
        assert!(validate_actions(actions).is_ok());
        let json = serde_json::to_value(&actions[0]).unwrap();
        assert_eq!(json, serde_json::json!({ "action": "cover", "cover": "12", "command": "close" }), "no null position");
    }

    #[test]
    fn a_cover_position_goes_with_its_command_only() {
        assert_eq!(cover_command(CoverOrder::Open, None), Ok(CoverCommand::Open));
        assert_eq!(cover_command(CoverOrder::Close, None), Ok(CoverCommand::Close));
        assert_eq!(cover_command(CoverOrder::Stop, None), Ok(CoverCommand::Stop));
        assert_eq!(cover_command(CoverOrder::Position, Some(0)), Ok(CoverCommand::OpenPercent(0)));
        assert_eq!(cover_command(CoverOrder::Position, Some(100)), Ok(CoverCommand::OpenPercent(100)));
        assert!(cover_command(CoverOrder::Position, Some(101)).is_err());
        assert!(cover_command(CoverOrder::Position, None).is_err());
        assert!(cover_command(CoverOrder::Close, Some(30)).is_err());
        let wrong = |position| vec![IrAction::Cover { cover: "1".into(), command: CoverOrder::Position, position }];
        assert!(validate_actions(&wrong(Some(101))).is_err(), "refused at save");
        assert!(validate_actions(&wrong(None)).is_err());
        let hue = |brightness| vec![IrAction::HueBrightness { lamp: "aa".into(), brightness }];
        assert!(validate_actions(&hue(0)).is_err() && validate_actions(&hue(101)).is_err());
        assert!(validate_actions(&hue(1)).is_ok() && validate_actions(&hue(100)).is_ok());
    }

    /// The AC comes back as it was last set, but a sleep timer armed then is not armed again.
    #[test]
    fn climate_on_resumes_the_last_settings_without_their_timer() {
        let last = mitsubishi_ir::parse_climate_settings("state-heat-21-fan-auto-vane-auto-stopin-60").unwrap();
        let again = resumed(Some(last.clone())).expect("settings");
        assert_eq!(again, ClimateSettings { stop_in_minutes: None, ..last });
        assert_eq!(again.command().unwrap(), "state-heat-21-fan-auto-vane-auto-wide-center");
        assert!(
            matches!(resumed(None), Err(AppError::Http { status: axum::http::StatusCode::NOT_FOUND, .. })),
            "nothing to resume: said so"
        );
    }

    #[test]
    fn rejects_unknown_action() {
        assert!(
            parse_keymap(r#"{ "1": { "actions": [{ "action": "teleport", "target": "moon" }] } }"#)
                .is_err()
        );
    }

    /// A short window is what makes the D-pad usable; the clamp is what
    /// stops a typo from disabling the phantom filter that protects toggles.
    #[test]
    fn debounce_is_per_binding_and_clamped() {
        let keymap = parse_keymap(
            r#"{
                "103": { "actions": [{ "action": "androidtv_key", "key": "up" }], "debounce_ms": 150 },
                "104": { "actions": [{ "action": "androidtv_key", "key": "up" }], "debounce_ms": 0 },
                "116": { "actions": [{ "action": "nabaztag", "command": "dance" }] }
            }"#,
        )
        .expect("parses");

        assert_eq!(
            keymap.get(&103).expect("binding").debounce(),
            Duration::from_millis(150)
        );
        // Zero is clamped, never honoured literally.
        assert_eq!(
            keymap.get(&104).expect("binding").debounce(),
            MIN_PRESS_DEBOUNCE
        );
        // Toggles keep the long window that protects them.
        assert_eq!(
            keymap.get(&116).expect("binding").debounce(),
            PRESS_DEBOUNCE
        );
    }

    #[tokio::test]
    async fn debounces_phantom_double_press_per_key() {
        let dir = crate::util::test_dir();
        let manager = IrManager::new(&dir.path().join("ir-keymap.json")).expect("empty manager");
        assert!(manager.accept_press(116).await, "first press fires");
        assert!(
            !manager.accept_press(116).await,
            "immediate second press is a phantom"
        );
        assert!(manager.accept_press(117).await, "other keys are independent");
    }

    #[tokio::test]
    async fn resolve_asks_the_current_state_only_to_toggle() {
        let never = || async { panic!("not asked for a forced state") };
        assert!(SwitchState::On.resolve(never).await.unwrap());
        assert!(!SwitchState::Off.resolve(never).await.unwrap());
        assert!(!SwitchState::Toggle.resolve(|| async { Ok(true) }).await.unwrap());
        assert!(SwitchState::Toggle.resolve(|| async { Ok(false) }).await.unwrap());
        assert!(SwitchState::Toggle.resolve(|| async { Err(AppError::not_found("x")) }).await.is_err());
    }

    /// A failed save must not leave a binding that exists in memory only (and vanishes on
    /// the next restart).
    #[tokio::test]
    async fn a_failed_save_changes_nothing() {
        let dir = crate::util::test_dir();
        let path = dir.path().join("ir-keymap.json");
        let manager = IrManager::new(&path).expect("empty manager");
        std::fs::create_dir(&path).expect("a directory where the file goes");
        let binding = IrBinding {
            actions: vec![IrAction::Nabaztag { command: "ping".into() }],
            label: None,
            repeat: false,
            debounce_ms: None,
        };
        assert!(manager.set_binding(1, binding).await.is_err());
        assert!(manager.binding(1).await.is_none());
    }

    /// Presses of one key run in arrival order even when an earlier one is slower; other
    /// keys do not wait for them.
    #[tokio::test]
    async fn jobs_run_in_order_per_key() {
        let dir = crate::util::test_dir();
        let manager = IrManager::new(&dir.path().join("ir-keymap.json")).expect("manager");
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (done, mut finished) = mpsc::unbounded_channel();
        for (code, label, wait_ms) in [(1, "1a", 80), (1, "1b", 0), (2, "2a", 0), (1, "1c", 10)] {
            let (log, done) = (log.clone(), done.clone());
            manager.run_in_order(code, async move {
                tokio::time::sleep(Duration::from_millis(wait_ms)).await;
                log.lock().unwrap().push(label);
                let _ = done.send(());
            });
        }
        for _ in 0..4 {
            finished.recv().await.expect("every job runs");
        }
        let log = log.lock().unwrap().clone();
        assert_eq!(log.first(), Some(&"2a"), "key 2 did not wait for key 1: {log:?}");
        let key_one: Vec<_> = log.iter().filter(|l| l.starts_with('1')).copied().collect();
        assert_eq!(key_one, ["1a", "1b", "1c"]);
    }

    #[tokio::test]
    async fn set_remove_persist_roundtrip() {
        let dir = crate::util::test_dir();
        let path = dir.path().join("ir-keymap.json");

        let manager = IrManager::new(&path).expect("empty manager");
        manager
            .set_binding(
                207,
                IrBinding {
                    actions: vec![IrAction::Nabaztag {
                        command: "chor taichi".to_string(),
                    }],
                    label: Some("OK".to_string()),
                    repeat: false,
                    debounce_ms: None,
                },
            )
            .await
            .expect("binding saved");

        // A fresh manager sees the persisted binding.
        let reloaded = IrManager::new(&path).expect("reload");
        assert!(reloaded.binding(207).await.is_some());

        assert!(reloaded.remove_binding(207).await.expect("removed"));
        assert!(!reloaded.remove_binding(207).await.expect("idempotent"));
        let reloaded_again = IrManager::new(&path).expect("reload again");
        assert!(reloaded_again.binding(207).await.is_none());
    }

    /// A held key cannot pile up work: past `KEY_QUEUE` waiting presses, the next are
    /// dropped (and said so), and the key takes presses again once its queue drains.
    #[tokio::test]
    async fn a_busy_key_drops_presses_past_its_queue() {
        let dir = crate::util::test_dir();
        let manager = IrManager::new(&dir.path().join("ir-keymap.json")).expect("manager");
        let (release, gate) = tokio::sync::watch::channel(false);
        let ran = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let job = || {
            let (mut gate, ran) = (gate.clone(), ran.clone());
            async move {
                let _ = gate.wait_for(|open| *open).await;
                ran.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        };
        assert!(manager.run_in_order(1, job()), "the first runs (and blocks)");
        tokio::task::yield_now().await;
        let queued = (0..KEY_QUEUE + 3).filter(|_| manager.run_in_order(1, job())).count();
        assert_eq!(queued, KEY_QUEUE, "the rest dropped");
        assert!(manager.run_in_order(2, job()), "another key is not held up");
        release.send(true).unwrap();
        while ran.load(std::sync::atomic::Ordering::SeqCst) < KEY_QUEUE + 2 {
            tokio::task::yield_now().await;
        }
        assert!(manager.run_in_order(1, job()), "drained: presses go through again");
    }

    #[test]
    fn a_climate_toggle_is_described_with_its_settings() {
        let binding = IrBinding {
            actions: vec![
                IrAction::Nabaztag { command: "ping".into() },
                IrAction::ClimateToggle { host: "h".into(), on_command: "state-cool-16-fan-4-vane-swing".into(), model: None },
            ],
            label: None,
            repeat: false,
            debounce_ms: None,
        };
        let json = described(&binding);
        assert!(json["actions"][0].get("settings").is_none());
        let settings = &json["actions"][1]["settings"];
        let expected = crate::mitsubishi_ir::parse_climate_settings("state-cool-16-fan-4-vane-swing").unwrap();
        assert_eq!(settings, &serde_json::to_value(expected).unwrap());
        assert_eq!(json["actions"][1]["onCommand"], "state-cool-16-fan-4-vane-swing", "the rest as kept");
    }

    /// An old snake_case keymap reads whole and is written back camelCase.
    #[test]
    fn an_old_snake_case_keymap_is_written_back_camel_case() {
        let old = parse_keymap(
            r#"{ "2": { "debounce_ms": 150, "actions": [
                { "action": "broadlink_code", "host": "h", "code_id": "tv-power" },
                { "action": "climate_toggle", "host": "h", "on_command": "state-cool-16-fan-4-vane-swing" },
                { "action": "tv_power", "switch_to_box": false },
                { "action": "androidtv_app", "package": "a.b", "ensure_tv_on": false }
            ] } }"#,
        )
        .expect("the old shape parses");
        let binding = &old[&2];
        assert_eq!(binding.debounce_ms, Some(150));
        assert_eq!(binding.actions[0], IrAction::BroadlinkCode { host: "h".into(), code_id: "tv-power".into() });
        let written = serde_json::to_value(binding).unwrap();
        assert_eq!(written["debounceMs"], 150);
        assert_eq!(written["actions"][0]["codeId"], "tv-power");
        assert_eq!(written["actions"][1]["onCommand"], "state-cool-16-fan-4-vane-swing");
        assert_eq!(written["actions"][2]["switchToBox"], false);
        assert_eq!(written["actions"][3]["ensureTvOn"], false);
        assert_eq!(written["actions"][3]["action"], "androidtv_app", "values stay snake_case");
        assert_eq!(&serde_json::from_value::<IrBinding>(written).unwrap(), binding);
    }
}
