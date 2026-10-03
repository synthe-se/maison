//! Philips TV control over the JointSPACE API.
//!
//! The living-room set (55PUS6753/12) runs Saphi, not Android TV, which is
//! what makes this tractable: the API answers plain HTTP on port 1925 with
//! `pairing_type: "none"` — no pairing, no digest auth, no certificate.
//! Android-branch Philips sets would need HTTPS on 1926 plus a paired
//! credential; nothing here would apply to them.
//!
//! Two hard-won constraints shape this module, both discovered by breaking the
//! set and having to power-cycle it from the mains:
//!
//!  1. **Only whitelisted endpoints may be touched.** Saphi answers `Forbidden`
//!     or `Not Found` on the endpoints it does not implement (`/6/sources`,
//!     `/6/applications`, `/6/activities/*`, anything under `/5/`), and hitting
//!     them repeatedly kills the embedded server *persistently*: neither a
//!     standby cycle nor the API's own `Standby` brings it back, only pulling
//!     the mains for ~30 s. So the reachable surface is an enum, not a string —
//!     an unsupported path is unrepresentable rather than merely discouraged.
//!  2. **Requests are serialized and spaced.** The server is single-threaded
//!     and fragile; a burst is what killed it in the first place. Every call
//!     goes through one gate holding at least `MIN_REQUEST_GAP` between hits.
//!
//! Power is the one thing this module does *not* do over the API. The set has
//! two sleep depths — *light* standby still answers on 1925
//! (`powerstate: "Standby"`), *deep* standby drops the network stack entirely
//! — and from the deep one nothing on the network can reach it. Wake-on-LAN
//! does not work despite the SSDP `WAKEUP` header: the interface is powered
//! down, and even raw layer-2 magic packets, which bypass all IP routing, go
//! unanswered. CEC is out too, the set having left the bus. So power on and
//! off both go through infrared, whose receiver stays live at every depth and
//! keeps working when the JointSPACE server has crashed. See `philips_ir`.

use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};

use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::{
    net::TcpStream,
    sync::Mutex,
    time::{timeout, Instant},
};

use crate::{
    AppState,
    broadlink::BroadlinkManager,
    error::AppError,
    ir::SwitchState,
    json_config::{Checked, JsonConfig},
    net::{self, device_host, device_ipv4, unreachable},
    philips_ir,
    util::non_blank,
};

const API_PORT: u16 = 1925;
const HTTP_TIMEOUT: Duration = Duration::from_secs(6);
/// A TCP connect is the cheapest way to tell the sleep depths apart, and it
/// never touches the HTTP server.
const PROBE_TIMEOUT: Duration = Duration::from_millis(1_500);
/// Minimum spacing between two calls to the TV. See the module note on the
/// single-threaded server.
const MIN_REQUEST_GAP: Duration = Duration::from_millis(900);
/// How long a power-on waits for JointSPACE before reporting back.
///
/// Deliberately short. The infrared code is discrete, so the set is on whether
/// or not the API ever answers, and waiting longer refines nothing — it only
/// leaves the caller staring at a spinner. The set rejoins the network within
/// about five seconds; its API needs far longer from deep standby and, once
/// that server has crashed, never comes back at all. Measured against a
/// crashed server, the previous 45 s budget turned a five-second power-on into
/// a forty-nine-second round trip.
const POWER_CONFIRM_BUDGET: Duration = Duration::from_secs(10);
const WAKE_POLL_GAP: Duration = Duration::from_secs(3);
/// How long to let the set catch up with a power-on before reporting back.
/// It acknowledges the write well before `powerstate` reflects it.
const POWER_SETTLE: Duration = Duration::from_secs(6);
const POWER_POLL_GAP: Duration = Duration::from_millis(900);
/// DIAL on the Android box. Waking the box makes it assert CEC One Touch Play,
/// which powers the TV on *and* switches it to the box's HDMI input.
const BOX_DIAL_PORT: u16 = 8008;
/// JointSPACE answers are a few hundred bytes; anything far larger is not the set.
const MAX_ANSWER_BYTES: usize = 64 * 1024;
/// What the box wakes over DIAL when none is configured.
const DEFAULT_WAKE_APP: &str = "YouTube";

