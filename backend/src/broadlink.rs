//! The Broadlink IR blaster (RM series, rbroadlink over UDP): finding it on the network,
//! learning and sending codes, the learnt codes' file, and the Mitsubishi AC's last
//! commanded state (IR is one-way: what was sent is the best knowledge of the unit's).

use std::{
    net::Ipv4Addr,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::{DateTime, Utc};
use rbroadlink::{network::WirelessConnection, traits::DeviceTrait, Device};
use serde::{Deserialize, Serialize};
use tokio::sync::{watch, Mutex, RwLock};
use uuid::Uuid;

use crate::{
    error::{AppError, Refusal},
    json_config::{Checked, JsonConfig},
    mitsubishi_ir,
    net::{self, device_ipv4},
    store::{self, Access, Corrupt},
    util::refused,
};

const DEFAULT_LEARN_TIMEOUT_SECS: u64 = 30;
/// A learning waits this long at most, whatever is asked: below the server's 180 s cap.
const MAX_LEARN_TIMEOUT_SECS: u64 = 120;
/// How many broadcasts one search sends before it says « none found » (each listens 10 s
/// for answers, rbroadlink's own wait).
const SEARCH_TRIES: usize = 2;
/// A search that found nothing is answered as such for this long: callers polling behind it
/// do not broadcast again at once.
const NOTHING_FOUND_FRESH: Duration = Duration::from_secs(10);
/// The device as error messages name it (the library's own errors stay in the log).
const BLASTER: &str = "Broadlink";

#[derive(Clone)]
pub struct BroadlinkManager {
    codes: Arc<JsonConfig<StoredCodes>>,
    climate_state_path: Arc<PathBuf>,
    /// The AC's last commanded state, said to listeners at each order.
    climate_state: Arc<watch::Sender<Option<StoredClimateState>>>,
    /// Its writes, one at a time: saved in the order they were made.
    climate_writes: Arc<Mutex<()>>,
    discovered: Arc<RwLock<Discovered>>,
    operation_lock: Arc<Mutex<()>>,
}

/// What the searches found: the blasters (kept until a forced search), or when the last
/// one found none.
#[derive(Debug, Default)]
struct Discovered {
    devices: Vec<BroadlinkDiscoveredDevice>,
    nothing_at: Option<Instant>,
}

impl Discovered {
    /// What to answer without searching, if anything.
    fn known(&self) -> Option<Vec<BroadlinkDiscoveredDevice>> {
        if !self.devices.is_empty() {
            Some(self.devices.clone())
        } else if self
            .nothing_at
            .is_some_and(|at| at.elapsed() < NOTHING_FOUND_FRESH)
        {
            Some(Vec::new())
        } else {
            None
        }
    }
}

/// Last Mitsubishi climate state commanded through this backend. IR is
/// one-way, so this is the best available approximation of the unit's state;
/// it is persisted so the UI can restore the last settings across restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredClimateState {
    /// Whether the last sent command left the unit on.
    pub power: bool,
    /// The most recent command sent (possibly `state-off`).
    pub last_command: String,
    /// The most recent non-off structured command, used to restore the form.
    pub last_on_command: Option<String>,
    /// The parsed settings of `last_on_command`, so clients never have to
    /// re-parse the command grammar.
    #[serde(default)]
    pub settings: Option<mitsubishi_ir::ClimateSettings>,
    pub host: String,
    pub model: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadlinkDiscoveredDevice {
    pub host: String,
    pub mac: String,
    pub model_code: u16,
    pub friendly_model: String,
    pub friendly_type: String,
    pub name: String,
    pub is_locked: bool,
    pub kind: String,
    pub supports_learning: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadlinkCodeEntry {
    pub id: String,
    pub name: String,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub command: String,
    pub packet_base64: String,
    pub packet_length: usize,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LearnResult {
    pub packet_base64: String,
    pub packet_length: usize,
    pub code: Option<BroadlinkCodeEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendResult {
    pub host: String,
    pub code_id: Option<String>,
    pub command: Option<String>,
    /// The settings a structured Mitsubishi `command` carries: clients never re-parse it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<mitsubishi_ir::ClimateSettings>,
    pub packet_length: usize,
}

/// A code to keep: what it is (`CodeDetails`) and its packet.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveCodeRequest {
    #[serde(flatten)]
    pub details: CodeDetails,
    pub packet_base64: String,
}

/// What a code is, as the person names it: a learnt code's too (its packet is the learnt one).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeDetails {
    pub name: String,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub command: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BroadlinkSecurityMode {
    None,
    Wep,
    Wpa,
    Wpa1,
    Wpa2,
}

/// The learnt codes file: `{ "codes": [...] }` (older files hold the bare list).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(from = "CodesFile")]
struct StoredCodes {
    codes: Vec<BroadlinkCodeEntry>,
}

impl Checked for StoredCodes {}

/// The codes file as found.
#[derive(Deserialize)]
#[serde(untagged)]
enum CodesFile {
    Stored { codes: Vec<BroadlinkCodeEntry> },
    Legacy(Vec<BroadlinkCodeEntry>),
}

impl From<CodesFile> for StoredCodes {
    fn from(file: CodesFile) -> Self {
        match file {
            CodesFile::Stored { codes } | CodesFile::Legacy(codes) => Self { codes },
        }
    }
}

/// The saved codes in `path` (missing is none; torn is an error: learnt codes cannot be
/// rebuilt without the remote in hand).
pub fn read_codes(path: &Path) -> Result<Vec<BroadlinkCodeEntry>, AppError> {
    Ok(store::read_json::<StoredCodes>(path, Corrupt::Fail)?.codes)
}

impl BroadlinkManager {
    pub fn new(codes_path: &Path, climate_state_path: &Path) -> Result<Self, AppError> {
        // the AC's last known settings: a cache of what was sent, never worth refusing to start
        let mut climate_state: Option<StoredClimateState> =
            store::read_json(climate_state_path, Corrupt::Reset)?;

        // Backfill parsed settings for files written before the field existed.
        if let Some(state) = climate_state.as_mut() {
            if state.settings.is_none() {
                state.settings = state
                    .last_on_command
                    .as_deref()
                    .and_then(mitsubishi_ir::parse_climate_settings);
            }
        }

        Ok(Self {
            codes: Arc::new(JsonConfig::load(codes_path)?),
            climate_state_path: Arc::new(climate_state_path.to_path_buf()),
            climate_state: Arc::new(watch::Sender::new(climate_state)),
            climate_writes: Arc::default(),
            discovered: Arc::default(),
            operation_lock: Arc::new(Mutex::new(())),
        })
    }

    pub async fn climate_state(&self) -> Option<StoredClimateState> {
        self.climate_state.borrow().clone()
    }

    /// The AC's last commanded state as each order changes it (an event stream's).
    pub fn subscribe_climate(&self) -> watch::Receiver<Option<StoredClimateState>> {
        self.climate_state.subscribe()
    }

    /// The blasters on the network: those found before, else one search of `SEARCH_TRIES`
    /// broadcasts that ends at the first answer. A search that found none is said for
    /// `NOTHING_FOUND_FRESH`, then tried again: an empty list is never kept.
    pub async fn discover(&self) -> Result<Vec<BroadlinkDiscoveredDevice>, AppError> {
        if let Some(known) = self.discovered.read().await.known() {
            return Ok(known);
        }
        let _guard = self.operation_lock.lock().await;
        // a search that ran while this one waited answers it
        if let Some(known) = self.discovered.read().await.known() {
            return Ok(known);
        }
        for _ in 0..SEARCH_TRIES {
            let found = search(None).await?;
            if !found.is_empty() {
                return Ok(self.found(found).await);
            }
        }
        Ok(self.found(Vec::new()).await)
    }

    /// One search now, from `local_ip`'s interface when given (setting up: an admin's).
    pub async fn rediscover(
        &self,
        local_ip: Option<String>,
    ) -> Result<Vec<BroadlinkDiscoveredDevice>, AppError> {
        let local_ip = parse_optional_ipv4(local_ip.as_deref())?;
        let _guard = self.operation_lock.lock().await;
        let found = search(local_ip).await?;
        Ok(self.found(found).await)
    }

    /// What a search found, kept (a list), or remembered as nothing for a moment.
    async fn found(
        &self,
        devices: Vec<BroadlinkDiscoveredDevice>,
    ) -> Vec<BroadlinkDiscoveredDevice> {
        let mut discovered = self.discovered.write().await;
        discovered.nothing_at = devices.is_empty().then(Instant::now);
        discovered.devices = devices.clone();
        devices
    }

    pub async fn provision(
        &self,
        ssid: String,
        password: Option<String>,
        security_mode: BroadlinkSecurityMode,
    ) -> Result<(), AppError> {
        let _guard = self.operation_lock.lock().await;
        let task = tokio::task::spawn_blocking(move || {
            let password = password.unwrap_or_default();
            let network = match security_mode {
                BroadlinkSecurityMode::None => WirelessConnection::None(&ssid),
                BroadlinkSecurityMode::Wep => WirelessConnection::WEP(&ssid, &password),
                BroadlinkSecurityMode::Wpa => WirelessConnection::WPA(&ssid, &password),
                BroadlinkSecurityMode::Wpa1 => WirelessConnection::WPA1(&ssid, &password),
                BroadlinkSecurityMode::Wpa2 => WirelessConnection::WPA2(&ssid, &password),
            };

            Device::connect_to_network(&network)
                .map(|_| ())
                .map_err(|error| net::unreachable(BLASTER, error))
        });

        task.await??;
        Ok(())
    }

    pub async fn learn_ir(
        &self,
        host: String,
        local_ip: Option<String>,
        timeout_secs: Option<u64>,
        save_as: Option<CodeDetails>,
    ) -> Result<LearnResult, AppError> {
        let guard = self.operation_lock.clone().lock_owned().await;
        let local_ip = parse_optional_ipv4(local_ip.as_deref())?;
        let host_ip = device_ipv4(&host)?;
        let timeout = Duration::from_secs(
            timeout_secs
                .unwrap_or(DEFAULT_LEARN_TIMEOUT_SECS)
                .clamp(1, MAX_LEARN_TIMEOUT_SECS),
        );
        let mut join = tokio::task::spawn_blocking(move || learn_ir_blocking(host_ip, local_ip));
        let packet = match tokio::time::timeout(timeout, &mut join).await {
            Ok(joined) => joined??,
            Err(_) => {
                // The blocking thread cannot be stopped: it keeps polling a blaster still in
                // learning mode (up to 30 s). The blaster stays ours until it ends, so no
                // send reaches it meanwhile.
                crate::supervised("broadlink learning", async move {
                    if let Ok(Err(error)) = join.await {
                        tracing::debug!(%error, "the abandoned IR learning ended");
                    }
                    drop(guard);
                });
                return Err(net::unreachable(BLASTER, "no IR code heard in time"));
            }
        };
        drop(guard);

        let packet_base64 = STANDARD.encode(&packet);
        let code = match save_as {
            Some(details) => Some(
                self.save_code(SaveCodeRequest {
                    details,
                    packet_base64: packet_base64.clone(),
                })
                .await?,
            ),
            None => None,
        };

        Ok(LearnResult {
            packet_length: packet.len(),
            packet_base64,
            code,
        })
    }

    pub async fn send_packet(
        &self,
        host: String,
        local_ip: Option<String>,
        packet_base64: String,
        code_id: Option<String>,
        command: Option<String>,
    ) -> Result<SendResult, AppError> {
        let _guard = self.operation_lock.lock().await;
        let packet = decode_packet(&packet_base64)?;
        let packet_length = packet.len();
        let local_ip = parse_optional_ipv4(local_ip.as_deref())?;
        let host_ip = device_ipv4(&host)?;

        tokio::task::spawn_blocking(move || send_packet_blocking(host_ip, local_ip, packet))
            .await??;

        Ok(SendResult {
            host,
            code_id,
            settings: command
                .as_deref()
                .and_then(mitsubishi_ir::parse_climate_settings),
            command,
            packet_length,
        })
    }

    pub async fn list_codes(&self) -> Vec<BroadlinkCodeEntry> {
        let mut codes = self.codes.read(|stored| stored.codes.clone()).await;
        codes.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then(left.command.cmp(&right.command))
        });
        codes
    }

    pub async fn list_mitsubishi_codes(&self, model: Option<&str>) -> Vec<BroadlinkCodeEntry> {
        let requested_model = model.map(normalize_lookup_value);
        let mut codes = self
            .codes
            .read(|stored| {
                stored
                    .codes
                    .iter()
                    .filter(|entry| is_mitsubishi_entry(entry))
                    .filter(|entry| {
                        requested_model
                            .as_deref()
                            .is_none_or(|model| is_model(entry, model))
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .await;
        codes.sort_by(|left, right| {
            left.command
                .cmp(&right.command)
                .then(left.name.cmp(&right.name))
        });
        codes
    }

    pub async fn save_code(
        &self,
        request: SaveCodeRequest,
    ) -> Result<BroadlinkCodeEntry, AppError> {
        let packet = decode_packet(&request.packet_base64)?;
        let details = request.details;
        let now = Utc::now();
        let normalized_command = normalize_lookup_value(&details.command);
        let normalized_brand = details.brand.as_deref().map(normalize_lookup_value);
        let normalized_model = details.model.as_deref().map(normalize_lookup_value);
        let normalized_name = crate::util::name(&details.name)?;

        if normalized_command.is_empty() {
            return Err(refused(Refusal::Invalid, "a code needs its command"));
        }

        let tags = normalize_tags(details.tags);
        self.codes
            .update(|stored| {
                if let Some(existing) = stored.codes.iter_mut().find(|entry| {
                    entry.command == normalized_command
                        && entry.brand.as_deref() == normalized_brand.as_deref()
                        && entry.model.as_deref() == normalized_model.as_deref()
                        && entry.name == normalized_name
                }) {
                    existing.packet_base64 = request.packet_base64;
                    existing.packet_length = packet.len();
                    existing.tags = tags;
                    existing.updated_at = now;
                    return Ok(existing.clone());
                }
                let entry = BroadlinkCodeEntry {
                    id: Uuid::new_v4().to_string(),
                    name: normalized_name.to_string(),
                    brand: normalized_brand.filter(|value| !value.is_empty()),
                    model: normalized_model.filter(|value| !value.is_empty()),
                    command: normalized_command,
                    packet_base64: request.packet_base64,
                    packet_length: packet.len(),
                    tags,
                    created_at: now,
                    updated_at: now,
                };
                stored.codes.push(entry.clone());
                Ok(entry)
            })
            .await
    }

    pub async fn send_saved_code(
        &self,
        host: String,
        local_ip: Option<String>,
        code_id: String,
    ) -> Result<SendResult, AppError> {
        let code = self
            .codes
            .read(|stored| {
                stored
                    .codes
                    .iter()
                    .find(|entry| entry.id == code_id)
                    .cloned()
            })
            .await
            .ok_or(Refusal::NotFound)?;

        self.send_packet(
            host,
            local_ip,
            code.packet_base64,
            Some(code.id),
            Some(code.command),
        )
        .await
    }

    /// Sends structured settings: the command is written here, from the same tables the
    /// encoder reads (the client never builds the grammar).
    pub async fn send_mitsubishi_settings(
        &self,
        host: String,
        local_ip: Option<String>,
        settings: &mitsubishi_ir::ClimateSettings,
        model: Option<String>,
    ) -> Result<SendResult, AppError> {
        let command = settings
            .command()
            .map_err(|reason| refused(Refusal::Invalid, reason))?;
        self.send_mitsubishi_command(host, local_ip, command, model)
            .await
    }

    pub async fn send_mitsubishi_command(
        &self,
        host: String,
        local_ip: Option<String>,
        command: String,
        model: Option<String>,
    ) -> Result<SendResult, AppError> {
        let normalized_command = normalize_lookup_value(&command);
        let clock_ticks = mitsubishi_ir::current_clock_ticks();
        if let Some(packet) =
            mitsubishi_ir::encode_mitsubishi_command(&normalized_command, clock_ticks)
                .map_err(|reason| refused(Refusal::Invalid, reason))?
        {
            let packet_base64 = STANDARD.encode(&packet);
            let result = self
                .send_packet(
                    host,
                    local_ip,
                    packet_base64,
                    None,
                    Some(normalized_command.clone()),
                )
                .await?;
            self.record_climate_state(&normalized_command, &result.host, model.as_deref())
                .await;
            return Ok(result);
        }

        let normalized_model = model.as_deref().map(normalize_lookup_value);

        // the code learnt for this model, else any Mitsubishi code for the command
        let code = self
            .codes
            .read(|stored| {
                let candidates = || {
                    stored.codes.iter().filter(|entry| {
                        is_mitsubishi_entry(entry) && entry.command == normalized_command
                    })
                };
                candidates()
                    .find(|entry| {
                        normalized_model
                            .as_deref()
                            .is_some_and(|model| is_model(entry, model))
                    })
                    .or_else(|| candidates().next())
                    .cloned()
            })
            .await
            .ok_or_else(|| {
                refused(
                    Refusal::NotFound,
                    format!("no Mitsubishi code for {normalized_command:?}"),
                )
            })?;

        let result = self
            .send_packet(
                host,
                local_ip,
                code.packet_base64,
                Some(code.id),
                Some(code.command.clone()),
            )
            .await?;
        self.record_raw_climate_command(&code.command, &result.host, normalized_model.as_deref())
            .await;
        Ok(result)
    }

    /// Remembers the last commanded climate state and persists it. Failures
    /// are logged but never propagated: the IR command itself already went
    /// out, and the stored state is only used to restore the UI.
    async fn record_climate_state(&self, command: &str, host: &str, model: Option<&str>) {
        // The file write happens under the lock so concurrent sends cannot
        // persist out of order (rare, and the write is a few kilobytes).
        let _guard = self.climate_writes.lock().await;
        let previous = self.climate_state.borrow().clone();
        let power = command != mitsubishi_ir::OFF_COMMAND;
        let (last_on_command, settings) = if power {
            (
                Some(command.to_string()),
                mitsubishi_ir::parse_climate_settings(command),
            )
        } else {
            previous
                .map(|previous| (previous.last_on_command, previous.settings))
                .unwrap_or((None, None))
        };
        let state = Some(StoredClimateState {
            power,
            last_command: command.to_string(),
            last_on_command,
            settings,
            host: host.to_string(),
            model: model.map(str::to_string),
            updated_at: Utc::now(),
        });
        persist_climate_state(self.climate_state_path.as_path(), state.as_ref()).await;
        self.climate_state.send_replace(state);
    }

    /// Records that a raw (learned, non-structured) command was sent. Its
    /// effect on the unit is unknown, so power/last-on state are left as-is;
    /// only the audit fields are refreshed.
    async fn record_raw_climate_command(&self, command: &str, host: &str, model: Option<&str>) {
        let _guard = self.climate_writes.lock().await;
        let Some(mut current) = self.climate_state.borrow().clone() else {
            return;
        };
        current.last_command = command.to_string();
        current.host = host.to_string();
        if model.is_some() {
            current.model = model.map(str::to_string);
        }
        current.updated_at = Utc::now();
        let state = Some(current);
        persist_climate_state(self.climate_state_path.as_path(), state.as_ref()).await;
        self.climate_state.send_replace(state);
    }
}

/// Failures are logged, not returned: the IR command already went out.
async fn persist_climate_state(path: &Path, state: Option<&StoredClimateState>) {
    if let Err(error) = store::write_json_async(path, &state, Access::Shared).await {
        tracing::warn!(%error, "failed to persist climate state");
    }
}

fn parse_optional_ipv4(value: Option<&str>) -> Result<Option<Ipv4Addr>, AppError> {
    value.map(device_ipv4).transpose()
}

/// One broadcast, and the blasters that answered within rbroadlink's wait. Another search
/// holding the port is « none found »: the next try may have it.
async fn search(local_ip: Option<Ipv4Addr>) -> Result<Vec<BroadlinkDiscoveredDevice>, AppError> {
    let listed = tokio::task::spawn_blocking(move || {
        Device::list(local_ip).map(|devices| {
            devices
                .into_iter()
                .map(map_discovered_device)
                .collect::<Vec<_>>()
        })
    })
    .await?;
    match listed {
        Ok(devices) => Ok(devices),
        Err(error) if is_address_in_use_discovery_error(&error) => {
            tracing::debug!(%error, "broadlink search: the port is taken");
            Ok(Vec::new())
        }
        Err(error) => Err(net::unreachable(BLASTER, error)),
    }
}

fn map_discovered_device(device: Device) -> BroadlinkDiscoveredDevice {
    let kind = match &device {
        Device::Remote { .. } => "remote",
        Device::Hvac { .. } => "hvac",
    }
    .to_string();
    let supports_learning = matches!(device, Device::Remote { .. });
    let info = device.get_info();

    BroadlinkDiscoveredDevice {
        host: info.address.to_string(),
        mac: crate::util::hex(&info.mac, ":"),
        model_code: info.model_code,
        friendly_model: info.friendly_model,
        friendly_type: info.friendly_type,
        name: info.name,
        is_locked: info.is_locked,
        kind,
        supports_learning,
    }
}

/// Another discovery holds the port: the last answer stands.
fn is_address_in_use_discovery_error(error: &str) -> bool {
    let message = error.to_ascii_lowercase();
    message.contains("could not send discovery message")
        && message.contains("could not bind to any port")
        && message.contains("address in use")
}

fn learn_ir_blocking(host: Ipv4Addr, local_ip: Option<Ipv4Addr>) -> Result<Vec<u8>, AppError> {
    let device =
        Device::from_ip(host, local_ip).map_err(|error| net::unreachable(BLASTER, error))?;
    match device {
        Device::Remote { remote } => remote
            .learn_ir()
            .map_err(|error| net::unreachable(BLASTER, error)),
        Device::Hvac { .. } => Err(refused(
            Refusal::Unsupported,
            format!("{host}: an HVAC Broadlink learns no IR"),
        )),
    }
}

fn send_packet_blocking(
    host: Ipv4Addr,
    local_ip: Option<Ipv4Addr>,
    packet: Vec<u8>,
) -> Result<(), AppError> {
    let device =
        Device::from_ip(host, local_ip).map_err(|error| net::unreachable(BLASTER, error))?;
    match device {
        Device::Remote { remote } => remote
            .send_code(&packet)
            .map_err(|error| net::unreachable(BLASTER, error)),
        Device::Hvac { .. } => Err(refused(
            Refusal::Unsupported,
            format!("{host}: an HVAC Broadlink sends no IR"),
        )),
    }
}

fn decode_packet(packet_base64: &str) -> Result<Vec<u8>, AppError> {
    STANDARD
        .decode(packet_base64)
        .map_err(|error| refused(Refusal::Invalid, format!("packetBase64: {error}")))
}

fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    let mut tags = tags
        .into_iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .collect::<Vec<_>>();
    tags.sort();
    tags.dedup();
    tags
}

fn normalize_lookup_value(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

/// Whether `entry` was learnt for `model` (already normalised).
fn is_model(entry: &BroadlinkCodeEntry, model: &str) -> bool {
    entry
        .model
        .as_deref()
        .map(normalize_lookup_value)
        .as_deref()
        == Some(model)
}

fn is_mitsubishi_entry(entry: &BroadlinkCodeEntry) -> bool {
    entry
        .brand
        .as_deref()
        .map(normalize_lookup_value)
        .as_deref()
        == Some("mitsubishi")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_lookup_value_trims_and_lowercases() {
        assert_eq!(normalize_lookup_value("  Cool_22_Auto "), "cool_22_auto");
    }

    #[test]
    fn normalize_tags_deduplicates_and_sorts() {
        let tags = normalize_tags(vec![
            " bedroom ".to_string(),
            "mitsubishi".to_string(),
            "bedroom".to_string(),
            "".to_string(),
        ]);
        assert_eq!(tags, vec!["bedroom", "mitsubishi"]);
    }

    #[tokio::test]
    async fn save_code_normalizes_and_persists() {
        let dir = crate::util::test_dir();
        let temp_root = dir.path();
        let path = temp_root.join("broadlink-codes.json");
        let climate_path = temp_root.join("climate-state.json");
        let manager = BroadlinkManager::new(&path, &climate_path).expect("manager should build");

        let code = manager
            .save_code(SaveCodeRequest {
                details: CodeDetails {
                    name: "Salon AC 22C".to_string(),
                    brand: Some(" Mitsubishi ".to_string()),
                    model: Some(" MSZ-AP ".to_string()),
                    command: " Cool_22_Auto ".to_string(),
                    tags: vec!["living-room".to_string(), "living-room".to_string()],
                },
                packet_base64: STANDARD.encode([1_u8, 2, 3, 4]),
            })
            .await
            .expect("code should save");

        assert_eq!(code.brand.as_deref(), Some("mitsubishi"));
        assert_eq!(code.model.as_deref(), Some("msz-ap"));
        assert_eq!(code.command, "cool_22_auto");
        assert_eq!(code.packet_length, 4);
        assert_eq!(code.tags, vec!["living-room"]);

        let saved = std::fs::read_to_string(&path).expect("codes file should exist");
        assert!(saved.contains("cool_22_auto"));
    }

    fn code(name: &str) -> SaveCodeRequest {
        SaveCodeRequest {
            details: CodeDetails {
                name: name.to_string(),
                brand: Some("mitsubishi".to_string()),
                model: None,
                command: "power".to_string(),
                tags: Vec::new(),
            },
            packet_base64: STANDARD.encode([1_u8, 2]),
        }
    }

    /// Saves at once all land on disk: none renames an older list over a newer one.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_saves_lose_no_code() {
        let dir = crate::util::test_dir();
        let path = dir.path().join("broadlink-codes.json");
        let manager =
            BroadlinkManager::new(&path, &dir.path().join("climate.json")).expect("manager");
        let saves: Vec<_> = (0..16)
            .map(|n| {
                let manager = manager.clone();
                tokio::spawn(async move { manager.save_code(code(&format!("code {n}"))).await })
            })
            .collect();
        for save in saves {
            save.await.expect("joined").expect("saved");
        }
        assert_eq!(manager.list_codes().await.len(), 16);
        assert_eq!(
            read_codes(&path).expect("readable").len(),
            16,
            "the disk holds every code"
        );
    }

    /// A save that cannot be written is not kept: memory never runs ahead of the disk.
    #[tokio::test]
    async fn a_failed_save_is_not_kept() {
        let dir = crate::util::test_dir();
        let path = dir.path().join("broadlink-codes.json");
        let manager =
            BroadlinkManager::new(&path, &dir.path().join("climate.json")).expect("manager");
        manager.save_code(code("first")).await.expect("saved");
        std::fs::remove_file(&path).expect("removed");
        std::fs::create_dir(&path).expect("a directory where the file goes");
        assert!(manager.save_code(code("second")).await.is_err());
        let names: Vec<_> = manager
            .list_codes()
            .await
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(names, vec!["first"]);
    }

    #[test]
    fn older_codes_files_hold_a_bare_list() {
        let dir = crate::util::test_dir();
        let path = dir.path().join("codes.json");
        let entry = serde_json::json!({
            "id": "1", "name": "n", "brand": null, "model": null, "command": "c",
            "packetBase64": "AQ==", "packetLength": 1, "tags": [],
            "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z"
        });
        std::fs::write(&path, serde_json::json!([entry]).to_string()).expect("written");
        assert_eq!(read_codes(&path).expect("legacy").len(), 1);
        std::fs::write(&path, serde_json::json!({"codes": [entry]}).to_string()).expect("written");
        assert_eq!(read_codes(&path).expect("current").len(), 1);
    }

    /// A code is named as every name is, and needs its command.
    #[tokio::test]
    async fn a_code_needs_a_name_and_a_command() {
        let dir = crate::util::test_dir();
        let manager = BroadlinkManager::new(
            &dir.path().join("codes.json"),
            &dir.path().join("climate.json"),
        )
        .unwrap();
        let mut nameless = code("  ");
        let refused = manager.save_code(nameless.clone()).await.unwrap_err();
        assert!(matches!(
            refused,
            AppError::Coded {
                code: "bad_name",
                ..
            }
        ));
        nameless.details.name = "Power".into();
        nameless.details.command = " ".into();
        let refused = manager.save_code(nameless.clone()).await.unwrap_err();
        assert!(matches!(
            refused,
            AppError::Coded {
                code: "invalid",
                ..
            }
        ));
        nameless.details.command = "power".into();
        nameless.packet_base64 = "***".into();
        let refused = manager.save_code(nameless).await.unwrap_err();
        assert!(matches!(
            refused,
            AppError::Coded {
                code: "invalid",
                ..
            }
        ));
    }

    /// A search that found nothing is not kept: it is said for a moment, then tried again.
    #[test]
    fn nothing_found_is_said_for_a_moment_only() {
        let mut discovered = Discovered::default();
        assert_eq!(discovered.known(), None, "never searched: search");
        discovered.nothing_at = Some(Instant::now());
        assert_eq!(
            discovered.known(),
            Some(Vec::new()),
            "just searched: nothing"
        );
        discovered.nothing_at = Instant::now().checked_sub(NOTHING_FOUND_FRESH);
        assert_eq!(discovered.known(), None, "a while ago: search again");
        let blaster = BroadlinkDiscoveredDevice {
            host: "192.168.1.60".into(),
            mac: "aa".into(),
            model_code: 0x5f36,
            friendly_model: "RM mini".into(),
            friendly_type: "Remote".into(),
            name: "rm".into(),
            is_locked: false,
            kind: "remote".into(),
            supports_learning: true,
        };
        discovered.devices = vec![blaster.clone()];
        assert_eq!(discovered.known(), Some(vec![blaster]), "found: kept");
    }

    /// Each order says the AC's new state to its listeners.
    #[tokio::test]
    async fn the_climate_state_is_said_at_each_order() {
        let dir = crate::util::test_dir();
        let manager = BroadlinkManager::new(
            &dir.path().join("codes.json"),
            &dir.path().join("climate.json"),
        )
        .unwrap();
        let mut climate = manager.subscribe_climate();
        manager
            .record_climate_state("state-cool-21-fan-auto-vane-auto", "192.168.1.60", None)
            .await;
        assert!(climate.has_changed().unwrap());
        assert!(climate
            .borrow_and_update()
            .as_ref()
            .is_some_and(|s| s.power));
        manager
            .record_raw_climate_command("power", "192.168.1.60", Some("msz"))
            .await;
        assert_eq!(
            climate.borrow_and_update().as_ref().unwrap().last_command,
            "power"
        );
    }

    #[test]
    fn discovery_recognises_a_busy_port() {
        assert!(is_address_in_use_discovery_error(
            "Could not send discovery message: could not bind to any port: Address in use"
        ));
        assert!(!is_address_in_use_discovery_error("timed out"));
    }

    #[tokio::test]
    async fn climate_state_is_recorded_and_reloaded() {
        let dir = crate::util::test_dir();
        let temp_root = dir.path();
        let codes_path = temp_root.join("broadlink-codes.json");
        let climate_path = temp_root.join("climate-state.json");
        let manager =
            BroadlinkManager::new(&codes_path, &climate_path).expect("manager should build");

        let on_command = "state-cool-21-fan-auto-vane-auto-wide-center-stopin-180";
        manager
            .record_climate_state(on_command, "192.168.1.50", Some("msz-hj5va"))
            .await;
        manager
            .record_climate_state("state-off", "192.168.1.50", Some("msz-hj5va"))
            .await;

        let state = manager.climate_state().await.expect("state should exist");
        assert!(!state.power);
        assert_eq!(state.last_command, "state-off");
        // Turning off must not forget the last on-state settings.
        assert_eq!(state.last_on_command.as_deref(), Some(on_command));
        let settings = state.settings.as_ref().expect("settings should be parsed");
        assert_eq!(settings.mode, mitsubishi_ir::Mode::Cool);
        assert_eq!(settings.temperature, 21);
        assert_eq!(settings.stop_in_minutes, Some(180));

        // A fresh manager reloads the persisted state from disk.
        let reloaded =
            BroadlinkManager::new(&codes_path, &climate_path).expect("manager should build");
        let state = reloaded
            .climate_state()
            .await
            .expect("state should persist");
        assert_eq!(state.last_on_command.as_deref(), Some(on_command));
    }
}
