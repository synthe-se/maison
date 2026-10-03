//! The Hue lamps of a build without Bluetooth: none, and every command says why.

use crate::{
    config::Config,
    error::AppError,
    lamps::{HueLampView, LampState, LampStats},
};

#[derive(Clone, Default)]
pub struct HueManager;

impl HueManager {
    pub fn new(_config: &Config) -> Result<Self, AppError> {
        Ok(Self)
    }

    pub async fn shutdown(&self) {}

    pub async fn list_lamps(&self) -> Vec<HueLampView> {
        Vec::new()
    }

    pub async fn get_lamp(&self, _lamp_id: &str) -> Option<HueLampView> {
        None
    }

    pub async fn stats(&self) -> LampStats {
        LampStats {
            total: 0,
            connected: 0,
            reachable: 0,
            disabled: true,
            message: Some(UNAVAILABLE.to_string()),
        }
    }

    pub async fn trigger_scan(&self) -> Result<(), AppError> {
        Err(bluetooth_unavailable())
    }

    pub async fn connect_all(&self) {}

    pub async fn disconnect_all(&self) {}

    pub async fn connect_lamp(&self, _lamp_id: &str) -> Result<bool, AppError> {
        Ok(false)
    }

    pub async fn disconnect_lamp(&self, _lamp_id: &str) -> Result<(), AppError> {
        Ok(())
    }

    pub async fn set_power(&self, _lamp_id: &str, _enabled: bool) -> Result<LampState, AppError> {
        Err(bluetooth_unavailable())
    }

    pub async fn set_brightness(&self, _lamp_id: &str, _brightness: u8) -> Result<LampState, AppError> {
        Err(bluetooth_unavailable())
    }

    pub async fn set_temperature(&self, _lamp_id: &str, _temperature: u8) -> Result<LampState, AppError> {
        Err(bluetooth_unavailable())
    }

    pub async fn set_lamp_state(
        &self,
        _lamp_id: &str,
        _is_on: bool,
        _brightness: Option<u8>,
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

const UNAVAILABLE: &str = "Hue Bluetooth support is not built into this binary";

fn bluetooth_unavailable() -> AppError {
    AppError::service_unavailable(UNAVAILABLE)
}
