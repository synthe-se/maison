//! Garenne (Nabaztag) client.
//!
//! The rabbit runs the garenne firmware from the clapier project: it fetches
//! its content over HTTP from the clapier server and accepts commands over
//! UDP port 9998 with a `grn1 ` magic prefix. This module drives that control
//! port directly (no dependency on clapier) and mirrors the daily Tempo
//! colors on the rabbit: today's color as the breathing LED, tomorrow's as
//! the ear position.

use std::{path::Path, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tracing::{debug, info};

use crate::{
    TempoService,
    broadlink::{device_host, device_http_client, unreachable},
    error::AppError,
    json_config::JsonConfig,
};

/// Garenne control port on the rabbit.
const CTL_PORT: u16 = 9998;
/// Magic prefix of every control datagram.
const CTL_MAGIC: &str = "grn1 ";
/// Rabbit HTTP status endpoint timeout.
const STATUS_TIMEOUT: Duration = Duration::from_secs(5);
/// Pause between the LED and ear commands of a Tempo push, so the firmware
/// processes them as distinct events.
const PUSH_STEP_DELAY: Duration = Duration::from_millis(300);
/// The belly (middle body) LED, where the Tempo color is displayed.
const BELLY_LED: u8 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NabaztagConfig {
    /// Rabbit address on the LAN (private IPv4 or `.local` name).
    pub host: Option<String>,
    /// Whether the daily Tempo colors are mirrored on the rabbit.
    #[serde(default = "default_true")]
    pub tempo_enabled: bool,
}

fn default_true() -> bool {
    true
}

impl Default for NabaztagConfig {
    fn default() -> Self {
        Self {
            host: None,
            tempo_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TempoPushResult {
    pub today_color: String,
    pub tomorrow_color: Option<String>,
    pub led_hex: String,
    pub ear_position: Option<u8>,
    /// Tomorrow's colour is a forecast (RTE has not published it yet).
    pub tomorrow_forecast: bool,
}

#[derive(Clone)]
pub struct NabaztagManager {
    inner: Arc<Inner>,
}

struct Inner {
    config: JsonConfig<NabaztagConfig>,
    /// `NABAZTAG_HOST`: the environment wins over the file, like the rest of the
    /// deployment configuration.
    env_host: Option<String>,
    http: reqwest::Client,
}

impl NabaztagManager {
    pub fn new(config_path: &Path, env_host: Option<&str>) -> Result<Self, AppError> {
        Ok(Self {
            inner: Arc::new(Inner {
                config: JsonConfig::load(config_path)?,
                env_host: crate::util::non_blank(env_host.map(str::to_string)),
                http: device_http_client(STATUS_TIMEOUT)?,
            }),
        })
    }

    pub async fn config(&self) -> NabaztagConfig {
        let mut config = self.inner.config.get().await;
        if self.inner.env_host.is_some() {
            config.host.clone_from(&self.inner.env_host);
        }
        config
    }

    /// The host must be a LAN device address (see `broadlink::device_host`).
    pub async fn set_config(&self, mut config: NabaztagConfig) -> Result<NabaztagConfig, AppError> {
        config.host = crate::util::non_blank(config.host).map(|host| device_host(&host)).transpose()?;
        self.inner.config.set(config).await
    }

    /// Checked again on use: the environment or a hand-edited file never went through
    /// `set_config`.
    async fn host(&self) -> Result<String, AppError> {
        let host = self.config().await.host.ok_or_else(|| {
            AppError::service_unavailable("No Nabaztag host configured (set NABAZTAG_HOST or the nabaztag config)")
        })?;
        device_host(&host)
    }

    /// Sends one garenne control command to the rabbit. Fire-and-forget by
    /// protocol design; delivery is not acknowledged.
    pub async fn send_command(&self, command: &str) -> Result<(), AppError> {
        let command = command.trim();
        if !command_is_allowed(command) {
            return Err(AppError::bad_request(format!("Command not allowed: {command}")));
        }

        let host = self.host().await?;
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        let payload = format!("{CTL_MAGIC}{command}");
        socket
            .send_to(payload.as_bytes(), (host.as_str(), CTL_PORT))
            .await
            .map_err(|error| unreachable("Rabbit", error))?;
        debug!(command, host = %host, "garenne command sent");
        Ok(())
    }

    /// Whether the rabbit answers its `/status` page (its only HTTP route). Only that
    /// yes or no leaves here: the page itself is not passed on.
    pub async fn reachable(&self) -> bool {
        let Ok(host) = self.host().await else { return false };
        match self.inner.http.get(format!("http://{host}/status")).send().await {
            Ok(response) => response.status().is_success(),
            Err(error) => {
                debug!(%error, "rabbit unreachable");
                false
            }
        }
    }

    /// Mirrors today's Tempo colour and tomorrow's on the rabbit: RTE's once published, else
    /// the forecast's when it is at least 80 % sure (marked: see [`tempo_ears`]), else the
    /// ears stay. `force_refresh` asks RTE again.
    pub async fn push_tempo_from(
        &self,
        tempo: &TempoService,
        force_refresh: bool,
    ) -> Result<TempoPushResult, AppError> {
        let (today, tomorrow) = tempo.rabbit_colors(force_refresh).await?;
        if let Some((color, true)) = tomorrow {
            debug!(?color, "forecast colour for tomorrow");
        }
        self.push_tempo(today.as_str(), tomorrow.map(|(c, forecast)| (c.as_str(), forecast))).await
    }

    /// Mirrors the Tempo colors: today as a static color on the belly LED,
    /// tomorrow as the ear position (see [`tempo_ears`]); `true` with tomorrow's
    /// colour: a forecast.
    pub async fn push_tempo(
        &self,
        today: &str,
        tomorrow: Option<(&str, bool)>,
    ) -> Result<TempoPushResult, AppError> {
        let led_hex = tempo_color_hex(today)
            .ok_or_else(|| AppError::bad_request(format!("Unknown Tempo color: {today}")))?;

        self.send_command(&format!("led {BELLY_LED} {led_hex}")).await?;

        let ears = tomorrow.and_then(|(color, forecast)| tempo_ears(color, forecast));
        if let Some((left, right)) = ears {
            tokio::time::sleep(PUSH_STEP_DELAY).await;
            self.send_command(&format!("ears {left} {right}"))
                .await?;
        }
        let ear_position = ears.map(|(left, _)| left);
        let tomorrow_forecast = tomorrow.is_some_and(|(_, forecast)| forecast);
        let tomorrow = tomorrow.map(|(color, _)| color);

        info!(
            today,
            tomorrow = tomorrow.unwrap_or("unknown"),
            led = led_hex,
            ears = ?ear_position,
            "tempo pushed to the rabbit"
        );

        Ok(TempoPushResult {
            today_color: today.to_string(),
            tomorrow_color: tomorrow.map(str::to_string),
            led_hex: led_hex.to_string(),
            ear_position,
            tomorrow_forecast,
        })
    }
}

/// Allow-list mirroring clapier's `vet_ctl`: safe garenne commands only.
/// `conf` is deliberately excluded — it can rewrite the rabbit's flash
/// configuration (including the server address).
fn command_is_allowed(command: &str) -> bool {
    if command.is_empty()
        || command.len() > 120
        || !command
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " ./_-".contains(c))
    {
        return false;
    }

    let verb = command.split_whitespace().next().unwrap_or_default();
    matches!(
        verb,
        "color" | "led" | "ears" | "vol" | "ping" | "dance" | "stop" | "chor" | "play" | "reboot"
    )
}

pub fn tempo_color_hex(color: &str) -> Option<&'static str> {
    match color.to_ascii_uppercase().as_str() {
        "BLUE" | "BLEU" => Some("0000ff"),
        "WHITE" | "BLANC" => Some("ffffff"),
        "RED" | "ROUGE" => Some("ff0000"),
        _ => None,
    }
}

/// The ears for tomorrow's colour: both at its position when RTE has published it; for a
/// forecast only the left one, the right one tilted to 4, a position no colour uses: the
/// rabbit is « not sure yet ».
fn tempo_ears(color: &str, forecast: bool) -> Option<(u8, u8)> {
    let position = tempo_ear_position(color)?;
    Some((position, if forecast { FORECAST_EAR } else { position }))
}

/// The right ear's position while tomorrow is only forecast.
const FORECAST_EAR: u8 = 4;

fn tempo_ear_position(color: &str) -> Option<u8> {
    match color.to_ascii_uppercase().as_str() {
        "BLUE" | "BLEU" => Some(0),
        "WHITE" | "BLANC" => Some(8),
        "RED" | "ROUGE" => Some(16),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_allow_list_accepts_garenne_verbs_only() {
        assert!(command_is_allowed("color 0000ff"));
        assert!(command_is_allowed("ears 8 8"));
        assert!(command_is_allowed("chor /vl/chor/pilot.chor"));
        assert!(!command_is_allowed("conf 10.0.0.1/vl"));
        assert!(!command_is_allowed("color; rm -rf /"));
        assert!(!command_is_allowed(""));
    }

    #[test]
    fn tempo_colors_map_in_both_languages() {
        assert_eq!(tempo_color_hex("BLUE"), Some("0000ff"));
        assert_eq!(tempo_color_hex("bleu"), Some("0000ff"));
        assert_eq!(tempo_color_hex("ROUGE"), Some("ff0000"));
        assert_eq!(tempo_color_hex("mauve"), None);
        assert_eq!(tempo_ear_position("WHITE"), Some(8));
        assert_eq!(tempo_ears("RED", false), Some((16, 16)));
        assert_eq!(tempo_ears("BLUE", true), Some((0, FORECAST_EAR)));
        assert_eq!(tempo_ears("mauve", true), None);
    }

    #[tokio::test]
    async fn the_host_must_be_a_lan_device() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("nabaztag.json");
        let manager = NabaztagManager::new(&path, None).expect("manager");
        for bad in ["127.0.0.1", "8.8.8.8", "192.168.1.2/status", "evil.example", "192.168.1.2:80"] {
            let config = NabaztagConfig { host: Some(bad.into()), tempo_enabled: true };
            assert!(manager.set_config(config).await.is_err(), "{bad} accepted");
        }
        let config = NabaztagConfig { host: Some(" 192.168.1.40 ".into()), tempo_enabled: false };
        manager.set_config(config).await.expect("a LAN address");
        let blank = NabaztagManager::new(&path, Some("  ")).expect("reload");
        assert_eq!(blank.config().await.host.as_deref(), Some("192.168.1.40"));
        assert!(!blank.config().await.tempo_enabled);

        // the environment wins, and is checked on use like the rest
        let from_env = NabaztagManager::new(&path, Some("127.0.0.1")).expect("env");
        assert_eq!(from_env.config().await.host.as_deref(), Some("127.0.0.1"));
        assert!(from_env.send_command("ping").await.is_err());
        assert!(!from_env.reachable().await);
    }
}
