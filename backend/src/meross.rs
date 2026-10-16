//! The Meross plugs, over their local HTTP protocol (signed JSON packets on `/config`).
//!
//! One reading per plug whatever the number of clients: a plug is asked again only when its
//! last reading is older than `FRESH` (the apps' fastest pace), and callers that come
//! together share one request (each plug's reading sits behind its own lock). What the
//! status read says (on, the electricity, the signal) serves the list and the electricity
//! too. The list as it changes is said on a watch channel (`MerossManager::subscribe`).
//! No MQTT client (AGENTS.md): Mosquitto is only there for the plugs' firmware.

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use maison_api::meross::{Electricity, ElectricityRaw, Plug};
use serde::{Deserialize, Serialize};
use tokio::sync::{watch, Mutex};
use uuid::Uuid;

use crate::{
    error::{AppError, Refusal},
    net,
    routes::DeviceRef,
    store::{self, Corrupt},
    util::{now_ms, refused},
};

const HTTP_TIMEOUT: Duration = Duration::from_secs(5);
/// A plug's answers are a few kilobytes; past this it is not a plug.
const MAX_ANSWER_BYTES: usize = 64 * 1024;
/// How errors name the plug to the client (the detail is logged).
const PLUG: &str = "Plug";
/// A reading younger than this is served as is: the apps' fastest pace for a plug.
const FRESH: Duration = Duration::from_millis(maison_api::paces::MEROSS_STATUS_MS);

/// A plug as `meross-devices.json` keeps it.
#[derive(Debug, Clone, Deserialize)]
pub struct MerossDeviceConfig {
    pub name: String,
    pub ip: String,
    pub key: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MerossStatus {
    pub online: bool,
    pub on: bool,
    pub electricity: Option<MerossStatusElectricity>,
    pub hardware: Option<MerossHardware>,
    pub firmware: Option<MerossFirmware>,
    pub wifi: MerossWifi,
    pub last_update: i64,
}

/// Volts, amperes, watts.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MerossStatusElectricity {
    pub voltage: f64,
    pub current: f64,
    pub power: f64,
}

