//! Garenne (Nabaztag) client.
//!
//! The rabbit runs the garenne firmware from the clapier project: it fetches
//! its content over HTTP from the clapier server and accepts commands over
//! UDP port 9998 with a `grn1 ` magic prefix. This module drives that control
//! port directly (no dependency on clapier) and mirrors the daily Tempo
//! colors on the rabbit: today's color on the belly LED (a static color),
//! tomorrow's as the ear position ([`NabaztagManager::follow_tempo`]: when
//! Tempo's answer changes, when the day turns, and every hour).

use std::{path::Path, sync::Arc, time::Duration};

use maison_api::tempo::Color;
use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tracing::{debug, info, warn};

use crate::{
    error::{AppError, Refusal},
    json_config::{Checked, JsonConfig},
    net::{device_client, device_host, unreachable},
    util::{house_time, house_today, non_blank},
    TempoService,
};

/// The colors are pushed again this often whatever happened: a rabbit that rebooted gets
/// them back.
pub const TEMPO_SAFETY_NET: Duration = Duration::from_secs(3600);

/// Garenne control port on the rabbit.
const CTL_PORT: u16 = 9998;
/// Magic prefix of every control datagram.
const CTL_MAGIC: &str = "grn1 ";
/// Rabbit HTTP status endpoint timeout.
const STATUS_TIMEOUT: Duration = Duration::from_secs(5);
/// A Tempo colour on the rabbit.
trait OnRabbit {
    /// Its LED colour, as RGB hex (the belly).
    fn led_hex(self) -> &'static str;
    /// Its ear position (0 to 16: down, level, up).
    fn ear_position(self) -> u8;
}

impl OnRabbit for Color {
    fn led_hex(self) -> &'static str {
        match self {
            Color::Blue => "0000ff",
            Color::White => "ffffff",
            Color::Red => "ff0000",
        }
    }

    fn ear_position(self) -> u8 {
        match self {
            Color::Blue => 0,
            Color::White => 8,
            Color::Red => 16,
        }
    }
}

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
    #[serde(default = "crate::util::default_true")]
    pub tempo_enabled: bool,
}

/// The host must be a LAN device address (see `net::device_host`).
impl Checked for NabaztagConfig {
    fn checked(mut self) -> Result<Self, AppError> {
        self.host = non_blank(self.host)
            .map(|host| device_host(&host))
            .transpose()?;
        Ok(self)
    }
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
    pub today_color: Color,
    pub tomorrow_color: Option<Color>,
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
                // checked like the file: a bad address refuses to start, it is not tried
                env_host: non_blank(env_host.map(str::to_string))
                    .map(|host| device_host(&host))
                    .transpose()?,
                http: device_client(STATUS_TIMEOUT)?,
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

    pub async fn set_config(&self, config: NabaztagConfig) -> Result<NabaztagConfig, AppError> {
        self.inner.config.set(config).await
    }

    /// Checked where it came in (the file by `Checked`, the environment by `new`).
    /// No host yet (`NABAZTAG_HOST` or the file): the coded `not_configured`.
    async fn host(&self) -> Result<String, AppError> {
        self.config()
            .await
            .host
            .ok_or_else(|| Refusal::NotConfigured.into())
    }

