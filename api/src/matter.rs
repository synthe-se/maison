//! The shutters over Matter (`/api/matter/covers`): positions are « how open », 100 = open,
//! 0 = closed; `None` until the switch is calibrated.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CoverMotion {
    Stopped,
    Opening,
    Closing,
}

/// Follow the sun: open at sunrise, close at sunset, each shifted by some minutes
/// (« 20 min after sunset »). Kept in the covers' file too.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SunSchedule {
    pub open_at_sunrise: bool,
    pub close_at_sunset: bool,
    /// Minutes after sunrise (negative: before), within ± 3 h.
    pub sunrise_offset_min: i16,
    pub sunset_offset_min: i16,
}

/// A shutter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cover {
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
    pub schedule: SunSchedule,
    /// When the schedule will next open / close it (none without a place or a schedule).
    pub next_open: Option<DateTime<Utc>>,
    pub next_close: Option<DateTime<Utc>>,
    /// The next opening / closing will not happen (they stay in `next_open` / `next_close`).
    pub skip_next_open: bool,
    pub skip_next_close: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Cover {
    /// A motor runs.
    pub fn moving(&self) -> bool {
        matches!(
            self.motion,
            Some(CoverMotion::Opening | CoverMotion::Closing)
        )
    }
}

/// `GET /api/matter/covers`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Covers {
    pub covers: Vec<Cover>,
}

/// One shutter's answer (a move, a rename, a schedule): it, as it now is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OneCover {
    pub cover: Cover,
}

/// `POST /api/matter/covers/{id}/position`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionRequest {
    /// 0 = closed, 100 = fully open.
    pub open_percent: u8,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Answer;

    #[test]
    fn the_covers_round_trip() {
        let json = r#"{"success":true,"covers":[{"id":"c1","name":"Salon","endpoint":1,"online":true,"openPercent":40,"targetOpenPercent":100,"motion":"opening","vendorId":4874,"productId":1,"schedule":{"openAtSunrise":true,"closeAtSunset":true,"sunriseOffsetMin":0,"sunsetOffsetMin":60},"nextOpen":"2026-10-17T06:12:00Z","nextClose":null,"skipNextOpen":false,"skipNextClose":false}]}"#;
        let c: Answer<Covers> = serde_json::from_str(json).unwrap();
        assert!(c.body.covers[0].moving());
        assert_eq!(c.body.covers[0].schedule.sunset_offset_min, 60);
        assert_eq!(serde_json::to_string(&c).unwrap(), json);
        assert_eq!(
            serde_json::to_string(&PositionRequest { open_percent: 30 }).unwrap(),
            r#"{"openPercent":30}"#
        );
    }
}
