//! Android TV box control: Remote v2 first (`atvremote.rs`: keys, app launches, the box's
//! power as it says it), ADB for the rest and whenever the box is not paired.
//!
//! The living-room box (MECOOL LEAP-S1, Android 14) has network debugging
//! enabled, and `adb.rs` speaks the protocol natively — so this module is
//! mostly a vocabulary: remote keys, app launching, power, the CEC lever
//! that routes the television to the box's own HDMI input, and DIAL, the other
//! way to wake the box (`dial_wake`).
//!
//! Two design notes:
//!
//!  - **Commands are never assembled from free-form input.** Keys are an enum
//!    and package names are validated, so nothing a client sends can turn into
//!    arbitrary shell.
//!  - **The connection is kept and reused.** The handshake costs an RSA
//!    signature, which is cheap on a laptop and distinctly not on an ARMv6 Pi;
//!    a dropped connection is re-established once, transparently.
//!
//! The signing key is generated on first use and persisted. Generating 2048
//! bits takes tens of seconds on a Pi 1, so it happens off the async runtime,
//! and the box will show one "Allow USB debugging?" prompt the first time the
//! backend talks to it.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use axum::body::Bytes;
use futures::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::{watch, Mutex, OwnedSemaphorePermit, Semaphore};

use crate::{
    adb::{self, AdbDevice, AdbKey},
    atvremote::{Identity, Pairing, Session},
    error::{AppError, Refusal},
    json_config::{Checked, JsonConfig},
    net::{self, device_host},
    store,
    util::{non_blank, refused},
    AppState,
};

const DEFAULT_ADB_PORT: u16 = 5555;
/// DIAL on the box. Waking an app over it makes the box assert CEC One Touch Play, which
/// powers the TV on *and* switches it to the box's HDMI input.
const DIAL_PORT: u16 = 8008;
const DIAL_TIMEOUT: Duration = Duration::from_secs(6);
/// What DIAL wakes on the box when none is configured.
const DEFAULT_WAKE_APP: &str = "YouTube";
/// After a failed Remote v2 connect, how long to stay on ADB before trying again: an
/// unpaired box would otherwise cost an RSA handshake on every status poll.
const REMOTE_RETRY_AFTER: Duration = Duration::from_secs(60);

/// Remote-control keys, mapped to Android keycodes: an enum, so no caller can smuggle shell
/// syntax into `input keyevent`.
pub use maison_api::remote::AndroidKey;

/// A key's spellings.
trait Codes {
    fn codes(self) -> (&'static str, i64);
    fn adb_command(self) -> String;
}

impl Codes for AndroidKey {
    /// The key's two spellings, said once: the symbolic name ADB's `input keyevent`
    /// takes, and the numeric Android keycode the Remote v2 protocol carries. One table,
    /// so a button cannot behave differently depending on whether the box is paired.
    fn codes(self) -> (&'static str, i64) {
        match self {
            Self::Home => ("KEYCODE_HOME", 3),
            Self::Back => ("KEYCODE_BACK", 4),
            Self::Up => ("KEYCODE_DPAD_UP", 19),
            Self::Down => ("KEYCODE_DPAD_DOWN", 20),
            Self::Left => ("KEYCODE_DPAD_LEFT", 21),
            Self::Right => ("KEYCODE_DPAD_RIGHT", 22),
            Self::Ok => ("KEYCODE_DPAD_CENTER", 23),
            Self::VolumeUp => ("KEYCODE_VOLUME_UP", 24),
            Self::VolumeDown => ("KEYCODE_VOLUME_DOWN", 25),
            Self::Power => ("KEYCODE_POWER", 26),
            Self::Menu => ("KEYCODE_MENU", 82),
            Self::Search => ("KEYCODE_SEARCH", 84),
            Self::PlayPause => ("KEYCODE_MEDIA_PLAY_PAUSE", 85),
            Self::Stop => ("KEYCODE_MEDIA_STOP", 86),
            Self::Next => ("KEYCODE_MEDIA_NEXT", 87),
            Self::Previous => ("KEYCODE_MEDIA_PREVIOUS", 88),
            Self::Rewind => ("KEYCODE_MEDIA_REWIND", 89),
            Self::FastForward => ("KEYCODE_MEDIA_FAST_FORWARD", 90),
            Self::Play => ("KEYCODE_MEDIA_PLAY", 126),
            Self::Pause => ("KEYCODE_MEDIA_PAUSE", 127),
            Self::Mute => ("KEYCODE_VOLUME_MUTE", 164),
            Self::ChannelUp => ("KEYCODE_CHANNEL_UP", 166),
            Self::ChannelDown => ("KEYCODE_CHANNEL_DOWN", 167),
            Self::Sleep => ("KEYCODE_SLEEP", 223),
            Self::Wakeup => ("KEYCODE_WAKEUP", 224),
        }
    }

