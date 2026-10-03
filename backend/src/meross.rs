use std::{collections::HashMap, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    error::AppError,
    store::{self, Corrupt},
    tuya::DeviceRef,
};

const HTTP_TIMEOUT_MS: u64 = 5_000;

#[derive(Debug, Clone, Deserialize)]
pub struct MerossDeviceConfig {
    pub name: String,
    pub ip: String,
    pub key: String,
    pub uuid: Option<String>,
    pub mac: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MerossDeviceListEntry {
    pub id: String,
    pub name: String,
    pub ip: String,
    #[serde(rename = "isOnline")]
    pub is_online: bool,
    #[serde(rename = "isOn")]
    pub is_on: bool,
    #[serde(rename = "lastPing")]
    pub last_ping: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MerossStatus {
    pub online: bool,
    pub on: bool,
    pub electricity: Option<MerossStatusElectricity>,
    pub hardware: Option<MerossHardware>,
    pub firmware: Option<MerossFirmware>,
    pub wifi: MerossWifi,
    #[serde(rename = "lastUpdate")]
    pub last_update: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MerossStatusElectricity {
    pub voltage: f64,
    pub current: f64,
    pub power: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MerossHardware {
    #[serde(rename = "type")]
    pub device_type: String,
    pub version: String,
    #[serde(rename = "chipType")]
    pub chip_type: String,
    pub uuid: String,
    pub mac: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MerossFirmware {
    pub version: String,
    #[serde(rename = "compileTime")]
    pub compile_time: String,
    #[serde(rename = "innerIp")]
    pub inner_ip: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MerossWifi {
    pub signal: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MerossElectricityFormatted {
    pub voltage: String,
    pub current: String,
    pub power: String,
    pub raw: MerossElectricityRaw,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerossElectricityRaw {
    pub channel: i32,
    pub current: i32,
    pub voltage: i32,
    pub power: i32,
    pub config: Option<MerossElectricityConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerossElectricityConfig {
    #[serde(rename = "voltageRatio")]
    pub voltage_ratio: i32,
    #[serde(rename = "electricityRatio")]
    pub electricity_ratio: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerossConsumptionEntry {
    pub date: String,
    pub time: i64,
    pub value: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct MerossConsumptionSummary {
    pub days: usize,
    #[serde(rename = "totalWh")]
    pub total_wh: i32,
    #[serde(rename = "totalKwh")]
    pub total_kwh: f64,
}

#[derive(Debug, Clone)]
pub struct MerossManager {
    client: reqwest::Client,
    devices: Arc<HashMap<String, MerossManagedDevice>>,
}

#[derive(Debug, Clone)]
struct MerossManagedDevice {
    config: MerossDeviceConfig,
    state: Arc<RwLock<MerossCachedState>>,
}

#[derive(Debug, Clone, Default)]
struct MerossCachedState {
    is_online: bool,
    is_on: bool,
    last_ping: i64,
}

#[derive(Debug, Serialize)]
pub struct MerossStats {
    pub total: usize,
    pub online: usize,
    pub offline: usize,
    pub devices: Vec<MerossStatsEntry>,
}

#[derive(Debug, Serialize)]
pub struct MerossStatsEntry {
    pub id: String,
    pub name: String,
    pub ip: String,
    #[serde(rename = "isOnline")]
    pub is_online: bool,
    #[serde(rename = "lastPing")]
    pub last_ping: i64,
}

#[derive(Debug, Deserialize)]
struct MerossPacket {
    header: MerossHeader,
    payload: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct MerossHeader {
    method: String,
}

#[derive(Debug, Serialize)]
struct MerossRequestPacket<'a> {
    header: MerossRequestHeader<'a>,
    payload: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct MerossRequestHeader<'a> {
    from: String,
    #[serde(rename = "messageId")]
    message_id: String,
    method: &'a str,
    namespace: &'a str,
    #[serde(rename = "payloadVersion")]
    payload_version: i32,
    sign: String,
    timestamp: i64,
    #[serde(rename = "timestampMs")]
    timestamp_ms: i64,
}

#[derive(Debug, Deserialize)]
struct MerossSystemAllPayload {
    all: MerossSystemAll,
}

#[derive(Debug, Deserialize)]
struct MerossSystemAll {
    system: MerossSystem,
    control: Option<MerossDigest>,
    digest: Option<MerossDigest>,
}

#[derive(Debug, Deserialize)]
struct MerossSystem {
    hardware: MerossSystemHardware,
    firmware: MerossSystemFirmware,
}

#[derive(Debug, Deserialize)]
struct MerossSystemHardware {
    #[serde(rename = "type")]
    device_type: String,
    version: String,
    #[serde(rename = "chipType")]
    chip_type: String,
    uuid: String,
    #[serde(rename = "macAddress")]
    mac_address: String,
}

#[derive(Debug, Deserialize)]
struct MerossSystemFirmware {
    version: String,
    #[serde(rename = "compileTime")]
    compile_time: String,
    #[serde(rename = "innerIp")]
    inner_ip: String,
}

#[derive(Debug, Deserialize)]
struct MerossDigest {
    toggle: Option<MerossToggle>,
    togglex: Option<Vec<MerossToggleX>>,
}

#[derive(Debug, Deserialize)]
struct MerossToggle {
    onoff: i32,
}

#[derive(Debug, Deserialize)]
struct MerossToggleX {
    onoff: i32,
}

#[derive(Debug, Deserialize)]
struct MerossRuntimePayload {
    runtime: MerossRuntime,
}

#[derive(Debug, Deserialize)]
struct MerossRuntime {
    signal: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct MerossElectricityPayload {
    electricity: MerossElectricityRaw,
}

#[derive(Debug, Deserialize)]
struct MerossConsumptionPayload {
    consumptionx: Option<Vec<MerossConsumptionEntry>>,
}

impl MerossManager {
    pub fn new(config_path: &std::path::Path) -> Result<Self, AppError> {
        let configs = store::read_json::<Vec<MerossDeviceConfig>>(config_path, Corrupt::Fail)?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(HTTP_TIMEOUT_MS))
            .build()?;

        let devices = configs
            .into_iter()
            .map(|config| {
                let id = config.ip.clone();
                let device = MerossManagedDevice { config, state: Arc::new(RwLock::new(MerossCachedState::default())) };
                (id, device)
            })
            .collect::<HashMap<_, _>>();

        Ok(Self {
            client,
            devices: Arc::new(devices),
        })
    }

    pub async fn list_devices(&self) -> Vec<MerossDeviceListEntry> {
        let mut entries = Vec::with_capacity(self.devices.len());
        for (id, device) in self.devices.iter() {
            let snapshot = self.snapshot_device(device).await;
            entries.push(MerossDeviceListEntry {
                id: id.clone(),
                name: device.config.name.clone(),
                ip: device.config.ip.clone(),
                is_online: snapshot.is_online,
                is_on: snapshot.is_on,
                last_ping: snapshot.last_ping,
            });
        }
        entries
    }

    pub async fn get_stats(&self) -> MerossStats {
        let devices = self.list_devices().await;
        let online = devices.iter().filter(|device| device.is_online).count();
        MerossStats {
            total: devices.len(),
            online,
            offline: devices.len().saturating_sub(online),
            devices: devices
                .into_iter()
                .map(|device| MerossStatsEntry {
                    id: device.id,
                    name: device.name,
                    ip: device.ip,
                    is_online: device.is_online,
                    last_ping: device.last_ping,
                })
                .collect(),
        }
    }

    pub async fn get_status(&self, device_id: &str) -> Result<(DeviceRef, MerossStatus), AppError> {
        let device = self.get_device(device_id)?;
        let system = match self.get_system_all(device).await {
            Ok(system) => system,
            Err(error) => {
                tracing::warn!(
                    device = %device.config.name,
                    ip = %device.config.ip,
                    error = %error,
                    "meross device unreachable, returning cached offline status"
                );
                let cached = device.state.read().await;
                let status = MerossStatus {
                    online: false,
                    on: cached.is_on,
                    electricity: None,
                    hardware: None,
                    firmware: None,
                    wifi: MerossWifi { signal: None },
                    last_update: cached.last_ping,
                };
                return Ok((device.reference(), status));
            }
        };
        let electricity = self.get_electricity_raw(device).await.ok().map(|raw| MerossStatusElectricity {
            voltage: f64::from(raw.voltage) / 10.0,
            current: f64::from(raw.current) / 1000.0,
            power: f64::from(raw.power) / 1000.0,
        });
        let signal = self.get_runtime_signal(device).await.ok().flatten();

        let is_on = extract_on_state(&system.all);
        let now = device.seen(Some(is_on)).await;
        let MerossSystem { hardware, firmware } = system.all.system;
        let status = MerossStatus {
            online: true,
            on: is_on,
            electricity,
            hardware: Some(MerossHardware {
                device_type: hardware.device_type,
                version: hardware.version,
                chip_type: hardware.chip_type,
                uuid: hardware.uuid,
                mac: hardware.mac_address,
            }),
            firmware: Some(MerossFirmware {
                version: firmware.version,
                compile_time: firmware.compile_time,
                inner_ip: firmware.inner_ip,
            }),
            wifi: MerossWifi { signal },
            last_update: now,
        };

        Ok((device.reference(), status))
    }

    pub async fn get_electricity(&self, device_id: &str) -> Result<(DeviceRef, MerossElectricityFormatted), AppError> {
        let device = self.get_device(device_id)?;
        let raw = self.get_electricity_raw(device).await?;
        device.seen(None).await;
        Ok((device.reference(), format_electricity(raw)))
    }

    pub async fn get_consumption(
        &self,
        device_id: &str,
    ) -> Result<(DeviceRef, Vec<MerossConsumptionEntry>, MerossConsumptionSummary), AppError> {
        let device = self.get_device(device_id)?;
        let payload: MerossConsumptionPayload = self
            .exchange(device, "GET", "Appliance.Control.ConsumptionX", serde_json::json!({}))
            .await?;
        device.seen(None).await;
        let consumption = payload.consumptionx.unwrap_or_default();
        let summary = summarize_consumption(&consumption);
        Ok((device.reference(), consumption, summary))
    }

    /// Switches the plug: `ToggleX` first (recent plugs), the older `Toggle` if refused.
    pub async fn toggle(&self, device_id: &str, on: bool) -> Result<DeviceRef, AppError> {
        let device = self.get_device(device_id)?;
        let (toggle_x, toggle) = toggle_payloads(on);
        let switched: Result<serde_json::Value, _> =
            self.exchange(device, "SET", "Appliance.Control.ToggleX", toggle_x).await;
        if switched.is_err() {
            let _: serde_json::Value = self.exchange(device, "SET", "Appliance.Control.Toggle", toggle).await?;
        }
        device.seen(Some(on)).await;
        Ok(device.reference())
    }

    pub async fn set_dnd(&self, device_id: &str, enabled: bool) -> Result<DeviceRef, AppError> {
        let device = self.get_device(device_id)?;
        let payload = serde_json::json!({ "DNDMode": { "mode": u8::from(enabled) } });
        let _: serde_json::Value = self.exchange(device, "SET", "Appliance.System.DNDMode", payload).await?;
        device.seen(None).await;
        Ok(device.reference())
    }

    fn get_device(&self, device_id: &str) -> Result<&MerossManagedDevice, AppError> {
        self.devices
            .get(device_id)
            .ok_or_else(|| AppError::not_found("Device not found"))
    }

    async fn get_system_all(&self, device: &MerossManagedDevice) -> Result<MerossSystemAllPayload, AppError> {
        self.exchange(device, "GET", "Appliance.System.All", serde_json::json!({})).await
    }

    async fn get_runtime_signal(&self, device: &MerossManagedDevice) -> Result<Option<i32>, AppError> {
        let payload: MerossRuntimePayload = self
            .exchange(device, "GET", "Appliance.System.Runtime", serde_json::json!({}))
            .await?;
        Ok(payload.runtime.signal)
    }

    async fn get_electricity_raw(&self, device: &MerossManagedDevice) -> Result<MerossElectricityRaw, AppError> {
        let payload: MerossElectricityPayload = self
            .exchange(
                device,
                "GET",
                "Appliance.Control.Electricity",
                serde_json::json!({ "electricity": { "channel": 0 } }),
            )
            .await?;
        Ok(payload.electricity)
    }

    /// One signed request to the plug and its answer's payload; any failure marks the
    /// plug offline.
    async fn exchange<T: for<'de> Deserialize<'de>>(
        &self,
        device: &MerossManagedDevice,
        method: &'static str,
        namespace: &'static str,
        payload: serde_json::Value,
    ) -> Result<T, AppError> {
        let result = self.exchange_raw(&device.config, method, namespace, payload).await;
        if result.is_err() {
            device.state.write().await.is_online = false;
        }
        serde_json::from_value(result?).map_err(AppError::from)
    }

    async fn exchange_raw(
        &self,
        config: &MerossDeviceConfig,
        method: &'static str,
        namespace: &'static str,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, AppError> {
        let packet = build_packet(config, namespace, method, payload);
        let response = self
            .client
            .post(format!("http://{}/config", config.ip))
            .json(&packet)
            .send()
            .await
            .map_err(|error| {
                AppError::service_unavailable(format!(
                    "Meross device {} ({}) is unreachable: {error}",
                    config.name, config.ip
                ))
            })?;
        if !response.status().is_success() {
            return Err(AppError::service_unavailable(format!("Meross device returned {}", response.status())));
        }
        let packet = response.json::<MerossPacket>().await?;
        if packet.header.method == "ERROR" {
            return Err(AppError::service_unavailable("Meross device returned protocol error"));
        }
        Ok(packet.payload)
    }

    async fn snapshot_device(&self, device: &MerossManagedDevice) -> MerossCachedState {
        if let Ok(system) = self.get_system_all(device).await {
            device.seen(Some(extract_on_state(&system.all))).await;
        }
        device.state.read().await.clone()
    }
}

impl MerossManagedDevice {
    fn reference(&self) -> DeviceRef {
        DeviceRef { id: self.config.ip.clone(), name: self.config.name.clone() }
    }

    /// The plug answered: online now, and `on` when the answer told its state. The time.
    async fn seen(&self, on: Option<bool>) -> i64 {
        let now = now_millis();
        let mut state = self.state.write().await;
        state.is_online = true;
        state.last_ping = now;
        if let Some(on) = on {
            state.is_on = on;
        }
        now
    }
}

/// The two ways plugs take a switch order: `ToggleX` (by channel) and the older `Toggle`.
fn toggle_payloads(on: bool) -> (serde_json::Value, serde_json::Value) {
    let onoff = u8::from(on);
    (
        serde_json::json!({ "togglex": { "channel": 0, "onoff": onoff } }),
        serde_json::json!({ "channel": 0, "toggle": { "onoff": onoff } }),
    )
}

fn format_electricity(raw: MerossElectricityRaw) -> MerossElectricityFormatted {
    MerossElectricityFormatted {
        voltage: format_voltage(raw.voltage),
        current: format_current(raw.current),
        power: format_power(raw.power),
        raw,
    }
}

fn summarize_consumption(consumption: &[MerossConsumptionEntry]) -> MerossConsumptionSummary {
    let total_wh = consumption.iter().map(|entry| entry.value).sum::<i32>();
    MerossConsumptionSummary {
        days: consumption.len(),
        total_wh,
        total_kwh: ((f64::from(total_wh) / 1000.0) * 100.0).round() / 100.0,
    }
}

fn build_packet(
    config: &MerossDeviceConfig,
    namespace: &'static str,
    method: &'static str,
    payload: serde_json::Value,
) -> MerossRequestPacket<'static> {
    let message_id = Uuid::new_v4().simple().to_string();
    let timestamp = chrono::Utc::now().timestamp();
    let timestamp_ms = now_millis() % 1000;
    let sign = compute_sign(&message_id, &config.key, timestamp);

    MerossRequestPacket {
        header: MerossRequestHeader {
            from: format!("http://{}/config", config.ip),
            message_id,
            method,
            namespace,
            payload_version: 1,
            sign,
            timestamp,
            timestamp_ms,
        },
        payload,
    }
}

fn compute_sign(message_id: &str, key: &str, timestamp: i64) -> String {
    let input = format!("{message_id}{key}{timestamp}");
    format!("{:x}", md5::compute(input))
}

fn extract_on_state(system: &MerossSystemAll) -> bool {
    if let Some(togglex) = system
        .control
        .as_ref()
        .and_then(|control| control.togglex.as_ref())
        .and_then(|togglex| togglex.first())
    {
        return togglex.onoff == 1;
    }

    if let Some(toggle) = system.control.as_ref().and_then(|control| control.toggle.as_ref()) {
        return toggle.onoff == 1;
    }

    if let Some(togglex) = system
        .digest
        .as_ref()
        .and_then(|digest| digest.togglex.as_ref())
        .and_then(|togglex| togglex.first())
    {
        return togglex.onoff == 1;
    }

    system
        .digest
        .as_ref()
        .and_then(|digest| digest.toggle.as_ref())
        .is_some_and(|toggle| toggle.onoff == 1)
}

fn format_voltage(raw: i32) -> String {
    trim_float_suffix(f64::from(raw) / 10.0, "V")
}

fn format_current(raw: i32) -> String {
    trim_float_suffix(f64::from(raw) / 1000.0, "A")
}

fn format_power(raw: i32) -> String {
    trim_float_suffix(f64::from(raw) / 1000.0, "W")
}

fn trim_float_suffix(value: f64, suffix: &str) -> String {
    if value.fract() == 0.0 {
        format!("{}{suffix}", value.trunc() as i64)
    } else {
        format!("{}{suffix}", value)
    }
}

fn now_millis() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn config() -> MerossDeviceConfig {
        MerossDeviceConfig {
            name: "Plug".into(),
            ip: "10.0.0.9".into(),
            key: "secret".into(),
            uuid: None,
            mac: None,
        }
    }

    fn system(value: serde_json::Value) -> MerossSystemAll {
        let mut all = json!({
            "system": {
                "hardware": { "type": "mss310", "version": "2", "chipType": "mt7682", "uuid": "u", "macAddress": "m" },
                "firmware": { "version": "6", "compileTime": "t", "innerIp": "10.0.0.9" },
            }
        });
        all.as_object_mut().unwrap().extend(value.as_object().cloned().unwrap());
        serde_json::from_value(all).unwrap()
    }

    #[test]
    fn the_sign_is_md5_of_id_key_and_time() {
        assert_eq!(compute_sign("", "", 0), format!("{:x}", md5::compute("0")));
        assert_eq!(compute_sign("abc", "key", 12), format!("{:x}", md5::compute("abckey12")));
    }

    #[test]
    fn a_packet_carries_a_signed_header() {
        let packet = serde_json::to_value(build_packet(&config(), "Appliance.System.All", "GET", json!({}))).unwrap();
        let header = &packet["header"];
        assert_eq!(header["method"], "GET");
        assert_eq!(header["namespace"], "Appliance.System.All");
        assert_eq!(header["from"], "http://10.0.0.9/config");
        assert_eq!(header["payloadVersion"], 1);
        let id = header["messageId"].as_str().unwrap();
        assert_eq!(id.len(), 32);
        let sign = compute_sign(id, "secret", header["timestamp"].as_i64().unwrap());
        assert_eq!(header["sign"], sign);
        assert_eq!(packet["payload"], json!({}));
    }

    #[test]
    fn the_on_state_prefers_control_then_digest_and_togglex() {
        let on = |value| extract_on_state(&system(value));
        assert!(on(json!({ "control": { "togglex": [{ "onoff": 1 }] }, "digest": { "togglex": [{ "onoff": 0 }] } })));
        assert!(!on(json!({ "control": { "toggle": { "onoff": 0 } }, "digest": { "togglex": [{ "onoff": 1 }] } })));
        assert!(on(json!({ "digest": { "togglex": [{ "onoff": 1 }] } })));
        assert!(on(json!({ "digest": { "toggle": { "onoff": 1 } } })));
        assert!(!on(json!({})));
    }

    #[test]
    fn toggle_payloads_say_on_as_one() {
        let (toggle_x, toggle) = toggle_payloads(true);
        assert_eq!(toggle_x, json!({ "togglex": { "channel": 0, "onoff": 1 } }));
        assert_eq!(toggle, json!({ "channel": 0, "toggle": { "onoff": 1 } }));
        assert_eq!(toggle_payloads(false).0["togglex"]["onoff"], 0);
    }

    #[test]
    fn electricity_is_formatted_in_units() {
        let formatted = format_electricity(MerossElectricityRaw {
            channel: 0,
            current: 1500,
            voltage: 2301,
            power: 12000,
            config: None,
        });
        assert_eq!(formatted.voltage, "230.1V");
        assert_eq!(formatted.current, "1.5A");
        assert_eq!(formatted.power, "12W");
    }

    #[test]
    fn consumption_sums_to_rounded_kwh() {
        let entries = [1234, 1000]
            .map(|value| MerossConsumptionEntry { date: "2026-10-01".into(), time: 0, value });
        let summary = summarize_consumption(&entries);
        assert_eq!((summary.days, summary.total_wh, summary.total_kwh), (2, 2234, 2.23));
        assert_eq!(summarize_consumption(&[]).total_kwh, 0.0);
    }

    #[tokio::test]
    async fn an_unreachable_plug_reads_as_offline() {
        let root = std::env::temp_dir().join("maison-meross-unit").join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("meross-devices.json");
        // port 1 on the loopback: refused at once, nothing on the network is touched
        std::fs::write(&path, r#"[{"name":"Plug","ip":"127.0.0.1:1","key":"k"}]"#).unwrap();
        let manager = MerossManager::new(&path).unwrap();

        let (device, status) = manager.get_status("127.0.0.1:1").await.unwrap();
        assert_eq!(device, DeviceRef { id: "127.0.0.1:1".into(), name: "Plug".into() });
        assert!(!status.online);
        assert!(manager.toggle("127.0.0.1:1", true).await.is_err());
        assert!(manager.toggle("nope", true).await.unwrap_err().to_string().contains("not found"));
        assert_eq!(manager.get_stats().await.offline, 1);
    }

    #[test]
    fn a_corrupt_device_list_is_an_error() {
        let root = std::env::temp_dir().join("maison-meross-unit").join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("meross-devices.json");
        assert!(MerossManager::new(&path).unwrap().devices.is_empty());
        std::fs::write(&path, "").unwrap();
        assert!(MerossManager::new(&path).is_err());
    }
}