    /// Sends one garenne control command to the rabbit. Fire-and-forget by
    /// protocol design; delivery is not acknowledged. A command off the allow-list is the
    /// coded `invalid`.
    pub async fn send_command(&self, command: &str) -> Result<(), AppError> {
        let command = command.trim();
        if !command_is_allowed(command) {
            debug!(command, "rabbit command not allowed");
            return Err(Refusal::Invalid.into());
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
        let Ok(host) = self.host().await else {
            return false;
        };
        match self
            .inner
            .http
            .get(format!("http://{host}/status"))
            .send()
            .await
        {
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
        self.push_tempo(today, tomorrow).await
    }

    /// Mirrors the Tempo colors: today as a static color on the belly LED,
    /// tomorrow as the ear position (see [`tempo_ears`]); `true` with tomorrow's
    /// colour: a forecast.
    pub async fn push_tempo(
        &self,
        today: Color,
        tomorrow: Option<(Color, bool)>,
    ) -> Result<TempoPushResult, AppError> {
        let led_hex = today.led_hex();
        self.send_command(&format!("led {BELLY_LED} {led_hex}"))
            .await?;

        let ears = tomorrow.map(|(color, forecast)| tempo_ears(color, forecast));
        if let Some((left, right)) = ears {
            tokio::time::sleep(PUSH_STEP_DELAY).await;
            self.send_command(&format!("ears {left} {right}")).await?;
        }
        let ear_position = ears.map(|(left, _)| left);
        let tomorrow_forecast = tomorrow.is_some_and(|(_, forecast)| forecast);
        let tomorrow = tomorrow.map(|(color, _)| color);

        info!(
            today = today.as_str(),
            tomorrow = tomorrow.map_or("unknown", Color::as_str),
            led = led_hex,
            ears = ?ear_position,
            "tempo pushed to the rabbit"
        );

        Ok(TempoPushResult {
            today_color: today,
            tomorrow_color: tomorrow,
            led_hex: led_hex.to_string(),
            ear_position,
            tomorrow_forecast,
        })
    }

    /// Mirrors Tempo on the rabbit for good: pushes when Tempo's answer changes (a reader
    /// asked, and today's or tomorrow's colors moved), when the day turns at the house, and
    /// every [`TEMPO_SAFETY_NET`] whatever happened (a rabbit that rebooted). Nothing while
    /// the push is off or no rabbit is set. Ends only when Tempo goes away.
    pub async fn follow_tempo(&self, tempo: &TempoService) {
        let mut answers = tempo.subscribe();
        let mut pushed: Option<TempoColors> = None;
        loop {
            let config = self.config().await;
            if config.tempo_enabled && config.host.is_some() {
                match tempo.rabbit_colors(false).await {
                    Ok(colors) if pushed != Some(colors) => {
                        match self.push_tempo(colors.0, colors.1).await {
                            Ok(_) => pushed = Some(colors),
                            Err(error) => warn!(%error, "tempo push to the rabbit failed"),
                        }
                    }
                    Ok(_) => debug!("tempo: the rabbit has these colors already"),
                    Err(error) => warn!(%error, "tempo colors unavailable for the rabbit"),
                }
            }
            let tomorrow = house_time(house_today() + chrono::Duration::days(1), 0, 1);
            let new_day = (tomorrow - chrono::Utc::now()).to_std().unwrap_or_default();
            tokio::select! {
                changed = answers.changed() => {
                    if changed.is_err() {
                        return;
                    }
                }
                _ = tokio::time::sleep(new_day) => {}
                _ = tokio::time::sleep(TEMPO_SAFETY_NET) => pushed = None,
            }
        }
    }
}

/// Today's color and tomorrow's (`true`: a forecast), as last pushed.
type TempoColors = (Color, Option<(Color, bool)>);

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

/// The ears for tomorrow's colour: both at its position when RTE has published it; for a
/// forecast only the left one, the right one tilted to 4, a position no colour uses: the
/// rabbit is « not sure yet ».
fn tempo_ears(color: Color, forecast: bool) -> (u8, u8) {
    let position = color.ear_position();
    (position, if forecast { FORECAST_EAR } else { position })
}

/// The right ear's position while tomorrow is only forecast.
const FORECAST_EAR: u8 = 4;

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
    fn tempo_colors_on_the_rabbit() {
        assert_eq!(Color::Blue.led_hex(), "0000ff");
        assert_eq!(Color::Red.led_hex(), "ff0000");
        assert_eq!(Color::White.ear_position(), 8);
        assert_eq!(tempo_ears(Color::Red, false), (16, 16));
        assert_eq!(tempo_ears(Color::Blue, true), (0, FORECAST_EAR));
        assert!(
            Color::ALL.iter().all(|c| c.ear_position() != FORECAST_EAR),
            "« not sure » is no colour"
        );
    }

    #[tokio::test]
    async fn the_host_must_be_a_lan_device() {
        let dir = crate::util::test_dir();
        let path = dir.path().join("nabaztag.json");
        let manager = NabaztagManager::new(&path, None).expect("manager");
        for bad in [
            "127.0.0.1",
            "8.8.8.8",
            "192.168.1.2/status",
            "evil.example",
            "192.168.1.2:80",
        ] {
            let config = NabaztagConfig {
                host: Some(bad.into()),
                tempo_enabled: true,
            };
            assert!(manager.set_config(config).await.is_err(), "{bad} accepted");
        }
        let config = NabaztagConfig {
            host: Some(" 192.168.1.40 ".into()),
            tempo_enabled: false,
        };
        manager.set_config(config).await.expect("a LAN address");
        let blank = NabaztagManager::new(&path, Some("  ")).expect("reload");
        assert_eq!(blank.config().await.host.as_deref(), Some("192.168.1.40"));
        assert!(!blank.config().await.tempo_enabled);

        // the environment wins, and is checked like the file
        assert!(NabaztagManager::new(&path, Some("127.0.0.1")).is_err());
        let from_env = NabaztagManager::new(&path, Some("rabbit.local")).expect("env");
        assert_eq!(
            from_env.config().await.host.as_deref(),
            Some("rabbit.local")
        );

        // a hand-edited file is checked on load
        std::fs::write(&path, r#"{"host": "8.8.8.8"}"#).expect("written");
        assert!(NabaztagManager::new(&path, None).is_err());
    }

    #[tokio::test]
    async fn refusals_are_coded() {
        let dir = crate::util::test_dir();
        let manager = NabaztagManager::new(&dir.path().join("nabaztag.json"), None).unwrap();
        let code = |error: AppError| match error {
            AppError::Coded { code, .. } => code,
            other => panic!("uncoded: {other}"),
        };
        assert_eq!(
            code(manager.send_command("conf x").await.unwrap_err()),
            "invalid"
        );
        assert_eq!(
            code(manager.send_command("ping").await.unwrap_err()),
            "not_configured"
        );
    }

    /// The push answer says colors as Tempo says them.
    #[test]
    fn a_push_answer_names_its_colors() {
        let answer = TempoPushResult {
            today_color: Color::Red,
            tomorrow_color: None,
            led_hex: "ff0000".into(),
            ear_position: None,
            tomorrow_forecast: false,
        };
        let json = serde_json::to_value(answer).unwrap();
        assert_eq!(json["todayColor"], "RED");
        assert!(crate::util::casing_offences(&json).is_empty());
    }
}