/// Every endpoint this module is allowed to touch, verified against the set.
/// Adding a variant means having verified it answers on Saphi — see the module
/// note before extending this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Endpoint {
    System,
    PowerState,
    AudioVolume,
    InputKey,
    AmbilightPower,
    AmbilightMode,
    AmbilightTopology,
    AmbilightSupportedStyles,
    AmbilightCurrentConfiguration,
}

impl Endpoint {
    fn path(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::PowerState => "powerstate",
            Self::AudioVolume => "audio/volume",
            Self::InputKey => "input/key",
            Self::AmbilightPower => "ambilight/power",
            Self::AmbilightMode => "ambilight/mode",
            Self::AmbilightTopology => "ambilight/topology",
            Self::AmbilightSupportedStyles => "ambilight/supportedstyles",
            Self::AmbilightCurrentConfiguration => "ambilight/currentconfiguration",
        }
    }
}

/// Remote-control keys accepted by `/6/input/key`. Kept as an enum for the same
/// reason as `Endpoint`: an unknown key name is a rejected request, and a
/// rejected request is a step towards a dead server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TvKey {
    Standby,
    Back,
    Home,
    Source,
    WatchTv,
    Confirm,
    CursorUp,
    CursorDown,
    CursorLeft,
    CursorRight,
    VolumeUp,
    VolumeDown,
    Mute,
    ChannelStepUp,
    ChannelStepDown,
    PlayPause,
    Pause,
    Stop,
    FastForward,
    Rewind,
    Next,
    Previous,
    Info,
    Options,
    Subtitle,
    Teletext,
    AmbilightOnOff,
}

impl TvKey {
    /// The wire name, which is CamelCase and not always the obvious casing
    /// (`WatchTV`, not `WatchTv`).
    fn wire_name(self) -> &'static str {
        match self {
            Self::Standby => "Standby",
            Self::Back => "Back",
            Self::Home => "Home",
            Self::Source => "Source",
            Self::WatchTv => "WatchTV",
            Self::Confirm => "Confirm",
            Self::CursorUp => "CursorUp",
            Self::CursorDown => "CursorDown",
            Self::CursorLeft => "CursorLeft",
            Self::CursorRight => "CursorRight",
            Self::VolumeUp => "VolumeUp",
            Self::VolumeDown => "VolumeDown",
            Self::Mute => "Mute",
            Self::ChannelStepUp => "ChannelStepUp",
            Self::ChannelStepDown => "ChannelStepDown",
            Self::PlayPause => "PlayPause",
            Self::Pause => "Pause",
            Self::Stop => "Stop",
            Self::FastForward => "FastForward",
            Self::Rewind => "Rewind",
            Self::Next => "Next",
            Self::Previous => "Previous",
            Self::Info => "Info",
            Self::Options => "Options",
            Self::Subtitle => "Subtitle",
            Self::Teletext => "Teletext",
            Self::AmbilightOnOff => "AmbilightOnOff",
        }
    }
}

/// How awake the set is. The distinction is diagnostic only now that power
/// goes over infrared, which reaches the set at either depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TvPower {
    On,
    /// Panel off, JointSPACE still answering.
    Standby,
    /// Network stack down — nothing answers on the API port at all. A
    /// *refusal* is the opposite signal: the set is up, its server is not.
    DeepStandby,
}

/// What a TCP connect to the API port reveals about the set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Probe {
    /// The server accepted the connection.
    Answering,
    /// The host answered and refused the port: powered up, JointSPACE down.
    HostUpApiDown,
    /// Nothing answered, so the network stack is down and the set is off.
    Unreachable,
}

