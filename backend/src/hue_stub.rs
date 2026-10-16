//! The Hue lamps of a build without Bluetooth: none, and every command refused, coded
//! `unsupported` (the server cannot speak Bluetooth).

use crate::{
    config::Config,
    error::{AppError, Refusal},
    lamps::{HueLamp, LampState, LampStats},
};

#[derive(Clone, Default)]
pub struct HueManager;

impl HueManager {
    pub fn new(_config: &Config) -> Result<Self, AppError> {
        Ok(Self)
    }

    pub async fn shutdown(&self) {}

    pub async fn list_lamps(&self) -> Vec<HueLamp> {
        Vec::new()
    }

    pub async fn get_lamp(&self, _lamp_id: &str) -> Option<HueLamp> {
        None
    }

    pub async fn stats(&self) -> LampStats {
        LampStats::of([], true, Some(UNAVAILABLE.to_string()))
    }

    pub async fn trigger_scan(&self) -> Result<(), AppError> {
        Err(bluetooth_unavailable())
    }

    pub async fn set_power(&self, _lamp_id: &str, _enabled: bool) -> Result<LampState, AppError> {
        Err(bluetooth_unavailable())
    }

    pub async fn set_brightness(
        &self,
        _lamp_id: &str,
        _brightness: u8,
    ) -> Result<LampState, AppError> {
        Err(bluetooth_unavailable())
    }

    pub async fn set_temperature(
        &self,
        _lamp_id: &str,
        _temperature: u8,
    ) -> Result<LampState, AppError> {
        Err(bluetooth_unavailable())
    }

    pub async fn rename_lamp(&self, _lamp_id: &str, _name: &str) -> Result<(), AppError> {
        Err(bluetooth_unavailable())
    }

    pub async fn blacklist_lamp(&self, _lamp_id: &str) -> Result<bool, AppError> {
        Err(bluetooth_unavailable())
    }
}

/// What the stats say (`message`), for the logs and curl.
const UNAVAILABLE: &str = "Hue Bluetooth support is not built into this binary";

fn bluetooth_unavailable() -> AppError {
    Refusal::Unsupported.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn without_bluetooth_there_is_no_lamp_and_every_order_is_unsupported() {
        let hue =
            HueManager::new(&Config::defaults(crate::util::test_dir().path().into())).unwrap();
        assert!(hue.list_lamps().await.is_empty() && hue.get_lamp("a").await.is_none());
        let stats = hue.stats().await;
        assert!(stats.disabled && stats.total == 0 && stats.message.is_some());
        let unsupported = |error: AppError| {
            matches!(
                error,
                AppError::Coded {
                    code: "unsupported",
                    ..
                }
            )
        };
        assert!(unsupported(hue.set_power("a", true).await.unwrap_err()));
        assert!(unsupported(hue.set_brightness("a", 5).await.unwrap_err()));
        assert!(unsupported(hue.set_temperature("a", 5).await.unwrap_err()));
        assert!(unsupported(hue.rename_lamp("a", "b").await.unwrap_err()));
        assert!(unsupported(hue.blacklist_lamp("a").await.unwrap_err()));
        assert!(unsupported(hue.trigger_scan().await.unwrap_err()));
    }
}