    /// The ADB command that presses it.
    fn adb_command(self) -> String {
        format!("input keyevent {}", self.codes().0)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidTvConfig {
    /// Box address, e.g. `192.168.1.153`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Packages surfaced as shortcuts in the dashboard. Written `favoriteApps`; the British
    /// spelling older files kept still loads.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        alias = "favouriteApps"
    )]
    pub favorite_apps: Vec<AndroidApp>,
    /// The DIAL app woken on the box when CEC cannot be had over ADB (`YouTube` when none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wake_app: Option<String>,
}

/// The host must be a LAN device address; package and DIAL app names are checked since
/// they reach the shell and a URL path.
impl Checked for AndroidTvConfig {
    fn checked(mut self) -> Result<Self, AppError> {
        self.host = non_blank(self.host)
            .map(|host| device_host(&host))
            .transpose()?;
        self.wake_app = non_blank(self.wake_app);
        for package in self
            .favorite_apps
            .iter()
            .map(|app| &app.package)
            .chain(&self.wake_app)
        {
            validate_package(package)?;
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidApp {
    pub package: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidTvStatus {
    pub configured: bool,
    pub reachable: bool,
    /// `false` while the box is asleep (screen off), which is its idle state.
    pub awake: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_app: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// True once a Remote v2 session is available, which is what makes keys
    /// fast. Unpaired is a normal state, not a fault: ADB still works.
    pub paired: bool,
}

#[derive(Clone)]
pub struct AndroidTvManager {
    client: reqwest::Client,
    key_path: PathBuf,
    identity_path: PathBuf,
    config: Arc<JsonConfig<AndroidTvConfig>>,
    key: Arc<Mutex<Option<AdbKey>>>,
    /// Reused across calls; also serializes them, which suits a remote
    /// control and keeps a single ADB stream open at a time.
    device: Arc<Mutex<Option<AdbDevice>>>,
    /// Remote v2 session, preferred for keys and app launches: ~20 ms against
    /// ADB's ~150 ms, nearly all of which is `input` booting a JVM per press.
    remote: Arc<Mutex<Option<Session>>>,
    /// A pairing in flight. It spans two HTTP requests — the TV shows a code
    /// between them — so the open TLS session has to be held here.
    pending_pairing: Arc<Mutex<Option<Pairing>>>,
    /// When a Remote v2 connect last failed (see `REMOTE_RETRY_AFTER`).
    remote_failed_at: Arc<std::sync::Mutex<Option<Instant>>>,
    /// One APK install at a time: each holds an upload and an ADB transfer.
    installing: Arc<Semaphore>,
    /// Whether the box is awake, as its Remote v2 session says it (`None`: no session, or
    /// it has not said yet).
    power: Arc<watch::Sender<Option<bool>>>,
    /// The box's model, read once (it does not change while the box is the same).
    model: Arc<std::sync::Mutex<Option<String>>>,
}

impl AndroidTvManager {
    pub fn new(
        config_path: &Path,
        key_path: &Path,
        identity_path: &Path,
    ) -> Result<Self, AppError> {
        Ok(Self {
            client: net::device_client(DIAL_TIMEOUT)?,
            key_path: key_path.to_path_buf(),
            identity_path: identity_path.to_path_buf(),
            config: Arc::new(JsonConfig::load(config_path)?),
            key: Arc::new(Mutex::new(None)),
            device: Arc::new(Mutex::new(None)),
            remote: Arc::new(Mutex::new(None)),
            pending_pairing: Arc::new(Mutex::new(None)),
            remote_failed_at: Arc::default(),
            installing: Arc::new(Semaphore::new(1)),
            power: Arc::new(watch::Sender::new(None)),
            model: Arc::default(),
        })
    }

    /// Whether the box is awake as its Remote v2 session says it, as it changes (an event
    /// stream's); `None` without a session.
    pub fn subscribe_power(&self) -> watch::Receiver<Option<bool>> {
        self.power.subscribe()
    }

    pub async fn config(&self) -> AndroidTvConfig {
        self.config.get().await
    }

    pub async fn set_config(&self, config: AndroidTvConfig) -> Result<AndroidTvConfig, AppError> {
        let config = self.config.set(config).await?;
        // The address may have changed: nothing may keep talking to the old box.
        *self.device.lock().await = None;
        *self.remote.lock().await = None;
        *self.pending_pairing.lock().await = None;
        *crate::util::lock(&self.model) = None;
        self.power.send_replace(None);
        self.set_remote_failed(None);
        Ok(config)
    }

    /// The box's host, checked where it came in (`Checked`).
    async fn host(&self) -> Result<String, AppError> {
        self.config()
            .await
            .host
            .ok_or_else(|| Refusal::NotConfigured.into())
    }

    async fn address(&self) -> Result<String, AppError> {
        let host = self.host().await?;
        Ok(format!(
            "{host}:{}",
            self.config().await.port.unwrap_or(DEFAULT_ADB_PORT)
        ))
    }

    fn set_remote_failed(&self, at: Option<Instant>) {
        *crate::util::lock(&self.remote_failed_at) = at;
    }

    fn remote_backing_off(&self) -> bool {
        let failed = *crate::util::lock(&self.remote_failed_at);
        failed.is_some_and(|at| at.elapsed() < REMOTE_RETRY_AFTER)
    }

    /// Loads the signing key, generating and persisting one on first use.
    /// Generation is CPU-bound and slow on the Pi, hence `spawn_blocking`.
    async fn signing_key(&self) -> Result<AdbKey, AppError> {
        let mut slot = self.key.lock().await;
        if let Some(key) = slot.as_ref() {
            return Ok(key.clone());
        }

        // The key authenticates this host to the box: written 0600 from the start, and a
        // torn one is replaced (the box asks to allow it once more) rather than fatal.
        let path = self.key_path.clone();
        let key = tokio::task::spawn_blocking(move || {
            store::load_or_create_secret(
                &path,
                |der| adb::decode_key(der).ok(),
                || {
                    tracing::info!("generating an ADB key (slow on the Pi, done once)");
                    let key = adb::generate_key()?;
                    let der = adb::encode_key(&key)?;
                    Ok((key, der))
                },
            )
        })
        .await??;

        let key = AdbKey::new(key);
        *slot = Some(key.clone());
        Ok(key)
    }

    /// Runs a shell command on the kept connection, reconnecting once if it died.
    async fn shell(&self, command: &str) -> Result<String, AppError> {
        let address = self.address().await?;
        let key = self.signing_key().await?;
        let mut slot = self.device.lock().await;
        shell_on(&mut slot, &address, &key, command).await
    }

    /// Over Remote v2 when the box is paired, else (or when the session fails) the same
    /// thing over ADB. The fallback matters: Remote v2 is unavailable until someone has
    /// paired the host, and ADB keeps working meanwhile — a slower remote beats no remote.
    async fn remote_or_adb<F, Fut>(&self, remote: F, adb_command: &str) -> Result<(), AppError>
    where
        F: FnOnce(Session) -> Fut,
        Fut: std::future::Future<Output = Result<(), AppError>>,
    {
        if let Some(session) = self.remote_session().await {
            match remote(session).await {
                Ok(()) => return Ok(()),
                Err(error) => {
                    tracing::debug!(%error, "remote session failed, falling back to ADB");
                    *self.remote.lock().await = None;
                }
            }
        }
        self.shell(adb_command).await.map(|_| ())
    }

    /// Sends a key, over Remote v2 when the box is paired.
    pub async fn send_key(&self, key: AndroidKey) -> Result<(), AppError> {
        let (_, number) = key.codes();
        self.remote_or_adb(
            |session| async move { session.key(number).await },
            &key.adb_command(),
        )
        .await
    }

    /// The live Remote v2 session, reconnecting if it dropped. Returns `None`
    /// when the host is not paired, which is not an error — it is the state
    /// every host starts in.
    async fn remote_session(&self) -> Option<Session> {
        let mut slot = self.remote.lock().await;
        if let Some(session) = slot.as_ref() {
            if session.is_open() {
                return Some(session.clone());
            }
        }

        if self.remote_backing_off() {
            return None;
        }
        let host = self.host().await.ok()?;
        let mut identity = match self.identity().await {
            Ok(identity) => identity,
            Err(error) => {
                tracing::warn!(%error, "no Android TV identity");
                return None;
            }
        };
        match Session::connect(&host, &mut identity).await {
            Ok(session) => {
                *slot = Some(session.clone());
                self.set_remote_failed(None);
                self.follow_power(&session);
                Some(session)
            }
            Err(error) => {
                tracing::debug!(%error, "no Remote v2 session (not paired?)");
                self.set_remote_failed(Some(Instant::now()));
                None
            }
        }
    }

    /// What `session` says of the box's power goes into `power`, until it ends (then: none).
    fn follow_power(&self, session: &Session) {
        let (mut said, power) = (session.power(), self.power.clone());
        crate::supervised("box power", async move {
            loop {
                power.send_replace(*said.borrow_and_update());
                if said.changed().await.is_err() {
                    power.send_replace(None);
                    return;
                }
            }
        });
    }

    async fn identity(&self) -> Result<Identity, AppError> {
        let path = self.identity_path.clone();
        tokio::task::spawn_blocking(move || Identity::load_or_create(&path)).await?
    }

    /// Opens a pairing session; the TV shows a six hex-digit code afterwards.
    pub async fn start_pairing(&self) -> Result<(), AppError> {
        let host = self.host().await?;
        let identity = self.identity().await?;
        let pairing = Pairing::start(&host, &identity).await?;
        *self.pending_pairing.lock().await = Some(pairing);
        Ok(())
    }

    /// Completes pairing with the code from the screen.
    pub async fn finish_pairing(&self, code: &str) -> Result<(), AppError> {
        let pairing = self
            .pending_pairing
            .lock()
            .await
            .take()
            .ok_or_else(|| refused(Refusal::NotConfigured, "no pairing in progress"))?;
        pairing.finish(code).await?;
        // Force the next key onto the freshly paired session, without waiting out the
        // back-off of the attempts made while unpaired.
        *self.remote.lock().await = None;
        self.set_remote_failed(None);
        Ok(())
    }

    /// What the box is up to. Its power comes from the Remote v2 session when it said it;
    /// ADB is asked only what the session cannot say (the app in front, the model once),
    /// and not at all while the box sleeps.
    pub async fn status(&self) -> AndroidTvStatus {
        let configured = self.config().await.host.is_some();
        let unknown = AndroidTvStatus {
            configured,
            reachable: false,
            awake: false,
            current_app: None,
            model: None,
            paired: false,
        };
        if !configured {
            return unknown;
        }

        // Ask whether a session can be had, not whether one happens to be cached: the
        // cache is empty after every restart and right after pairing. Opening it here also
        // keeps the session warm, so the first key press is already fast.
        let paired = self.remote_session().await.is_some();
        let said = *self.power.borrow();
        let model = crate::util::lock(&self.model).clone();
        if said == Some(false) {
            return AndroidTvStatus {
                reachable: true,
                model,
                paired,
                ..unknown
            };
        }

        // One shell round-trip for what is left: each call costs a stream.
        let mut asked = Vec::new();
        if model.is_none() {
            asked.push("getprop ro.product.model");
        }
        if said.is_none() {
            asked.push("dumpsys power | grep -m1 mWakefulness=");
        }
        asked.push("dumpsys activity activities | grep -m1 ResumedActivity");
        match self.shell(&asked.join("; ")).await {
            Ok(output) => {
                let model = model.or_else(|| {
                    let read = output.lines().next().map(|line| line.trim().to_string());
                    let read = read.filter(|value| !value.is_empty());
                    crate::util::lock(&self.model).clone_from(&read);
                    read
                });
                AndroidTvStatus {
                    configured: true,
                    reachable: true,
                    awake: said.unwrap_or_else(|| output.contains("mWakefulness=Awake")),
                    current_app: parse_resumed_package(&output),
                    model,
                    paired,
                }
            }
            Err(error) => {
                tracing::debug!(%error, "the box's state not read over ADB");
                AndroidTvStatus {
                    reachable: said.is_some(),
                    awake: said.unwrap_or(false),
                    model,
                    paired,
                    ..unknown
                }
            }
        }
    }

    pub async fn launch_app(&self, package: &str) -> Result<(), AppError> {
        validate_package(package)?;
        self.remote_or_adb(
            |session| async move { session.launch(package).await },
            &format!("monkey -p {package} -c android.intent.category.LAUNCHER 1"),
        )
        .await
    }

    /// Installed launchable packages, for the app picker.
    pub async fn apps(&self) -> Result<Vec<String>, AppError> {
        let output = self.shell("pm list packages -3").await?;
        let mut packages: Vec<String> = output
            .lines()
            .filter_map(|line| line.trim().strip_prefix("package:"))
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .collect();
        packages.sort();
        Ok(packages)
    }

    /// Wakes the box, which asserts CEC One Touch Play — that powers the
    /// television on and routes it to this box's HDMI input.
    pub async fn wake(&self) -> Result<(), AppError> {
        self.send_key(AndroidKey::Wakeup).await?;
        self.one_touch_play().await
    }

    /// Sleeping the box broadcasts a CEC standby, which also turns the TV off
    /// (the box runs with `power_control_mode=broadcast`).
    pub async fn sleep(&self) -> Result<(), AppError> {
        self.send_key(AndroidKey::Sleep).await
    }

    /// The right to install an APK, one at a time: a second upload is refused while the
    /// first runs (409), before its bytes are read.
    pub fn install_permit(&self) -> Result<OwnedSemaphorePermit, AppError> {
        self.installing
            .clone()
            .try_acquire_owned()
            .map_err(|_| Refusal::Busy.into())
    }

    /// Installs an APK as it is uploaded: push it to the box's temp directory,
    /// hand it to the package manager, then clean up. `-r` reinstalls over an
    /// existing copy, which is what makes iterating on your own app painless.
    ///
    /// The upload streams through to the box, never held whole: the Pi 1 has
    /// 512 MB and no swap worth the name. The caller holds `install_permit`.
    pub async fn install_apk<S>(&self, mut apk: S) -> Result<(), AppError>
    where
        S: Stream<Item = Result<Bytes, AppError>> + Unpin + Send,
    {
        const REMOTE_PATH: &str = "/data/local/tmp/maison-install.apk";

        // ZIP local file header: every APK is a zip, so this catches a wrong
        // upload before it costs a slow transfer.
        let mut head = Vec::new();
        while head.len() < 4 {
            match apk.next().await.transpose()? {
                Some(chunk) => head.extend_from_slice(&chunk),
                None => break,
            }
        }
        if !head.starts_with(b"PK\x03\x04") {
            return Err(refused(
                Refusal::Invalid,
                "not an APK (empty, or no zip signature)",
            ));
        }

        let address = self.address().await?;
        let key = self.signing_key().await?;

        // The sync service runs on its own connection: keep the shared one
        // clean by dialling a dedicated device for the transfer.
        let mut transfer = AdbDevice::connect(&address, &key).await?;
        let whole = futures::stream::iter([Ok(Bytes::from(head))]).chain(apk);
        transfer.push_stream(REMOTE_PATH, whole, 0o644).await?;
        drop(transfer);

        let output = self.shell(&format!("pm install -r {REMOTE_PATH}")).await;
        // cleaned up whatever the install said: the box's storage is small
        clean_up(self.shell(&format!("rm -f {REMOTE_PATH}")).await);
        let output = output?;

        // what the package manager said stays in the log: a refusal is the APK's
        if output.contains("Success") {
            tracing::info!(output = output.trim(), "APK installed on the box");
            Ok(())
        } else {
            Err(refused(
                Refusal::Invalid,
                format!("the box refused the install: {}", output.trim()),
            ))
        }
    }

    /// Wakes an app on the box over DIAL (`wake_app`, else YouTube), which has it assert
    /// CEC One Touch Play: the TV powers on and switches to the box's HDMI input. It
    /// launches the app, so it interrupts what plays: only when ADB's CEC cannot be had.
    pub async fn dial_wake(&self) -> Result<(), AppError> {
        let config = self.config().await;
        let host = config.host.ok_or(Refusal::NotConfigured)?;
        let app = config
            .wake_app
            .unwrap_or_else(|| DEFAULT_WAKE_APP.to_string());
        let response = self
            .client
            .post(format!("http://{host}:{DIAL_PORT}/apps/{app}"))
            .header("Content-Type", "text/plain; charset=utf-8")
            .body("")
            .send()
            .await
            .map_err(|error| net::unreachable("Android TV box", error))?;
        if !response.status().is_success() {
            return Err(net::unreachable(
                "Android TV box",
                format!("DIAL wake refused ({})", response.status()),
            ));
        }
        Ok(())
    }

    pub async fn one_touch_play(&self) -> Result<(), AppError> {
        self.shell("cmd hdmi_control onetouchplay")
            .await
            .map(|_| ())
    }
}

/// Runs `command` on the kept connection, dialling again once if it went stale (the box
/// slept, or adbd restarted): the common case, not a fault.
async fn shell_on(
    slot: &mut Option<AdbDevice>,
    address: &str,
    key: &AdbKey,
    command: &str,
) -> Result<String, AppError> {
    if let Some(device) = slot.as_mut() {
        match device.shell(command).await {
            Ok(output) => return Ok(output),
            Err(error) => {
                tracing::debug!(%error, "ADB connection went stale, reconnecting");
                *slot = None;
            }
        }
    }
    let mut device = AdbDevice::connect(address, key).await?;
    let output = device.shell(command).await?;
    *slot = Some(device);
    Ok(output)
}

/// What removing the uploaded APK said. A leftover costs storage, not the install, so it is
/// logged rather than returned (`rm -f` prints nothing when it works).
fn clean_up(rm: Result<String, AppError>) {
    match rm {
        Ok(output) if output.trim().is_empty() => {}
        Ok(output) => tracing::warn!(output = output.trim(), "the uploaded APK stayed on the box"),
        Err(error) => tracing::warn!(%error, "could not remove the uploaded APK from the box"),
    }
}

/// Launches an app on the box, first powering the television on and routing it to the
/// box when asked: launching on a dark screen is rarely what is meant. Best-effort for
/// the TV: a dead TV link does not stop the launch.
pub async fn launch_with_tv(
    state: &AppState,
    package: &str,
    ensure_tv_on: bool,
) -> Result<(), AppError> {
    if ensure_tv_on {
        if let Err(error) = crate::tv::ensure_on(state).await {
            tracing::debug!(%error, "could not power the TV on before launching");
        }
    }
    state.androidtv.launch_app(package).await
}

/// Package names reach the shell (and DIAL app names a URL path), so they are checked
/// against the Android naming rules rather than trusted.
pub(crate) fn validate_package(package: &str) -> Result<(), AppError> {
    let valid = !package.is_empty()
        && package.len() <= 255
        && package
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_');
    if valid {
        Ok(())
    } else {
        Err(refused(
            Refusal::Invalid,
            format!("invalid package name {package:?}"),
        ))
    }
}

/// Pulls the package out of a `ResumedActivity` dump line, which looks like
/// `ResumedActivity: ActivityRecord{… u0 org.smarttube.beta/….PlaybackActivity t32}`.
fn parse_resumed_package(output: &str) -> Option<String> {
    let line = output
        .lines()
        .find(|line| line.contains("ResumedActivity"))?;
    let activity = line.split_whitespace().find(|token| token.contains('/'))?;
    let package = activity.split('/').next()?;
    (!package.is_empty()).then(|| package.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two channels spell the same key differently — a name for ADB, a
    /// number for Remote v2 — so they are asserted against each other. A
    /// mismatch would make a button behave differently depending on whether
    /// the box happens to be paired, which is the worst kind of bug to chase.
    #[test]
    fn keycode_spellings_agree() {
        let pairs = [
            (AndroidKey::Home, "KEYCODE_HOME", 3),
            (AndroidKey::Back, "KEYCODE_BACK", 4),
            (AndroidKey::Up, "KEYCODE_DPAD_UP", 19),
            (AndroidKey::Ok, "KEYCODE_DPAD_CENTER", 23),
            (AndroidKey::VolumeUp, "KEYCODE_VOLUME_UP", 24),
            (AndroidKey::PlayPause, "KEYCODE_MEDIA_PLAY_PAUSE", 85),
            (AndroidKey::Mute, "KEYCODE_VOLUME_MUTE", 164),
            (AndroidKey::Wakeup, "KEYCODE_WAKEUP", 224),
        ];
        for (key, name, code) in pairs {
            assert_eq!(key.codes(), (name, code), "{key:?}");
        }
        assert_eq!(
            AndroidKey::Ok.adb_command(),
            "input keyevent KEYCODE_DPAD_CENTER"
        );
    }

    /// The wire names the API takes map onto the table: every key has both spellings,
    /// and no two keys share one.
    #[test]
    fn every_key_has_its_own_codes() {
        let names = [
            "up",
            "down",
            "left",
            "right",
            "ok",
            "back",
            "home",
            "menu",
            "search",
            "volume_up",
            "volume_down",
            "mute",
            "play_pause",
            "play",
            "pause",
            "stop",
            "next",
            "previous",
            "rewind",
            "fast_forward",
            "channel_up",
            "channel_down",
            "power",
            "sleep",
            "wakeup",
        ];
        let codes: Vec<_> = names
            .iter()
            .map(|name| {
                serde_json::from_value::<AndroidKey>(serde_json::json!(name))
                    .expect(name)
                    .codes()
            })
            .collect();
        for (index, (name, number)) in codes.iter().enumerate() {
            assert!(name.starts_with("KEYCODE_"));
            assert!(
                codes[index + 1..]
                    .iter()
                    .all(|(n, c)| n != name && c != number),
                "{name} twice"
            );
        }
    }

    /// A kept connection that died is dialled again once, transparently; a box that is
    /// really gone is an error.
    #[tokio::test]
    async fn a_stale_connection_is_dialled_again() {
        let (address, connections) =
            adb::fake::serve(1, |command| vec![format!("ran {command}").into_bytes()]).await;
        let key = AdbKey::new(adb::generate_key().expect("key"));
        let mut slot = None;
        assert_eq!(
            shell_on(&mut slot, &address, &key, "first")
                .await
                .expect("first"),
            "ran first"
        );
        // the fake hangs up after one command: the kept connection is now stale
        assert_eq!(
            shell_on(&mut slot, &address, &key, "second")
                .await
                .expect("second"),
            "ran second"
        );
        assert_eq!(connections.load(std::sync::atomic::Ordering::SeqCst), 2);

        let mut nowhere = None;
        assert!(shell_on(&mut nowhere, "127.0.0.1:1", &key, "x")
            .await
            .is_err());
        assert!(nowhere.is_none());
    }

    /// Package names are interpolated into a shell command, so anything that
    /// could break out of the argument must be refused.
    #[test]
    fn package_validation_rejects_shell_metacharacters() {
        assert!(validate_package("org.smarttube.beta").is_ok());
        assert!(validate_package("com.google.android.youtube.tv").is_ok());
        for bad in [
            "",
            "a; rm -rf /",
            "a b",
            "a$(id)",
            "a`id`",
            "a&&b",
            "a|b",
            "a>f",
            "a'b",
        ] {
            assert!(validate_package(bad).is_err(), "{bad:?} should be refused");
        }
    }

    #[test]
    fn parses_the_resumed_package() {
        let dump = "LEAP-S1\n  mWakefulness=Awake\n  ResumedActivity: ActivityRecord{6c4101d u0 org.smarttube.beta/com.liskovsoft.smartyoutubetv2.tv.ui.playback.PlaybackActivity t32}";
        assert_eq!(
            parse_resumed_package(dump).as_deref(),
            Some("org.smarttube.beta")
        );
    }

    #[test]
    fn resumed_package_is_absent_when_the_dump_has_none() {
        assert_eq!(parse_resumed_package("LEAP-S1\nmWakefulness=Asleep"), None);
    }

    #[tokio::test]
    async fn missing_config_is_an_empty_config_not_an_error() {
        let dir = crate::util::test_dir();
        let manager = AndroidTvManager::new(
            &dir.path().join("androidtv.json"),
            &dir.path().join("adb-key"),
            &dir.path().join("atv-identity"),
        )
        .expect("manager");
        assert!(manager.config().await.host.is_none());
        let status = manager.status().await;
        assert!(!status.configured);
        assert!(!status.reachable);
    }

    /// What the session says of the box's power is the status's, and ADB is not asked
    /// while the box sleeps.
    #[tokio::test]
    async fn the_box_power_comes_from_its_session() {
        let dir = crate::util::test_dir();
        let manager = new_manager(&dir);
        manager
            .set_config(AndroidTvConfig {
                // nothing listens there: ADB fails at once
                host: Some("192.168.255.254".into()),
                port: Some(1),
                ..AndroidTvConfig::default()
            })
            .await
            .unwrap();
        // no Remote v2 attempt (it would dial the address)
        manager.set_remote_failed(Some(Instant::now()));
        *crate::util::lock(&manager.model) = Some("LEAP-S1".into());
        let power = manager.subscribe_power();
        manager.power.send_replace(Some(false));
        assert!(power.has_changed().unwrap());
        let status = manager.status().await;
        assert!(status.reachable && !status.awake, "{status:?}");
        assert_eq!(status.model.as_deref(), Some("LEAP-S1"));
    }

    fn new_manager(dir: &tempfile::TempDir) -> AndroidTvManager {
        AndroidTvManager::new(
            &dir.path().join("androidtv.json"),
            &dir.path().join("adb-key"),
            &dir.path().join("atv-identity"),
        )
        .expect("manager")
    }

    #[tokio::test]
    async fn the_host_must_be_a_lan_device_and_a_new_one_forgets_the_old_box() {
        let dir = crate::util::test_dir();
        let manager = new_manager(&dir);
        for bad in [
            "127.0.0.1",
            "8.8.8.8",
            "192.168.1.153:5555",
            "box.example.com",
            "a/b.local",
        ] {
            let config = AndroidTvConfig {
                host: Some(bad.into()),
                ..AndroidTvConfig::default()
            };
            assert!(manager.set_config(config).await.is_err(), "{bad} accepted");
        }
        manager.set_remote_failed(Some(Instant::now()));
        assert!(manager.remote_backing_off());
        let config = AndroidTvConfig {
            host: Some(" box.local ".into()),
            port: Some(5555),
            ..AndroidTvConfig::default()
        };
        let saved = manager.set_config(config).await.expect("saved");
        assert_eq!(saved.host.as_deref(), Some("box.local"));
        assert!(!manager.remote_backing_off(), "a new box gets a fresh try");
        assert!(manager.remote.lock().await.is_none());
        assert!(manager.pending_pairing.lock().await.is_none());
    }

    #[test]
    fn remote_v2_backs_off_after_a_failure() {
        let dir = crate::util::test_dir();
        let manager = new_manager(&dir);
        assert!(!manager.remote_backing_off());
        manager.set_remote_failed(Some(Instant::now()));
        assert!(manager.remote_backing_off());
        manager.set_remote_failed(
            Instant::now().checked_sub(REMOTE_RETRY_AFTER + Duration::from_secs(1)),
        );
        assert!(!manager.remote_backing_off());
    }

    #[test]
    fn one_install_at_a_time() {
        let dir = crate::util::test_dir();
        let manager = new_manager(&dir);
        let first = manager.install_permit().expect("free");
        assert!(
            manager.install_permit().is_err(),
            "busy while the first runs"
        );
        drop(first);
        assert!(manager.install_permit().is_ok());
    }

    /// The signature is read off the stream's first bytes, however they are cut.
    #[tokio::test]
    async fn an_upload_that_is_not_an_apk_is_refused_before_any_transfer() {
        let dir = crate::util::test_dir();
        let manager = new_manager(&dir);
        let chunks = |parts: Vec<&'static [u8]>| {
            futures::stream::iter(parts.into_iter().map(|p| Ok(Bytes::from_static(p))))
        };
        let code = |error: AppError| match error {
            AppError::Coded { code, .. } => code,
            other => panic!("not coded: {other}"),
        };
        let error = manager
            .install_apk(chunks(vec![b"MZ", b"\x90\x00"]))
            .await
            .unwrap_err();
        assert_eq!(code(error), "invalid", "not an APK");
        let error = manager.install_apk(chunks(vec![])).await.unwrap_err();
        assert_eq!(code(error), "invalid", "empty");
        // a real zip head, cut in two: it gets past the check, to the missing box
        let error = manager
            .install_apk(chunks(vec![b"P", b"K\x03\x04rest"]))
            .await
            .unwrap_err();
        assert_eq!(code(error), "not_configured", "no box");
    }

    #[tokio::test]
    async fn a_torn_adb_key_is_replaced_and_kept_private() {
        let dir = crate::util::test_dir();
        let manager = new_manager(&dir);
        std::fs::write(dir.path().join("adb-key"), b"torn").unwrap();
        let key = manager.signing_key().await.expect("a new key");
        let reread = new_manager(&dir).signing_key().await.expect("kept");
        assert_eq!(key.private_key(), reread.private_key());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join("adb-key"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[tokio::test]
    async fn set_config_persists_and_rejects_bad_packages() {
        let dir = crate::util::test_dir();
        let path = dir.path().join("androidtv.json");
        let manager = AndroidTvManager::new(
            &path,
            &dir.path().join("adb-key"),
            &dir.path().join("atv-identity"),
        )
        .expect("manager");

        manager
            .set_config(AndroidTvConfig {
                host: Some("192.168.1.153".to_string()),
                port: None,
                favorite_apps: vec![AndroidApp {
                    package: "org.smarttube.beta".to_string(),
                    label: "SmartTube".to_string(),
                }],
                wake_app: Some(" YouTube ".into()),
            })
            .await
            .expect("saved");

        let reloaded = AndroidTvManager::new(
            &path,
            &dir.path().join("adb-key"),
            &dir.path().join("atv-identity"),
        )
        .expect("reload")
        .config()
        .await;
        assert_eq!(reloaded.host.as_deref(), Some("192.168.1.153"));
        assert_eq!(reloaded.favorite_apps.len(), 1);
        assert_eq!(reloaded.wake_app.as_deref(), Some("YouTube"));

        let rejected = manager
            .set_config(AndroidTvConfig {
                host: Some("192.168.1.153".to_string()),
                port: None,
                favorite_apps: vec![AndroidApp {
                    package: "evil; reboot".to_string(),
                    label: "nope".to_string(),
                }],
                wake_app: None,
            })
            .await;
        assert!(rejected.is_err());
        let rejected = manager
            .set_config(AndroidTvConfig {
                host: Some("192.168.1.153".to_string()),
                wake_app: Some("../../admin".into()),
                ..AndroidTvConfig::default()
            })
            .await;
        assert!(rejected.is_err(), "the DIAL app goes into a URL path");
    }

    #[test]
    fn favorite_apps_keep_loading_under_their_old_spelling() {
        let old = r#"{"host":"192.168.1.153","favouriteApps":[{"package":"org.smarttube.beta","label":"SmartTube"}]}"#;
        let config: AndroidTvConfig = serde_json::from_str(old).expect("old file");
        assert_eq!(config.favorite_apps.len(), 1);
        let written = serde_json::to_string(&config).expect("written");
        assert!(
            written.contains("\"favoriteApps\"") && !written.contains("favourite"),
            "{written}"
        );
    }
}