/// Reads a failed connect. Only an outright refusal proves the host is up —
/// a set in deep standby fails *fast* too, with EHOSTUNREACH once ARP has
/// given up on it, so "returned an error quickly" is not the same as "is
/// there".
fn probe_from_error(kind: std::io::ErrorKind) -> Probe {
    match kind {
        std::io::ErrorKind::ConnectionRefused => Probe::HostUpApiDown,
        _ => Probe::Unreachable,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TvConfig {
    /// TV address, e.g. `192.168.1.52`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// Broadlink IR blaster that fronts the set, e.g. `192.168.1.73`. Without
    /// it the set cannot be powered on or off at all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ir_blaster_host: Option<String>,
    /// Android TV box address. Used to force the HDMI input via CEC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub box_host: Option<String>,
    /// DIAL app woken on the box to trigger CEC One Touch Play.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub box_wake_app: Option<String>,
}

/// Blank fields are absent; every address must be a LAN device (the set and the box are
/// reached over HTTP, the blaster over Broadlink's IPv4-only protocol) and the DIAL app a
/// plain name, since it goes into the URL path.
impl Checked for TvConfig {
    fn checked(self) -> Result<Self, AppError> {
        let host = |value: Option<String>| non_blank(value).map(|v| device_host(&v)).transpose();
        let ir_blaster_host =
            non_blank(self.ir_blaster_host).map(|v| device_ipv4(&v).map(|ip| ip.to_string())).transpose()?;
        let box_wake_app = non_blank(self.box_wake_app);
        if let Some(app) = &box_wake_app {
            crate::androidtv::validate_package(app)?;
        }
        Ok(Self { host: host(self.host)?, ir_blaster_host, box_host: host(self.box_host)?, box_wake_app })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TvVolume {
    pub current: u8,
    pub min: u8,
    pub max: u8,
    pub muted: bool,
}

impl TvVolume {
    /// Read from the set's JSON, which nothing bounds: a value past 255 saturates rather
    /// than wrapping (256 must not read as 0).
    fn from_json(value: &serde_json::Value) -> Self {
        let level = |key: &str, default: u8| {
            value
                .get(key)
                .and_then(|v| v.as_u64())
                .map_or(default, |v| u8::try_from(v).unwrap_or(u8::MAX))
        };
        Self {
            current: level("current", 0),
            min: level("min", 0),
            max: level("max", 60),
            muted: value.get("muted").and_then(|v| v.as_bool()).unwrap_or(false),
        }
    }

    /// `level` within the set's range. Never panics, even on a range the set got upside
    /// down (min above max): the max wins then.
    fn clamp(&self, level: u8) -> u8 {
        level.max(self.min).min(self.max)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TvAmbilight {
    pub power: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setting: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TvStatus {
    pub configured: bool,
    pub power: TvPower,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<TvVolume>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ambilight: Option<TvAmbilight>,
}

#[derive(Clone)]
pub struct TvManager {
    config: Arc<JsonConfig<TvConfig>>,
    client: reqwest::Client,
    /// The request gate: held across every call so requests are both
    /// serialized and spaced by at least `MIN_REQUEST_GAP`.
    gate: Arc<Mutex<Option<Instant>>>,
    /// Blaster fronting the set. Power goes through it rather than the API,
    /// infrared being the only channel that survives deep standby.
    broadlink: BroadlinkManager,
    /// RC5 toggle bit, flipped on every code sent.
    ir_toggle: Arc<AtomicBool>,
}

impl TvManager {
    pub fn new(config_path: &Path, broadlink: BroadlinkManager) -> Result<Self, AppError> {
        Ok(Self {
            config: Arc::new(JsonConfig::load(config_path)?),
            client: net::device_client(HTTP_TIMEOUT)?,
            gate: Arc::new(Mutex::new(None)),
            broadlink,
            ir_toggle: Arc::new(AtomicBool::new(false)),
        })
    }

    pub async fn config(&self) -> TvConfig {
        self.config.get().await
    }

    pub async fn set_config(&self, config: TvConfig) -> Result<TvConfig, AppError> {
        self.config.set(config).await
    }

    /// Checked where it came in (`Checked`), not again here.
    async fn host(&self) -> Result<String, AppError> {
        self.config().await.host.ok_or_else(|| AppError::service_unavailable("No TV configured"))
    }

    /// Waits out the inter-request gap, then reports the moment the caller may
    /// hit the wire. The guard is held for the duration of the request.
    async fn acquire(&self) -> tokio::sync::MutexGuard<'_, Option<Instant>> {
        let mut gate = self.gate.lock().await;
        if let Some(last) = *gate {
            let elapsed = last.elapsed();
            if elapsed < MIN_REQUEST_GAP {
                tokio::time::sleep(MIN_REQUEST_GAP - elapsed).await;
            }
        }
        *gate = Some(Instant::now());
        gate
    }

    async fn get(&self, endpoint: Endpoint) -> Result<serde_json::Value, AppError> {
        let host = self.host().await?;
        let _guard = self.acquire().await;
        let url = format!("http://{host}:{API_PORT}/6/{}", endpoint.path());
        let response = self.client.get(&url).send().await.map_err(|error| unreachable("TV", error))?;
        if !response.status().is_success() {
            return Err(AppError::service_unavailable(format!(
                "TV refused GET {} ({})",
                endpoint.path(),
                response.status()
            )));
        }
        let body = net::read_capped(response, MAX_ANSWER_BYTES, "TV").await?;
        serde_json::from_slice(&body).map_err(|error| unreachable("TV", error))
    }

    async fn post(&self, endpoint: Endpoint, body: serde_json::Value) -> Result<(), AppError> {
        let host = self.host().await?;
        let _guard = self.acquire().await;
        let url = format!("http://{host}:{API_PORT}/6/{}", endpoint.path());
        let response =
            self.client.post(&url).json(&body).send().await.map_err(|error| unreachable("TV", error))?;
        if !response.status().is_success() {
            return Err(AppError::service_unavailable(format!(
                "TV refused POST {} ({})",
                endpoint.path(),
                response.status()
            )));
        }
        Ok(())
    }

    /// Is the API port answering? A plain TCP connect, so it costs the HTTP
    /// server nothing and tells light standby from deep standby.
    /// One TCP connect, read for everything it says.
    ///
    /// Collapsing this to a bool loses the distinction that matters most on a
    /// bad day: a refused connection means the set is powered up and only its
    /// server is gone, which is a very different thing from silence.
    async fn probe(&self) -> Probe {
        let Ok(host) = self.host().await else {
            return Probe::Unreachable;
        };
        match timeout(PROBE_TIMEOUT, TcpStream::connect((host.as_str(), API_PORT))).await {
            Ok(Ok(_)) => Probe::Answering,
            Ok(Err(error)) => probe_from_error(error.kind()),
            Err(_) => Probe::Unreachable,
        }
    }

    async fn api_reachable(&self) -> bool {
        matches!(self.probe().await, Probe::Answering)
    }

    pub async fn power(&self) -> TvPower {
        match self.probe().await {
            Probe::Unreachable => return TvPower::DeepStandby,
            // Powered up with JointSPACE gone. Reporting deep standby here is
            // what left the remote's Power button able to switch the set on
            // and never off: a toggle reads this state, believes the set is
            // asleep, and switches it on again.
            Probe::HostUpApiDown => return TvPower::On,
            Probe::Answering => {}
        }
        power_from_answer(self.get(Endpoint::PowerState).await)
    }

    /// Full snapshot for the dashboard shelf. Volume and Ambilight are only
    /// read when the panel is actually on — they are meaningless otherwise and
    /// would spend gate time for nothing.
    pub async fn status(&self) -> TvStatus {
        let configured = self.config().await.host.is_some();
        if !configured {
            return TvStatus {
                configured: false,
                power: TvPower::DeepStandby,
                name: None,
                volume: None,
                ambilight: None,
            };
        }

        let power = self.power().await;
        if power != TvPower::On {
            return TvStatus {
                configured: true,
                power,
                name: None,
                volume: None,
                ambilight: None,
            };
        }

        let name = self
            .get(Endpoint::System)
            .await
            .ok()
            .and_then(|value| value.get("name")?.as_str().map(str::to_string));

        TvStatus {
            configured: true,
            power,
            name,
            volume: self.volume().await.ok(),
            ambilight: self.ambilight().await.ok(),
        }
    }

    pub async fn volume(&self) -> Result<TvVolume, AppError> {
        Ok(TvVolume::from_json(&self.get(Endpoint::AudioVolume).await?))
    }

    /// Absolute volume. Preferred over repeated `VolumeUp` keys: the key path
    /// is fire-and-forget and its effect lags the readback by about a step.
    pub async fn set_volume(&self, level: u8, muted: Option<bool>) -> Result<TvVolume, AppError> {
        let current = self.volume().await?;
        let level = current.clamp(level);
        self.post(
            Endpoint::AudioVolume,
            json!({ "muted": muted.unwrap_or(current.muted), "current": level }),
        )
        .await?;
        self.volume().await
    }

    pub async fn send_key(&self, key: TvKey) -> Result<(), AppError> {
        self.post(Endpoint::InputKey, json!({ "key": key.wire_name() }))
            .await
    }

    pub async fn ambilight(&self) -> Result<TvAmbilight, AppError> {
        let power = self
            .get(Endpoint::AmbilightPower)
            .await?
            .get("power")
            .and_then(|v| v.as_str())
            .map(|v| v.eq_ignore_ascii_case("on"))
            .unwrap_or(false);
        let mode = self
            .get(Endpoint::AmbilightMode)
            .await
            .ok()
            .and_then(|v| v.get("current")?.as_str().map(str::to_string));
        let configuration = self.get(Endpoint::AmbilightCurrentConfiguration).await.ok();

        Ok(TvAmbilight {
            power,
            mode,
            style: configuration
                .as_ref()
                .and_then(|v| v.get("styleName")?.as_str().map(str::to_string)),
            setting: configuration
                .as_ref()
                .and_then(|v| v.get("stringValue")?.as_str().map(str::to_string)),
        })
    }

    pub async fn set_ambilight_power(&self, on: bool) -> Result<(), AppError> {
        self.post(
            Endpoint::AmbilightPower,
            json!({ "power": if on { "On" } else { "Off" } }),
        )
        .await
    }

    pub async fn ambilight_styles(&self) -> Result<serde_json::Value, AppError> {
        self.get(Endpoint::AmbilightSupportedStyles).await
    }

    pub async fn ambilight_topology(&self) -> Result<serde_json::Value, AppError> {
        self.get(Endpoint::AmbilightTopology).await
    }

    /// Powers the set on, whatever depth it is sleeping at. Routing it to the box's
    /// input is the caller's next step (`power_on_and_route`), over CEC first.
    ///
    /// One channel only: infrared reaches the set at either sleep depth and
    /// keeps working when JointSPACE has crashed, which is more than the API,
    /// Wake-on-LAN or CEC can each claim. The code is discrete rather than a
    /// toggle, so firing it at a set that is already on is a no-op.
    pub async fn power_on(&self) -> Result<TvPower, AppError> {
        self.send_power_code(philips_ir::TV_POWER_ON).await?;

        // The set rejoins the network several seconds before JointSPACE starts
        // listening, and once that server has crashed it never listens again
        // until the mains are pulled. So this wait is best-effort rather than a
        // precondition: a discrete power-on code cannot have left the set
        // anywhere but on, and reporting failure because the API stayed quiet
        // would be reporting on the wrong thing.
        let reachable = self.await_api(POWER_CONFIRM_BUDGET).await;
        if !reachable {
            tracing::info!(
                "TV switched on over infrared; JointSPACE has not answered yet. \
                 If it stays silent the embedded server has crashed and needs a \
                 mains power cycle."
            );
        }

        if !reachable {
            return Ok(TvPower::On);
        }

        // The set lags its own writes: reading `powerstate` straight back
        // reports the previous value, so a successful power-on looked like it
        // had left the TV in standby. Give it a moment to catch up rather than
        // reporting a state we know to be stale.
        let deadline = Instant::now() + POWER_SETTLE;
        while Instant::now() < deadline {
            if self.power().await == TvPower::On {
                break;
            }
            tokio::time::sleep(POWER_POLL_GAP).await;
        }

        Ok(self.power().await)
    }

    /// Polls the API until it answers or the budget runs out.
    async fn await_api(&self, budget: Duration) -> bool {
        let deadline = Instant::now() + budget;
        loop {
            if self.api_reachable().await {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(WAKE_POLL_GAP).await;
        }
    }

    /// Fires one Philips RC5 power code through the configured blaster.
    async fn send_power_code(&self, command: u8) -> Result<(), AppError> {
        let host = self
            .config()
            .await
            .ir_blaster_host
            .ok_or_else(|| {
                AppError::service_unavailable("No IR blaster configured for the TV")
            })?;

        // RC5 wants the toggle bit to flip between presses; a receiver seeing
        // it unchanged reads the second frame as a held key and drops it.
        let toggle = self.ir_toggle.fetch_xor(true, Ordering::Relaxed);
        let packet = philips_ir::encode_rc5(philips_ir::TV_ADDRESS, command, toggle);

        tracing::info!(
            command = format!("0x{command:02X}"),
            %host,
            toggle,
            "sending a TV power code over infrared"
        );

        self.broadlink
            .send_packet(host, None, STANDARD.encode(&packet), None, None)
            .await
            .map(|_| ())
    }

    /// Switches the set off.
    ///
    /// Infrared again rather than the API's `Standby` key, for the symmetry as
    /// much as the reach: one channel to reason about, and it still works on
    /// the day JointSPACE is down.
    pub async fn power_off(&self) -> Result<TvPower, AppError> {
        self.send_power_code(philips_ir::TV_POWER_OFF).await?;
        Ok(TvPower::Standby)
    }

    /// Nudges the Android box awake over DIAL so it asserts CEC One Touch
    /// Play, which both powers the set and routes it to the box's HDMI input.
    ///
    /// This is what fixes "the TV came up on the wrong input": JointSPACE
    /// cannot switch sources at all on Saphi (`/6/sources` is `Forbidden`), so
    /// the input has to be driven from the HDMI side.
    pub async fn switch_to_box(&self) -> Result<(), AppError> {
        let config = self.config().await;
        let host = config
            .box_host
            .ok_or_else(|| AppError::service_unavailable("No Android TV box configured"))?;
        let app = config.box_wake_app.unwrap_or_else(|| DEFAULT_WAKE_APP.to_string());
        let url = format!("http://{host}:{BOX_DIAL_PORT}/apps/{app}");

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "text/plain; charset=utf-8")
            .body("")
            .send()
            .await
            .map_err(|error| unreachable("Android TV box", error))?;
        if !response.status().is_success() {
            return Err(AppError::service_unavailable(format!(
                "box refused the DIAL wake ({})",
                response.status()
            )));
        }
        Ok(())
    }
}

/// What the set's `powerstate` answer says, the port having taken the connection. An
/// answer that fails or cannot be read means the set is up and its server faltering, as
/// with a refused connect (`Probe::HostUpApiDown`): reporting it asleep would have a toggle
/// switch on a set that is on.
fn power_from_answer(answer: Result<serde_json::Value, AppError>) -> TvPower {
    match answer {
        Ok(value) => match value.get("powerstate").and_then(|v| v.as_str()) {
            Some("On") => TvPower::On,
            // Saphi reports "Standby"; treat anything else answered by a
            // live API as standby rather than guessing.
            _ => TvPower::Standby,
        },
        Err(_) => TvPower::On,
    }
}

/// Switches the set on or off, or flips it; on, it is routed to the box when asked. The
/// one way both the dashboard and the IR remote power the set.
pub async fn tv_power(state: &AppState, switch: SwitchState, to_box: bool) -> Result<TvPower, AppError> {
    let on = switch.resolve(|| async { Ok(state.tv.power().await == TvPower::On) }).await?;
    if on {
        power_on_and_route(state, to_box).await
    } else {
        state.tv.power_off().await
    }
}

/// Wakes the box and routes the set to its HDMI input.
///
/// Waking matters as much as the CEC: asserting One Touch Play against a
/// sleeping box turns the television on to a black screen, which then powers
/// itself back off for want of a signal. `wake()` does both, in that order.
///
/// The DIAL fallback works by *launching an app*, so it would yank the viewer
/// out of what they were watching — it is only worth it when ADB is down.
pub async fn route_to_box(state: &AppState) -> Result<(), AppError> {
    match state.androidtv.wake().await {
        Ok(()) => Ok(()),
        Err(error) => {
            tracing::debug!(%error, "CEC route failed, falling back to DIAL");
            state.tv.switch_to_box().await
        }
    }
}

/// Powers the set on, then optionally routes it to the box. The two are
/// separate steps so the input switch can go through CEC rather than the
/// app-launching DIAL path.
async fn power_on_and_route(state: &AppState, to_box: bool) -> Result<TvPower, AppError> {
    let power = state.tv.power_on().await?;
    if to_box {
        // Best-effort: the set is on either way, and the box may simply not be
        // configured.
        if let Err(error) = route_to_box(state).await {
            tracing::debug!(%error, "could not route the TV to the box");
        }
    }
    Ok(power)
}

/// Powers the set on and routes it to the box unless it is already on: launching an
/// app on a dark screen is never what the caller meant.
pub async fn ensure_on(state: &AppState) -> Result<(), AppError> {
    if state.tv.power().await == TvPower::On {
        return Ok(());
    }
    power_on_and_route(state, true).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The manager needs a blaster to hold, but no test here sends a code, so
    /// it only has to exist and point at scratch paths.
    fn test_broadlink(dir: &tempfile::TempDir) -> BroadlinkManager {
        BroadlinkManager::new(&dir.path().join("codes.json"), &dir.path().join("climate.json"))
            .expect("broadlink")
    }

    /// A deep-sleeping set fails the connect quickly, not slowly: ARP gives up
    /// after six probes and the stack answers EHOSTUNREACH. So speed of
    /// failure says nothing, and only an outright refusal proves the set is
    /// powered up with its server gone.
    #[test]
    fn only_a_refusal_means_the_host_is_up() {
        use std::io::ErrorKind;
        assert_eq!(
            probe_from_error(ErrorKind::ConnectionRefused),
            Probe::HostUpApiDown
        );
        assert_eq!(probe_from_error(ErrorKind::HostUnreachable), Probe::Unreachable);
        assert_eq!(probe_from_error(ErrorKind::NetworkUnreachable), Probe::Unreachable);
        assert_eq!(probe_from_error(ErrorKind::TimedOut), Probe::Unreachable);
    }

    #[test]
    fn a_failed_powerstate_after_a_connect_is_on() {
        assert_eq!(power_from_answer(Err(AppError::service_unavailable("TV unreachable"))), TvPower::On);
        assert_eq!(power_from_answer(Ok(json!({ "powerstate": "On" }))), TvPower::On);
        assert_eq!(power_from_answer(Ok(json!({ "powerstate": "Standby" }))), TvPower::Standby);
        assert_eq!(power_from_answer(Ok(json!({}))), TvPower::Standby);
    }

    #[tokio::test]
    async fn a_hand_edited_file_is_checked_on_load() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("tv.json");
        std::fs::write(&path, r#"{"host": "169.254.169.254"}"#).expect("written");
        assert!(TvManager::new(&path, test_broadlink(&dir)).is_err());
    }

    /// The wire spelling is not derivable from the variant name, so it is
    /// worth pinning: `WatchTV` carries a capital V.
    #[test]
    fn key_wire_names_match_the_api_spelling() {
        assert_eq!(TvKey::WatchTv.wire_name(), "WatchTV");
        assert_eq!(TvKey::Standby.wire_name(), "Standby");
        assert_eq!(TvKey::AmbilightOnOff.wire_name(), "AmbilightOnOff");
    }

    /// Guards the module's core safety property: every reachable path is one
    /// the set actually implements. A typo here is a bricked API until someone
    /// pulls the mains, so the list is asserted rather than assumed.
    #[test]
    fn endpoints_stay_within_the_verified_whitelist() {
        let allowed = [
            "system",
            "powerstate",
            "audio/volume",
            "input/key",
            "ambilight/power",
            "ambilight/mode",
            "ambilight/topology",
            "ambilight/supportedstyles",
            "ambilight/currentconfiguration",
        ];
        for endpoint in [
            Endpoint::System,
            Endpoint::PowerState,
            Endpoint::AudioVolume,
            Endpoint::InputKey,
            Endpoint::AmbilightPower,
            Endpoint::AmbilightMode,
            Endpoint::AmbilightTopology,
            Endpoint::AmbilightSupportedStyles,
            Endpoint::AmbilightCurrentConfiguration,
        ] {
            assert!(
                allowed.contains(&endpoint.path()),
                "{} is not a verified endpoint",
                endpoint.path()
            );
        }
    }

    #[tokio::test]
    async fn missing_config_file_is_an_empty_config_not_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let manager =
            TvManager::new(&dir.path().join("tv.json"), test_broadlink(&dir)).expect("manager");
        assert!(manager.config().await.host.is_none());
        assert!(!manager.status().await.configured);
    }

    #[tokio::test]
    async fn set_config_persists_and_rejects_bad_mac() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("tv.json");
        let manager = TvManager::new(&path, test_broadlink(&dir)).expect("manager");

        manager
            .set_config(TvConfig {
                host: Some("192.168.1.52".to_string()),
                ir_blaster_host: Some("192.168.1.73".to_string()),
                box_host: Some("192.168.1.153".to_string()),
                box_wake_app: None,
            })
            .await
            .expect("saved");

        let reloaded = TvManager::new(&path, test_broadlink(&dir))
            .expect("reload")
            .config()
            .await;
        assert_eq!(reloaded.host.as_deref(), Some("192.168.1.52"));
        assert_eq!(reloaded.box_host.as_deref(), Some("192.168.1.153"));

        let rejected = manager
            .set_config(TvConfig {
                host: Some("192.168.1.52".to_string()),
                ir_blaster_host: Some("nope".to_string()),
                box_host: None,
                box_wake_app: None,
            })
            .await;
        assert!(rejected.is_err());
    }

    #[tokio::test]
    async fn every_address_and_the_wake_app_are_checked() {
        let dir = tempfile::tempdir().expect("tempdir");
        let manager = TvManager::new(&dir.path().join("tv.json"), test_broadlink(&dir)).expect("manager");
        let base = TvConfig { host: Some("192.168.1.52".into()), ..TvConfig::default() };
        let bad = [
            TvConfig { host: Some("127.0.0.1".into()), ..base.clone() },
            TvConfig { host: Some("192.168.1.52/6/system#".into()), ..base.clone() },
            TvConfig { box_host: Some("169.254.169.254".into()), ..base.clone() },
            TvConfig { box_host: Some("box.example.com".into()), ..base.clone() },
            TvConfig { ir_blaster_host: Some("blaster.local".into()), ..base.clone() },
            TvConfig { box_wake_app: Some("../../admin".into()), ..base.clone() },
            TvConfig { box_wake_app: Some("YouTube?x=1".into()), ..base.clone() },
        ];
        for config in bad {
            assert!(manager.set_config(config.clone()).await.is_err(), "{config:?} accepted");
        }
        let saved = manager
            .set_config(TvConfig {
                host: Some(" tv.local ".into()),
                ir_blaster_host: Some("".into()),
                box_host: Some("192.168.1.153".into()),
                box_wake_app: Some("YouTube".into()),
            })
            .await
            .expect("saved");
        assert_eq!(saved.host.as_deref(), Some("tv.local"));
        assert_eq!(saved.ir_blaster_host, None, "blank is absent");
    }

    /// The set's JSON is not trusted for the arithmetic: a range past 255 saturates, and
    /// an upside-down one must not panic the clamp.
    #[test]
    fn volume_reads_and_clamps_without_panicking() {
        let volume = TvVolume::from_json(&json!({ "current": 12, "min": 0, "max": 256, "muted": true }));
        assert_eq!((volume.current, volume.min, volume.max, volume.muted), (12, 0, 255, true));
        assert_eq!(volume.clamp(200), 200);

        let upside_down = TvVolume::from_json(&json!({ "current": 5, "min": 60, "max": 10 }));
        assert_eq!(upside_down.clamp(30), 10);
        assert_eq!(upside_down.clamp(0), 10);

        let usual = TvVolume::from_json(&json!({ "current": 5, "min": 0, "max": 60 }));
        assert_eq!(usual.clamp(80), 60);
        assert_eq!(usual.clamp(22), 22);
        let missing = TvVolume::from_json(&json!({}));
        assert_eq!((missing.min, missing.max), (0, 60));
    }

    /// The gate is the other half of the safety story: bursts are what killed
    /// the server, so two consecutive acquisitions must be spaced.
    #[tokio::test]
    async fn request_gate_spaces_consecutive_calls() {
        let dir = tempfile::tempdir().expect("tempdir");
        let manager =
            TvManager::new(&dir.path().join("tv.json"), test_broadlink(&dir)).expect("manager");

        let start = Instant::now();
        drop(manager.acquire().await);
        drop(manager.acquire().await);
        assert!(
            start.elapsed() >= MIN_REQUEST_GAP,
            "second call was not spaced (elapsed {:?})",
            start.elapsed()
        );
    }
}
