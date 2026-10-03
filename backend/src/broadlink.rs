use std::{
    net::Ipv4Addr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use rbroadlink::{Device, network::WirelessConnection, traits::DeviceTrait};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::{
    error::AppError,
    json_config::{Checked, JsonConfig},
    mitsubishi_ir,
    net::{self, device_ipv4},
    store::{self, Access, Corrupt},
};

const DEFAULT_LEARN_TIMEOUT_SECS: u64 = 30;
/// The device as error messages name it (the library's own errors stay in the log).
const BLASTER: &str = "Broadlink";

#[derive(Clone)]
pub struct BroadlinkManager {
    codes: Arc<JsonConfig<StoredCodes>>,
    climate_state_path: Arc<PathBuf>,
    climate_state: Arc<RwLock<Option<StoredClimateState>>>,
    discovered_devices: Arc<RwLock<Option<Vec<BroadlinkDiscoveredDevice>>>>,
    operation_lock: Arc<Mutex<()>>,
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

#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveCodeRequest {
    pub name: String,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub command: String,
    pub packet_base64: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearnCodeSaveRequest {
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
            climate_state: Arc::new(RwLock::new(climate_state)),
            discovered_devices: Arc::new(RwLock::new(None)),
            operation_lock: Arc::new(Mutex::new(())),
        })
    }

    pub async fn climate_state(&self) -> Option<StoredClimateState> {
        self.climate_state.read().await.clone()
    }

    pub async fn discover(
        &self,
        local_ip: Option<String>,
        force_refresh: bool,
    ) -> Result<Vec<BroadlinkDiscoveredDevice>, AppError> {
        if !force_refresh {
            if let Some(cached) = self.discovered_devices.read().await.clone() {
                return Ok(cached);
            }
        }

        let _guard = self.operation_lock.lock().await;
        if !force_refresh {
            if let Some(cached) = self.discovered_devices.read().await.clone() {
                return Ok(cached);
            }
        }

        let local_ip = parse_optional_ipv4(local_ip.as_deref())?;
        let discover_result = tokio::task::spawn_blocking(move || {
            Device::list(local_ip)
                .map(|devices| devices.into_iter().map(map_discovered_device).collect::<Vec<_>>())
                .map_err(|error| error.to_string())
        })
        .await?;

        let devices = match discover_result {
            Ok(devices) => devices,
            Err(error) if is_address_in_use_discovery_error(&error) => {
                if let Some(cached) = self.discovered_devices.read().await.clone() {
                    return Ok(cached);
                }
                return Ok(Vec::new());
            }
            Err(error) => return Err(net::unreachable(BLASTER, error)),
        };

        *self.discovered_devices.write().await = Some(devices.clone());

        Ok(devices)
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
        save_request: Option<LearnCodeSaveRequest>,
    ) -> Result<LearnResult, AppError> {
        let guard = self.operation_lock.clone().lock_owned().await;
        let local_ip = parse_optional_ipv4(local_ip.as_deref())?;
        let host_ip = device_ipv4(&host)?;
        let timeout = Duration::from_secs(timeout_secs.unwrap_or(DEFAULT_LEARN_TIMEOUT_SECS));
        let mut join = tokio::task::spawn_blocking(move || learn_ir_blocking(host_ip, local_ip));
        let packet = match tokio::time::timeout(timeout, &mut join).await {
            Ok(joined) => joined??,
            Err(_) => {
                // The blocking thread cannot be stopped: it keeps polling a blaster still in
                // learning mode (up to 30 s). The blaster stays ours until it ends, so no
                // send reaches it meanwhile.
                tokio::spawn(async move {
                    let _ = join.await;
                    drop(guard);
                });
                return Err(AppError::service_unavailable("Timed out while waiting for an IR code"));
            }
        };
        drop(guard);

        let packet_base64 = STANDARD.encode(&packet);
        let code = if let Some(save_request) = save_request {
            Some(
                self.save_code(SaveCodeRequest {
                    name: save_request.name,
                    brand: save_request.brand,
                    model: save_request.model,
                    command: save_request.command,
                    packet_base64: packet_base64.clone(),
                    tags: save_request.tags,
                })
                .await?,
            )
        } else {
            None
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
            settings: command.as_deref().and_then(mitsubishi_ir::parse_climate_settings),
            command,
            packet_length,
        })
    }

    pub async fn list_codes(&self) -> Vec<BroadlinkCodeEntry> {
        let mut codes = self.codes.read(|stored| stored.codes.clone()).await;
        codes.sort_by(|left, right| left.name.cmp(&right.name).then(left.command.cmp(&right.command)));
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
                        requested_model.as_deref().is_none_or(|model| {
                            entry.model.as_deref().map(normalize_lookup_value).as_deref() == Some(model)
                        })
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .await;
        codes.sort_by(|left, right| left.command.cmp(&right.command).then(left.name.cmp(&right.name)));
        codes
    }

    pub async fn save_code(&self, request: SaveCodeRequest) -> Result<BroadlinkCodeEntry, AppError> {
        let packet = decode_packet(&request.packet_base64)?;
        let now = Utc::now();
        let normalized_command = normalize_lookup_value(&request.command);
        let normalized_brand = request.brand.as_deref().map(normalize_lookup_value);
        let normalized_model = request.model.as_deref().map(normalize_lookup_value);
        let normalized_name = request.name.trim();

        if normalized_name.is_empty() {
            return Err(AppError::bad_request("name is required"));
        }

        if normalized_command.is_empty() {
            return Err(AppError::bad_request("command is required"));
        }

        let tags = normalize_tags(request.tags);
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
            .read(|stored| stored.codes.iter().find(|entry| entry.id == code_id).cloned())
            .await
            .ok_or_else(|| AppError::not_found("Broadlink code not found"))?;

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
        let command = settings.command().map_err(AppError::bad_request)?;
        self.send_mitsubishi_command(host, local_ip, command, model).await
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
                .map_err(AppError::bad_request)?
        {
            let packet_base64 = STANDARD.encode(&packet);
            let result = self
                .send_packet(host, local_ip, packet_base64, None, Some(normalized_command.clone()))
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
                    stored
                        .codes
                        .iter()
                        .filter(|entry| is_mitsubishi_entry(entry) && entry.command == normalized_command)
                };
                candidates()
                    .find(|entry| {
                        normalized_model.as_deref().is_some_and(|model| {
                            entry.model.as_deref().map(normalize_lookup_value).as_deref() == Some(model)
                        })
                    })
                    .or_else(|| candidates().next())
                    .cloned()
            })
            .await
            .ok_or_else(|| {
                AppError::not_found(format!(
                    "No saved Mitsubishi code found for command '{normalized_command}'"
                ))
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
        let mut state = self.climate_state.write().await;
        let power = command != mitsubishi_ir::OFF_COMMAND;
        let (last_on_command, settings) = if power {
            (
                Some(command.to_string()),
                mitsubishi_ir::parse_climate_settings(command),
            )
        } else {
            state
                .as_ref()
                .map(|previous| (previous.last_on_command.clone(), previous.settings.clone()))
                .unwrap_or((None, None))
        };
        *state = Some(StoredClimateState {
            power,
            last_command: command.to_string(),
            last_on_command,
            settings,
            host: host.to_string(),
            model: model.map(str::to_string),
            updated_at: Utc::now(),
        });
        persist_climate_state(self.climate_state_path.as_path(), state.as_ref()).await;
    }

    /// Records that a raw (learned, non-structured) command was sent. Its
    /// effect on the unit is unknown, so power/last-on state are left as-is;
    /// only the audit fields are refreshed.
    async fn record_raw_climate_command(&self, command: &str, host: &str, model: Option<&str>) {
        let mut state = self.climate_state.write().await;
        let Some(current) = state.as_mut() else {
            return;
        };
        current.last_command = command.to_string();
        current.host = host.to_string();
        if model.is_some() {
            current.model = model.map(str::to_string);
        }
        current.updated_at = Utc::now();
        persist_climate_state(self.climate_state_path.as_path(), state.as_ref()).await;
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
    let device = Device::from_ip(host, local_ip).map_err(|error| net::unreachable(BLASTER, error))?;
    match device {
        Device::Remote { remote } => remote.learn_ir().map_err(|error| net::unreachable(BLASTER, error)),
        Device::Hvac { .. } => Err(AppError::bad_request(
            "The selected Broadlink device does not support IR learning",
        )),
    }
}

fn send_packet_blocking(
    host: Ipv4Addr,
    local_ip: Option<Ipv4Addr>,
    packet: Vec<u8>,
) -> Result<(), AppError> {
    let device = Device::from_ip(host, local_ip).map_err(|error| net::unreachable(BLASTER, error))?;
    match device {
        Device::Remote { remote } => remote.send_code(&packet).map_err(|error| net::unreachable(BLASTER, error)),
        Device::Hvac { .. } => Err(AppError::bad_request(
            "The selected Broadlink device does not support IR code sending",
        )),
    }
}

fn decode_packet(packet_base64: &str) -> Result<Vec<u8>, AppError> {
    STANDARD
        .decode(packet_base64)
        .map_err(|_| AppError::bad_request("packetBase64 must be valid base64"))
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

fn is_mitsubishi_entry(entry: &BroadlinkCodeEntry) -> bool {
    entry.brand.as_deref().map(normalize_lookup_value).as_deref() == Some("mitsubishi")
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
                name: "Salon AC 22C".to_string(),
                brand: Some(" Mitsubishi ".to_string()),
                model: Some(" MSZ-AP ".to_string()),
                command: " Cool_22_Auto ".to_string(),
                packet_base64: STANDARD.encode([1_u8, 2, 3, 4]),
                tags: vec!["living-room".to_string(), "living-room".to_string()],
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
            name: name.to_string(),
            brand: Some("mitsubishi".to_string()),
            model: None,
            command: "power".to_string(),
            packet_base64: STANDARD.encode([1_u8, 2]),
            tags: Vec::new(),
        }
    }

    /// Saves at once all land on disk: none renames an older list over a newer one.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_saves_lose_no_code() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("broadlink-codes.json");
        let manager = BroadlinkManager::new(&path, &dir.path().join("climate.json")).expect("manager");
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
        assert_eq!(read_codes(&path).expect("readable").len(), 16, "the disk holds every code");
    }

    /// A save that cannot be written is not kept: memory never runs ahead of the disk.
    #[tokio::test]
    async fn a_failed_save_is_not_kept() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("broadlink-codes.json");
        let manager = BroadlinkManager::new(&path, &dir.path().join("climate.json")).expect("manager");
        manager.save_code(code("first")).await.expect("saved");
        std::fs::remove_file(&path).expect("removed");
        std::fs::create_dir(&path).expect("a directory where the file goes");
        assert!(manager.save_code(code("second")).await.is_err());
        let names: Vec<_> = manager.list_codes().await.into_iter().map(|c| c.name).collect();
        assert_eq!(names, vec!["first"]);
    }

    #[test]
    fn older_codes_files_hold_a_bare_list() {
        let dir = tempfile::tempdir().expect("tempdir");
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
        let state = reloaded.climate_state().await.expect("state should persist");
        assert_eq!(state.last_on_command.as_deref(), Some(on_command));
    }
}
