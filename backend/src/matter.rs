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

use crate::{config::Config, error::AppError};

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CoverMotion {
    Stopped,
    Opening,
    Closing,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverView {
    pub id: String,
    pub name: String,
    pub endpoint: u16,
    pub online: bool,
    /// `None` until the switch has learnt its travel time (calibration).
    pub open_percent: Option<u8>,
    pub target_open_percent: Option<u8>,
    pub motion: Option<CoverMotion>,
    pub vendor_id: Option<u16>,
    pub product_id: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy)]
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
        Ok(Self {
            inner: Arc::new(Inner {
                state_dir: state_dir.to_path_buf(),
                trust_dir: trust_dir.to_path_buf(),
                test_roots,
                controller: Mutex::new(None),
                covers: RwLock::new(covers),
                commissioning: Mutex::new(()),
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
        let reads = covers.into_iter().map(|cover| {
            let controller = controller.clone();
            async move {
                match controller {
                    Some(controller) => read_cover(&controller, cover).await,
                    None => offline_view(&cover, "Matter controller unavailable".into()),
                }
            }
        });
        futures::future::join_all(reads).await
    }

    pub async fn get(&self, id: &str) -> Result<CoverView, AppError> {
        let cover = self.cover(id).await?;
        let controller = self.controller().await?;
        Ok(read_cover(&controller, cover).await)
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
        };
        tracing::info!(
            node_id = format_args!("{:016X}", info.node_id),
            endpoint,
            "matter cover commissioned"
        );
        {
            let mut covers = self.inner.covers.write().await;
            covers.insert(cover.node_id, cover.clone());
            self.persist(&covers)?;
        }
        Ok(read_cover(&controller, cover).await)
    }

    pub async fn command(&self, id: &str, command: CoverCommand) -> Result<CoverView, AppError> {
        let cover = self.cover(id).await?;
        let controller = self.controller().await?;
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
        let result = within(COMMAND_TIMEOUT, controller.node(cover.node_id).invoke(path, fields))
            .await
            .map_err(operation_error)?;
        if let matter_controller::InvokeResult::Status(matter_controller::ImStatus::Failure(code)) =
            result
        {
            return Err(AppError::http(
                StatusCode::BAD_GATEWAY,
                format!("{} rejected the command (status {code:#04x})", cover.name),
            ));
        }
        Ok(read_cover(&controller, cover).await)
    }

    pub async fn rename(&self, id: &str, name: &str) -> Result<CoverView, AppError> {
        let name = normalize_name(name)?;
        let node_id = parse_id(id)?;
        let cover = {
            let mut covers = self.inner.covers.write().await;
            let cover = covers.get_mut(&node_id).ok_or_else(not_found)?;
            cover.name = name;
            let cover = cover.clone();
            self.persist(&covers)?;
            cover
        };
        let controller = self.controller().await?;
        Ok(read_cover(&controller, cover).await)
    }

    /// Leave the device's fabric table (best effort: an unplugged switch can
    /// still be forgotten) and drop it locally.
    pub async fn remove(&self, id: &str) -> Result<(), AppError> {
        let cover = self.cover(id).await?;
        let controller = self.controller().await?;
        decommission(&controller, cover.node_id).await;
        let mut covers = self.inner.covers.write().await;
        covers.remove(&cover.node_id);
        self.persist(&covers)
    }

    async fn cover(&self, id: &str) -> Result<CoverConfig, AppError> {
        let node_id = parse_id(id)?;
        self.inner
            .covers
            .read()
            .await
            .get(&node_id)
            .cloned()
            .ok_or_else(not_found)
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
        create_private_dir(&self.inner.state_dir)?;
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

    fn persist(&self, covers: &BTreeMap<u64, CoverConfig>) -> Result<(), AppError> {
        create_private_dir(&self.inner.state_dir)?;
        let list: Vec<&CoverConfig> = covers.values().collect();
        let path = covers_path(&self.inner.state_dir);
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&list)?)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }
}

fn covers_path(state_dir: &Path) -> PathBuf {
    state_dir.join("covers.json")
}

fn load_covers(path: &Path) -> Result<BTreeMap<u64, CoverConfig>, AppError> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) if bytes.iter().all(u8::is_ascii_whitespace) => return Ok(BTreeMap::new()),
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => return Err(error.into()),
    };
    let list: Vec<CoverConfig> = serde_json::from_slice(&bytes)?;
    Ok(list
        .into_iter()
        .map(|cover| (cover.node_id, cover))
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

fn create_private_dir(dir: &Path) -> Result<(), AppError> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
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
        Err(error) => tracing::warn!(%error, "matter: cannot read fabric index, forgetting locally"),
    }
    if let Err(error) = controller.forget_node(node_id).await {
        tracing::warn!(%error, "matter: forget_node failed");
    }
}

async fn read_cover(controller: &MatterController, cover: CoverConfig) -> CoverView {
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
        Err(error) => return offline_view(&cover, error.to_string()),
    };

    let mut view = offline_view(&cover, String::new());
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

fn offline_view(cover: &CoverConfig, error: String) -> CoverView {
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
        error: Some(error),
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
        return Err(not_found());
    }
    u64::from_str_radix(id, 16).map_err(|_| not_found())
}

fn not_found() -> AppError {
    AppError::http(StatusCode::NOT_FOUND, "Unknown shutter")
}

fn normalize_name(name: &str) -> Result<String, AppError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err(AppError::http(
            StatusCode::BAD_REQUEST,
            "Name must be 1 to 64 characters",
        ));
    }
    Ok(name.to_string())
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
        Err(AppError::http(
            StatusCode::BAD_REQUEST,
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
        CallError::Matter(Error::SetupCode(detail)) => AppError::http(
            StatusCode::BAD_REQUEST,
            format!("Invalid pairing code: {detail}"),
        ),
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

    /// Every vendored root must parse: `from_dirs` fails as a whole on a
    /// single bad certificate, which would block all commissioning.
    #[test]
    fn vendored_attestation_roots_load() {
        let trust_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../matter-trust");
        load_trust(&trust_dir).expect("matter-trust/ must load");
    }
}
