//! The Philips TV: what its clients send (`/api/tv`).

use serde::{Deserialize, Serialize};

use crate::actions::{SwitchState, default_true};

/// `POST /api/tv/power`, as `Action::TvPower` powers it, with the same defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PowerRequest {
    #[serde(default)]
    pub state: SwitchState,
    /// Also route the set to the box's HDMI input, which is almost always what
    /// is wanted: the TV otherwise comes back on whatever source it was left on.
    #[serde(default = "default_true")]
    pub switch_to_box: bool,
}

/// `PUT /api/tv/volume`: what is said changes, the rest stays.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
}

/// `POST /api/tv/key`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyRequest {
    pub key: TvKey,
}

/// `POST /api/tv/ambilight`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AmbilightRequest {
    #[serde(default)]
    pub state: SwitchState,
}

/// Remote-control keys accepted by the TV's `/6/input/key`. An enum: an unknown key name is a
/// rejected request, and a rejected request is a step towards a dead server (the TV's
/// JointSPACE dies on what it does not implement).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TvKey {
    Standby,
    Back,
    Home,
    Source,
    WatchTv,
    Confirm,
    CursorUp,
    CursorDown,
    CursorLeft,
    CursorRight,
    VolumeUp,
    VolumeDown,
    Mute,
    ChannelStepUp,
    ChannelStepDown,
    PlayPause,
    Pause,
    Stop,
    FastForward,
    Rewind,
    Next,
    Previous,
    Info,
    Options,
    Subtitle,
    Teletext,
    AmbilightOnOff,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{from_str, json, to_value};

    #[test]
    fn its_bodies_read_as_the_routes_always_did() {
        let on = PowerRequest {
            state: SwitchState::On,
            switch_to_box: true,
        };
        assert_eq!(
            to_value(&on).unwrap(),
            json!({ "state": "on", "switchToBox": true })
        );
        let bare: PowerRequest = from_str("{}").unwrap();
        assert_eq!(
            (bare.state, bare.switch_to_box),
            (SwitchState::Toggle, true)
        );
        let level = VolumeRequest {
            level: Some(12),
            muted: None,
        };
        assert_eq!(to_value(&level).unwrap(), json!({ "level": 12 }));
        let key = KeyRequest {
            key: TvKey::CursorUp,
        };
        assert_eq!(to_value(key).unwrap(), json!({ "key": "cursor_up" }));
        let bare: AmbilightRequest = from_str("{}").unwrap();
        assert_eq!(bare.state, SwitchState::Toggle);
    }
}