impl From<&ElectricityRaw> for MerossStatusElectricity {
    fn from(raw: &ElectricityRaw) -> Self {
        Self {
            voltage: raw.volts(),
            current: raw.amperes(),
            power: raw.watts(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MerossHardware {
    pub r#type: String,
    pub version: String,
    pub chip_type: String,
    pub uuid: String,
    /// `macAddress` on the plug's side.
    #[serde(alias = "macAddress")]
    pub mac: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MerossFirmware {
    pub version: String,
    pub compile_time: String,
    pub inner_ip: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MerossWifi {
    pub signal: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MerossConsumptionEntry {
    pub date: String,
    pub time: i64,
    pub value: i32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MerossConsumptionSummary {
    pub days: usize,
    pub total_wh: i32,
    pub total_kwh: f64,
}

#[derive(Debug, Clone)]
pub struct MerossManager {
    client: reqwest::Client,
    devices: Arc<HashMap<String, MerossManagedDevice>>,
    /// The list as it changes (online, on).
    list: Arc<watch::Sender<Vec<Plug>>>,
}

#[derive(Debug, Clone)]
struct MerossManagedDevice {
    config: MerossDeviceConfig,
    /// `host[:port]`, checked once at load (`net::device_address`); none: not a device on
    /// the house's network, said in the log, and never dialled.
    address: Option<String>,
    reading: Arc<Mutex<Reading>>,
}

/// What the plug said last, and when.
#[derive(Debug, Clone, Default)]
struct Reading {
    online: bool,
    on: bool,
    /// When the plug last answered (ms since the epoch); 0: never.
    last_ping: i64,
    /// When its state (`on`) was last read.
    on_at: Option<Instant>,
    system: Option<(MerossHardware, MerossFirmware)>,
    electricity: Option<ElectricityRaw>,
    electricity_at: Option<Instant>,
    signal: Option<i32>,
    /// When the whole status (system, electricity, signal) was last read.
    status_at: Option<Instant>,
}

fn fresh(at: Option<Instant>) -> bool {
    at.is_some_and(|at| at.elapsed() < FRESH)
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
#[serde(rename_all = "camelCase")]
struct MerossRequestHeader<'a> {
    from: String,
    message_id: String,
    method: &'a str,
    namespace: &'a str,
    payload_version: i32,
    sign: String,
    timestamp: i64,
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
    hardware: MerossHardware,
    firmware: MerossFirmware,
}

/// Where a plug says whether it is on: `togglex` (by channel) on recent plugs, `toggle` on
/// older ones.
#[derive(Debug, Deserialize)]
struct MerossDigest {
    toggle: Option<OnOff>,
    togglex: Option<Vec<OnOff>>,
}

#[derive(Debug, Deserialize)]
struct OnOff {
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
    electricity: ElectricityRaw,
}

#[derive(Debug, Deserialize)]
struct MerossConsumptionPayload {
    consumptionx: Option<Vec<MerossConsumptionEntry>>,
}

impl MerossManager {
    pub fn new(config_path: &std::path::Path) -> Result<Self, AppError> {
        let configs = store::read_json::<Vec<MerossDeviceConfig>>(config_path, Corrupt::Fail)?;
        let checked = configs
            .into_iter()
            .map(|config| {
                let address = net::device_address(&config.ip)
                    .inspect_err(|_| {
                        tracing::warn!(plug = %config.name, ip = %config.ip, "meross plug without a LAN address: never dialled");
                    })
                    .ok()
                    .map(|(host, port)| match port {
                        Some(port) => format!("{host}:{port}"),
                        None => host,
                    });
                (config, address)
            })
            .collect();
        Self::with(checked)
    }

    /// The manager over `plugs` and their checked addresses.
    fn with(plugs: Vec<(MerossDeviceConfig, Option<String>)>) -> Result<Self, AppError> {
        let devices = plugs
            .into_iter()
            .map(|(config, address)| {
                let device = MerossManagedDevice {
                    config,
                    address,
                    reading: Arc::default(),
                };
                (device.config.ip.clone(), device)
            })
            .collect::<HashMap<_, _>>();
        let mut list: Vec<Plug> = devices
            .values()
            .map(|device| device.listed(&Reading::default()))
            .collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(Self {
            client: net::device_client(HTTP_TIMEOUT)?,
            devices: Arc::new(devices),
            list: Arc::new(watch::Sender::new(list)),
        })
    }

    /// The list as it changes (a plug comes or goes online, is switched): an event stream's.
    pub fn subscribe(&self) -> watch::Receiver<Vec<Plug>> {
        self.list.subscribe()
    }

    /// Every plug, each read unless it was a moment ago, all at once: a dead one costs its
    /// timeout once, not once per plug after it.
    pub async fn list_devices(&self) -> Vec<Plug> {
        let reads = self.devices.values().map(|device| async move {
            // unreachable is said by the reading the failed read leaves
            if let Err(error) = self.read_on(device).await {
                tracing::debug!(plug = %device.config.name, %error, "meross plug not read");
            }
        });
        futures::future::join_all(reads).await;
        self.list.borrow().clone()
    }

    /// Whether the plug is on (asked to it unless it said so a moment ago).
    pub async fn is_on(&self, device_id: &str) -> Result<bool, AppError> {
        self.read_on(self.get_device(device_id)?).await
    }

    async fn read_on(&self, device: &MerossManagedDevice) -> Result<bool, AppError> {
        let mut reading = device.reading.lock().await;
        if fresh(reading.on_at) {
            return if reading.online {
                Ok(reading.on)
            } else {
                Err(net::unreachable(PLUG, "offline a moment ago"))
            };
        }
        let read = self.get_system_all(device).await;
        // a failed read too is not tried again for a moment: callers behind it hear it
        reading.on_at = Some(Instant::now());
        let read = self.answered(device, &mut reading, read);
        if let Ok(system) = &read {
            reading.on = system.all.on();
        }
        self.publish(device, &reading);
        let system = read?;
        reading.system = Some((system.all.system.hardware, system.all.system.firmware));
        Ok(reading.on)
    }

    /// The plug's whole status (read unless it was a moment ago); an unreachable plug reads
    /// offline, with what it said last.
    pub async fn get_status(&self, device_id: &str) -> Result<(DeviceRef, MerossStatus), AppError> {
        let device = self.get_device(device_id)?;
        let mut reading = device.reading.lock().await;
        if !fresh(reading.status_at) {
            if let Err(error) = self.read_status(device, &mut reading).await {
                tracing::warn!(plug = %device.config.name, ip = %device.config.ip, %error, "meross plug unreachable, said offline");
            }
            self.publish(device, &reading);
        }
        let status = MerossStatus {
            online: reading.online,
            on: reading.on,
            electricity: reading
                .online
                .then(|| {
                    reading
                        .electricity
                        .as_ref()
                        .map(MerossStatusElectricity::from)
                })
                .flatten(),
            hardware: reading
                .online
                .then(|| reading.system.clone())
                .flatten()
                .map(|s| s.0),
            firmware: reading
                .online
                .then(|| reading.system.clone())
                .flatten()
                .map(|s| s.1),
            wifi: MerossWifi {
                signal: reading.online.then_some(reading.signal).flatten(),
            },
            last_update: reading.last_ping,
        };
        Ok((device.reference(), status))
    }

    /// System, electricity and signal: the status's three requests, one reading.
    async fn read_status(
        &self,
        device: &MerossManagedDevice,
        reading: &mut Reading,
    ) -> Result<(), AppError> {
        let read = self.get_system_all(device).await;
        let now = Instant::now();
        // a failed read too is not tried again for a moment: callers behind it hear it
        reading.status_at = Some(now);
        reading.on_at = Some(now);
        let system = self.answered(device, reading, read)?;
        reading.on = system.all.on();
        reading.on_at = Some(now);
        reading.system = Some((system.all.system.hardware, system.all.system.firmware));
        // the plug answered: what it does not say is only missing
        match self.get_electricity_raw(device).await {
            Ok(raw) => {
                reading.electricity = Some(raw);
                reading.electricity_at = Some(now);
            }
            Err(error) => {
                tracing::debug!(plug = %device.config.name, %error, "meross electricity not read")
            }
        }
        match self.get_runtime_signal(device).await {
            Ok(signal) => reading.signal = signal,
            Err(error) => {
                tracing::debug!(plug = %device.config.name, %error, "meross signal not read")
            }
        }
        Ok(())
    }

    /// The plug's electricity, from its status read when that was a moment ago.
    pub async fn get_electricity(
        &self,
        device_id: &str,
    ) -> Result<(DeviceRef, Electricity), AppError> {
        let device = self.get_device(device_id)?;
        let mut reading = device.reading.lock().await;
        let raw = match reading.electricity.clone() {
            Some(raw) if fresh(reading.electricity_at) && reading.online => raw,
            _ => {
                let read = self.get_electricity_raw(device).await;
                let raw = self.answered(device, &mut reading, read);
                self.publish(device, &reading);
                let raw = raw?;
                reading.electricity = Some(raw.clone());
                reading.electricity_at = Some(Instant::now());
                raw
            }
        };
        Ok((device.reference(), raw.said()))
    }

    pub async fn get_consumption(
        &self,
        device_id: &str,
    ) -> Result<
        (
            DeviceRef,
            Vec<MerossConsumptionEntry>,
            MerossConsumptionSummary,
        ),
        AppError,
    > {
        let device = self.get_device(device_id)?;
        let mut reading = device.reading.lock().await;
        let read = self
            .exchange::<MerossConsumptionPayload>(
                device,
                "GET",
                "Appliance.Control.ConsumptionX",
                serde_json::json!({}),
            )
            .await;
        let payload = self.answered(device, &mut reading, read);
        self.publish(device, &reading);
        let consumption = payload?.consumptionx.unwrap_or_default();
        let summary = summarize_consumption(&consumption);
        Ok((device.reference(), consumption, summary))
    }

    /// Switches the plug: `ToggleX` first (recent plugs), the older `Toggle` when the plug
    /// answers that it does not know `ToggleX`. A plug that does not answer is not asked
    /// twice.
    pub async fn toggle(&self, device_id: &str, on: bool) -> Result<DeviceRef, AppError> {
        let device = self.get_device(device_id)?;
        let mut reading = device.reading.lock().await;
        let (toggle_x, toggle) = toggle_payloads(on);
        let sent = match self
            .request(device, "SET", "Appliance.Control.ToggleX", toggle_x)
            .await
        {
            Ok(Reply::Refused) => self
                .exchange::<serde_json::Value>(device, "SET", "Appliance.Control.Toggle", toggle)
                .await
                .map(drop),
            other => other.map(drop),
        };
        let sent = self.answered(device, &mut reading, sent);
        if sent.is_ok() {
            reading.on = on;
            reading.on_at = Some(Instant::now());
        }
        self.publish(device, &reading);
        sent?;
        Ok(device.reference())
    }

    pub async fn set_dnd(&self, device_id: &str, enabled: bool) -> Result<DeviceRef, AppError> {
        let device = self.get_device(device_id)?;
        let mut reading = device.reading.lock().await;
        let payload = serde_json::json!({ "DNDMode": { "mode": u8::from(enabled) } });
        let sent = self
            .exchange::<serde_json::Value>(device, "SET", "Appliance.System.DNDMode", payload)
            .await;
        let sent = self.answered(device, &mut reading, sent);
        self.publish(device, &reading);
        sent?;
        Ok(device.reference())
    }

    fn get_device(&self, device_id: &str) -> Result<&MerossManagedDevice, AppError> {
        self.devices
            .get(device_id)
            .ok_or_else(|| Refusal::NotFound.into())
    }

    /// What a request to the plug did to its reading: an answer, online now; none (not a
    /// refusal), offline.
    fn answered<T>(
        &self,
        device: &MerossManagedDevice,
        reading: &mut Reading,
        result: Result<T, AppError>,
    ) -> Result<T, AppError> {
        match &result {
            Ok(_) => {
                reading.online = true;
                reading.last_ping = now_ms();
            }
            // a refusal is an answer
            Err(AppError::Coded { code, .. }) if *code == Refusal::Unsupported.as_str() => {
                reading.online = true;
                reading.last_ping = now_ms();
            }
            Err(_) => {
                tracing::debug!(plug = %device.config.name, "meross plug did not answer");
                reading.online = false;
            }
        }
        result
    }

    /// `device`'s line of the list as its reading says it, said to the list's listeners
    /// when it changed.
    fn publish(&self, device: &MerossManagedDevice, reading: &Reading) {
        let listed = device.listed(reading);
        self.list
            .send_if_modified(|list| match list.iter_mut().find(|p| p.id == listed.id) {
                Some(kept) if *kept != listed => {
                    *kept = listed;
                    true
                }
                _ => false,
            });
    }

    async fn get_system_all(
        &self,
        device: &MerossManagedDevice,
    ) -> Result<MerossSystemAllPayload, AppError> {
        self.exchange(device, "GET", "Appliance.System.All", serde_json::json!({}))
            .await
    }

    async fn get_runtime_signal(
        &self,
        device: &MerossManagedDevice,
    ) -> Result<Option<i32>, AppError> {
        let payload: MerossRuntimePayload = self
            .exchange(
                device,
                "GET",
                "Appliance.System.Runtime",
                serde_json::json!({}),
            )
            .await?;
        Ok(payload.runtime.signal)
    }

    async fn get_electricity_raw(
        &self,
        device: &MerossManagedDevice,
    ) -> Result<ElectricityRaw, AppError> {
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

    /// One signed request to the plug, its answer's payload read as `T`; a refusal is the
    /// coded `unsupported` here.
    async fn exchange<T: for<'de> Deserialize<'de>>(
        &self,
        device: &MerossManagedDevice,
        method: &'static str,
        namespace: &'static str,
        payload: serde_json::Value,
    ) -> Result<T, AppError> {
        match self.request(device, method, namespace, payload).await? {
            Reply::Payload(payload) => {
                serde_json::from_value(payload).map_err(|error| net::unreachable(PLUG, error))
            }
            Reply::Refused => Err(refused(
                Refusal::Unsupported,
                format!("{} refused {namespace}", device.config.name),
            )),
        }
    }

    /// One signed request to the plug; a refusal (`ERROR`) is an answer.
    async fn request(
        &self,
        device: &MerossManagedDevice,
        method: &'static str,
        namespace: &'static str,
        payload: serde_json::Value,
    ) -> Result<Reply, AppError> {
        let Some(address) = device.address.as_deref() else {
            return Err(net::unreachable(PLUG, "not a device address"));
        };
        let packet = build_packet(address, &device.config.key, namespace, method, payload);
        let response = self
            .client
            .post(format!("http://{address}/config"))
            .json(&packet)
            .send()
            .await
            .map_err(|error| net::unreachable(PLUG, error))?;
        if !response.status().is_success() {
            return Err(net::unreachable(
                PLUG,
                format!("HTTP {}", response.status()),
            ));
        }
        let body = net::read_capped(response, MAX_ANSWER_BYTES, PLUG).await?;
        let packet: MerossPacket =
            serde_json::from_slice(&body).map_err(|error| net::unreachable(PLUG, error))?;
        Ok(if packet.header.method == "ERROR" {
            Reply::Refused
        } else {
            Reply::Payload(packet.payload)
        })
    }
}

/// What a plug answered: its payload, or a protocol `ERROR` (it does not know the order).
enum Reply {
    Payload(serde_json::Value),
    Refused,
}

impl MerossManagedDevice {
    fn reference(&self) -> DeviceRef {
        DeviceRef {
            id: self.config.ip.clone(),
            name: self.config.name.clone(),
        }
    }

    fn listed(&self, reading: &Reading) -> Plug {
        Plug {
            id: self.config.ip.clone(),
            name: self.config.name.clone(),
            ip: self.config.ip.clone(),
            is_online: reading.online,
            is_on: reading.on,
            last_ping: reading.last_ping,
        }
    }
}

impl MerossSystemAll {
    /// Whether the plug is on: its `control` first, then its `digest`; `togglex`, then
    /// `toggle`, in each.
    fn on(&self) -> bool {
        [&self.control, &self.digest]
            .into_iter()
            .flatten()
            .find_map(|said| {
                said.togglex
                    .as_ref()
                    .and_then(|togglex| togglex.first())
                    .or(said.toggle.as_ref())
            })
            .is_some_and(|switch| switch.onoff == 1)
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

fn summarize_consumption(consumption: &[MerossConsumptionEntry]) -> MerossConsumptionSummary {
    let total_wh = consumption.iter().map(|entry| entry.value).sum::<i32>();
    MerossConsumptionSummary {
        days: consumption.len(),
        total_wh,
        total_kwh: ((f64::from(total_wh) / 1000.0) * 100.0).round() / 100.0,
    }
}

fn build_packet(
    address: &str,
    key: &str,
    namespace: &'static str,
    method: &'static str,
    payload: serde_json::Value,
) -> MerossRequestPacket<'static> {
    let message_id = Uuid::new_v4().simple().to_string();
    let timestamp = chrono::Utc::now().timestamp();
    let timestamp_ms = now_ms() % 1000;
    let sign = compute_sign(&message_id, key, timestamp);

    MerossRequestPacket {
        header: MerossRequestHeader {
            from: format!("http://{address}/config"),
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

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;

    use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
    use serde_json::{json, Value};

    use super::*;

    fn config(ip: &str) -> MerossDeviceConfig {
        MerossDeviceConfig {
            name: "Desk plug".into(),
            ip: ip.into(),
            key: "plug-key".into(),
        }
    }

    /// A fake plug: records each packet and answers by namespace; `refuse_togglex` makes it
    /// answer `ToggleX` with an error, as older plugs do; `broken` answers HTTP 500 to all;
    /// `delay` holds every answer that long.
    #[derive(Clone, Default)]
    struct FakePlug {
        received: Arc<StdMutex<Vec<Value>>>,
        refuse_togglex: bool,
        broken: bool,
        delay: Duration,
    }

    impl FakePlug {
        fn namespaces(&self) -> Vec<String> {
            self.received
                .lock()
                .unwrap()
                .iter()
                .map(|p| p["header"]["namespace"].as_str().unwrap().to_string())
                .collect()
        }
    }

    async fn answer(
        State(plug): State<FakePlug>,
        Json(packet): Json<Value>,
    ) -> Result<Json<Value>, StatusCode> {
        plug.received.lock().unwrap().push(packet.clone());
        tokio::time::sleep(plug.delay).await;
        if plug.broken {
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        let namespace = packet["header"]["namespace"].as_str().unwrap_or_default();
        let method = if namespace == "Appliance.Control.ToggleX" && plug.refuse_togglex {
            "ERROR"
        } else {
            "GETACK"
        };
        let payload = match namespace {
            "Appliance.System.All" => json!({ "all": {
                "system": {
                    "hardware": { "type": "mss310", "version": "2.0.0", "chipType": "mt7682", "uuid": "abc", "macAddress": "aa:bb" },
                    "firmware": { "version": "6.1.8", "compileTime": "2021", "innerIp": "127.0.0.1" },
                },
                "digest": { "togglex": [{ "channel": 0, "onoff": 1 }] },
            }}),
            "Appliance.Control.Electricity" => {
                json!({ "electricity": { "channel": 0, "current": 1500, "voltage": 2301, "power": 12000 } })
            }
            "Appliance.System.Runtime" => json!({ "runtime": { "signal": 87 } }),
            "Appliance.Control.ConsumptionX" => json!({ "consumptionx": [
                { "date": "2026-10-01", "time": 1, "value": 1234 },
                { "date": "2026-10-02", "time": 2, "value": 1000 },
            ]}),
            _ => json!({}),
        };
        Ok(Json(
            json!({ "header": { "method": method }, "payload": payload }),
        ))
    }

    /// Starts `plug` on the loopback: its address, which is also its device id.
    async fn serve(plug: FakePlug) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let router = Router::new()
            .route("/config", post(answer))
            .with_state(plug);
        tokio::spawn(async move { axum::serve(listener, router).await });
        address
    }

    /// A manager over plugs on the loopback (no address check: a test's plugs are local).
    fn manager(ips: &[&str]) -> MerossManager {
        MerossManager::with(
            ips.iter()
                .map(|ip| (config(ip), Some((*ip).to_string())))
                .collect(),
        )
        .unwrap()
    }

    fn system(value: Value) -> MerossSystemAll {
        let mut all = json!({
            "system": {
                "hardware": { "type": "mss310", "version": "2", "chipType": "mt7682", "uuid": "u", "macAddress": "m" },
                "firmware": { "version": "6", "compileTime": "t", "innerIp": "10.0.0.9" },
            }
        });
        all.as_object_mut()
            .unwrap()
            .extend(value.as_object().cloned().unwrap());
        serde_json::from_value(all).unwrap()
    }

    #[test]
    fn the_sign_is_md5_of_id_key_and_time() {
        assert_eq!(compute_sign("", "", 0), format!("{:x}", md5::compute("0")));
        assert_eq!(
            compute_sign("abc", "key", 12),
            format!("{:x}", md5::compute("abckey12"))
        );
    }

    #[test]
    fn a_packet_carries_a_signed_header() {
        let packet = serde_json::to_value(build_packet(
            "10.0.0.9",
            "secret",
            "Appliance.System.All",
            "GET",
            json!({}),
        ))
        .unwrap();
        let header = &packet["header"];
        assert_eq!(header["method"], "GET");
        assert_eq!(header["namespace"], "Appliance.System.All");
        assert_eq!(header["from"], "http://10.0.0.9/config");
        assert_eq!(header["payloadVersion"], 1);
        assert!(header["timestampMs"].is_i64());
        let id = header["messageId"].as_str().unwrap();
        assert_eq!(id.len(), 32);
        let sign = compute_sign(id, "secret", header["timestamp"].as_i64().unwrap());
        assert_eq!(header["sign"], sign);
        assert_eq!(packet["payload"], json!({}));
    }

    #[test]
    fn the_on_state_prefers_control_then_digest_and_togglex() {
        let on = |value| system(value).on();
        assert!(on(
            json!({ "control": { "togglex": [{ "onoff": 1 }] }, "digest": { "togglex": [{ "onoff": 0 }] } })
        ));
        assert!(!on(
            json!({ "control": { "toggle": { "onoff": 0 } }, "digest": { "togglex": [{ "onoff": 1 }] } })
        ));
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
    fn consumption_sums_to_rounded_kwh() {
        let entries = [1234, 1000].map(|value| MerossConsumptionEntry {
            date: "2026-10-01".into(),
            time: 0,
            value,
        });
        let summary = summarize_consumption(&entries);
        assert_eq!(
            (summary.days, summary.total_wh, summary.total_kwh),
            (2, 2234, 2.23)
        );
        assert_eq!(summarize_consumption(&[]).total_kwh, 0.0);
    }

    #[tokio::test]
    async fn toggle_sends_a_signed_togglex() {
        let plug = FakePlug::default();
        let ip = serve(plug.clone()).await;
        let manager = manager(&[&ip]);
        let device = manager.toggle(&ip, true).await.unwrap();
        assert_eq!(
            device,
            DeviceRef {
                id: ip.clone(),
                name: "Desk plug".into()
            }
        );
        let received = plug.received.lock().unwrap().clone();
        assert_eq!(received.len(), 1);
        let header = &received[0]["header"];
        assert_eq!(
            (header["method"].as_str(), header["namespace"].as_str()),
            (Some("SET"), Some("Appliance.Control.ToggleX"))
        );
        let signed = format!(
            "{}plug-key{}",
            header["messageId"].as_str().unwrap(),
            header["timestamp"].as_i64().unwrap()
        );
        assert_eq!(header["sign"], format!("{:x}", md5::compute(signed)));
        assert_eq!(
            received[0]["payload"],
            json!({ "togglex": { "channel": 0, "onoff": 1 } })
        );
    }

    #[tokio::test]
    async fn an_older_plug_is_switched_with_toggle() {
        let plug = FakePlug {
            refuse_togglex: true,
            ..FakePlug::default()
        };
        let ip = serve(plug.clone()).await;
        manager(&[&ip]).toggle(&ip, false).await.unwrap();
        assert_eq!(
            plug.namespaces(),
            ["Appliance.Control.ToggleX", "Appliance.Control.Toggle"]
        );
        assert_eq!(
            plug.received.lock().unwrap()[1]["payload"],
            json!({ "channel": 0, "toggle": { "onoff": 0 } })
        );
    }

    /// One status read serves the status, the electricity and the list for a moment:
    /// three readers, one reading.
    #[tokio::test]
    async fn status_electricity_and_the_list_share_one_reading() {
        let plug = FakePlug::default();
        let ip = serve(plug.clone()).await;
        let manager = manager(&[&ip]);
        let mut list = manager.subscribe();
        list.mark_unchanged();

        let (_, status) = manager.get_status(&ip).await.unwrap();
        assert!(status.online && status.on);
        assert_eq!(
            status.electricity,
            Some(MerossStatusElectricity {
                voltage: 230.1,
                current: 1.5,
                power: 12.0
            })
        );
        assert_eq!(status.hardware.as_ref().unwrap().mac, "aa:bb");
        assert_eq!(status.firmware.as_ref().unwrap().inner_ip, "127.0.0.1");
        assert_eq!(status.wifi.signal, Some(87));
        let read = plug.namespaces().len();
        assert_eq!(read, 3, "system, electricity, signal");

        let (_, electricity) = manager.get_electricity(&ip).await.unwrap();
        assert_eq!(
            (
                electricity.voltage.as_str(),
                electricity.current.as_str(),
                electricity.power.as_str()
            ),
            ("230.1V", "1.5A", "12W")
        );
        let listed = manager.list_devices().await;
        assert!(listed[0].is_online && listed[0].is_on);
        for _ in 0..3 {
            manager.get_status(&ip).await.unwrap();
        }
        assert_eq!(plug.namespaces().len(), read, "nothing asked again");
        assert!(list.has_changed().unwrap(), "the list said it came online");
        assert!(list.borrow_and_update()[0].is_on);

        let (_, consumption, summary) = manager.get_consumption(&ip).await.unwrap();
        assert_eq!(consumption.len(), 2);
        assert_eq!(
            (summary.days, summary.total_wh, summary.total_kwh),
            (2, 2234, 2.23)
        );
        manager.set_dnd(&ip, true).await.unwrap();
        assert_eq!(
            plug.namespaces()[read..],
            ["Appliance.Control.ConsumptionX", "Appliance.System.DNDMode"]
        );
    }

    /// Clients asking at once share one request per plug.
    #[tokio::test]
    async fn readers_at_once_share_one_request() {
        let plug = FakePlug {
            delay: Duration::from_millis(200),
            ..FakePlug::default()
        };
        let ip = serve(plug.clone()).await;
        let manager = manager(&[&ip]);
        let (a, b, c) = tokio::join!(
            manager.list_devices(),
            manager.list_devices(),
            manager.is_on(&ip)
        );
        assert!(a[0].is_on && b[0].is_on && c.unwrap());
        assert_eq!(plug.namespaces(), ["Appliance.System.All"]);
    }

    /// A plug that fails (here a 500; a timeout alike) is not asked again with `Toggle`: only
    /// a protocol refusal means « try the older order ».
    #[tokio::test]
    async fn a_failing_plug_is_not_asked_twice() {
        let plug = FakePlug {
            broken: true,
            ..FakePlug::default()
        };
        let ip = serve(plug.clone()).await;
        let manager = manager(&[&ip]);
        let error = manager.toggle(&ip, true).await.unwrap_err();
        assert!(error.to_string().contains("unreachable"), "{error}");
        assert_eq!(plug.received.lock().unwrap().len(), 1);
        let (_, status) = manager.get_status(&ip).await.unwrap();
        assert!(!status.online, "said offline");
        assert!(status.electricity.is_none() && status.hardware.is_none());
    }

    /// The list asks every plug at once: three slow plugs cost one delay, not three.
    #[tokio::test]
    async fn the_list_asks_every_plug_at_once() {
        let delay = Duration::from_millis(400);
        let mut ips = Vec::new();
        for _ in 0..3 {
            ips.push(
                serve(FakePlug {
                    delay,
                    ..FakePlug::default()
                })
                .await,
            );
        }
        let manager = manager(&ips.iter().map(String::as_str).collect::<Vec<_>>());
        let started = Instant::now();
        let listed = manager.list_devices().await;
        assert_eq!(listed.len(), 3);
        assert!(listed.iter().all(|plug| plug.is_online));
        assert!(
            started.elapsed() < delay * 2,
            "asked one by one: {:?}",
            started.elapsed()
        );
    }

    /// An address that is not a device on the house's network is refused at load: the plug
    /// is listed, offline, and never dialled.
    #[tokio::test]
    async fn a_plug_without_a_lan_address_is_never_dialled() {
        let dir = crate::util::test_dir();
        let path = dir.path().join("meross-devices.json");
        std::fs::write(
            &path,
            r#"[{"name":"Plug","ip":"127.0.0.1:1","key":"k"},{"name":"LAN","ip":"192.168.1.20","key":"k"}]"#,
        )
        .unwrap();
        let manager = MerossManager::new(&path).unwrap();
        let addresses: Vec<Option<&str>> = ["127.0.0.1:1", "192.168.1.20"]
            .iter()
            .map(|id| manager.devices[*id].address.as_deref())
            .collect();
        assert_eq!(addresses, [None, Some("192.168.1.20")]);

        let (device, status) = manager.get_status("127.0.0.1:1").await.unwrap();
        assert_eq!(device.name, "Plug");
        assert!(!status.online);
        assert!(manager.toggle("127.0.0.1:1", true).await.is_err());
        assert!(manager.is_on("127.0.0.1:1").await.is_err());
        assert!(matches!(
            manager.toggle("nope", true).await,
            Err(AppError::Coded {
                code: "not_found",
                ..
            })
        ));
    }

    #[test]
    fn a_corrupt_device_list_is_an_error() {
        let dir = crate::util::test_dir();
        let root = dir.path();
        let path = root.join("meross-devices.json");
        assert!(MerossManager::new(&path).unwrap().devices.is_empty());
        std::fs::write(&path, "").unwrap();
        assert!(MerossManager::new(&path).is_err());
    }
}
