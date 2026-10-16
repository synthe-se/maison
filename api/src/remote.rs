//! The remote's actions (`/api/ir`): what a key of the set-top box's remote, a scene or the
//! configurator's test runs, and the keys it may send to the TV and the box. Kept in the
//! keymap and the scenes' files too: each renamed field reads its old snake_case name.

use serde::{Deserialize, Serialize};

/// serde's `default` for flags that are on unless said otherwise.
pub fn default_true() -> bool {
    true
}

/// What a `cover` action asks of a shutter (Matter's own commands, said as the API says them).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverOrder {
    Open,
    Close,
    Stop,
    /// To the action's `position`, an open percentage (0 closed, 100 open).
    Position,
}

/// What a switch-like action does to the device: force a state, or flip
/// whatever the current state is — the remote-control default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwitchState {
    On,
    Off,
    #[default]
    Toggle,
}

/// The `action` values are snake_case, the fields camelCase like the rest of the API. The
/// keymap and scenes written before kept snake_case fields: each renamed one reads its old
/// name too.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "action",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum IrAction {
    Nabaztag {
        command: String,
    },
    ZigbeePower {
        lamp: String,
        #[serde(default)]
        state: SwitchState,
    },
    ZigbeeBrightness {
        lamp: String,
        brightness: u8,
    },
    /// A Hue Bluetooth lamp, as `zigbee_power` drives a Zigbee one.
    HuePower {
        lamp: String,
        #[serde(default)]
        state: SwitchState,
    },
    /// A Hue Bluetooth lamp's brightness (1–100).
    HueBrightness {
        lamp: String,
        brightness: u8,
    },
    /// A Matter shutter: opened, closed, stopped or sent to `position` (an open percentage,
    /// 0 closed, 100 open; only with `position`).
    Cover {
        cover: String,
        command: CoverOrder,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        position: Option<u8>,
    },
    BroadlinkCode {
        host: String,
        #[serde(alias = "code_id")]
        code_id: String,
    },
    MerossPower {
        device: String,
        #[serde(default)]
        state: SwitchState,
    },
    /// Powers the Philips TV (over IR, through the Broadlink blaster). `switchToBox` also
    /// routes the set to the Android box's HDMI input on power-on, since the TV otherwise
    /// comes back on whatever source it was last left on.
    TvPower {
        #[serde(default)]
        state: SwitchState,
        #[serde(default = "default_true", alias = "switch_to_box")]
        switch_to_box: bool,
    },
    /// Sends one remote-control key to the TV.
    TvKey {
        key: TvKey,
    },
    /// Absolute volume on the TV, preferred over repeated volume keys.
    TvVolume {
        level: u8,
    },
    TvAmbilight {
        #[serde(default)]
        state: SwitchState,
    },
    /// Launches an app on the Android TV box, powering the television on and
    /// routing it to the box first — a remote button that lights up a dark
    /// room's screen is the whole point.
    #[serde(rename = "androidtv_app")]
    AndroidTvApp {
        package: String,
        #[serde(default = "default_true", alias = "ensure_tv_on")]
        ensure_tv_on: bool,
    },
    /// Sends one key to the Android TV box (D-pad, media, volume).
    #[serde(rename = "androidtv_key")]
    AndroidTvKey {
        key: AndroidKey,
    },
    /// Toggles the Mitsubishi AC through the Broadlink blaster: if the last
    /// commanded state left it on, sends `state-off`; otherwise sends
    /// `onCommand` (a structured `state-…` command, e.g.
    /// `state-cool-16-fan-4-vane-swing`). IR is one-way: the last commanded state is the
    /// best approximation of the AC's.
    ClimateToggle {
        host: String,
        #[serde(alias = "on_command")]
        on_command: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
    /// Switches the Mitsubishi AC off (`state-off`), recorded as off like any order.
    ClimateOff {
        host: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
    /// Switches the Mitsubishi AC back on with the last settings it was given (from its page,
    /// a toggle or a binding), without their sleep timer: that was for then, not now.
    ClimateOn {
        host: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
    /// Runs a saved scene's actions (`scenes.rs`). Only from a key or a test: a scene may
    /// not hold another scene.
    Scene {
        scene: String,
    },
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

/// Remote-control keys, mapped to Android keycodes. An enum rather than a
/// string so no caller can smuggle shell syntax into `input keyevent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AndroidKey {
    Up,
    Down,
    Left,
    Right,
    Ok,
    Back,
    Home,
    Menu,
    Search,
    VolumeUp,
    VolumeDown,
    Mute,
    PlayPause,
    Play,
    Pause,
    Stop,
    Next,
    Previous,
    Rewind,
    FastForward,
    ChannelUp,
    ChannelDown,
    Power,
    Sleep,
    Wakeup,
}

/// How one action went, in the order they ran (a scene run from a key runs the scene's
/// actions): the action itself (the apps name it in their words), whether it went, and if not,
/// the refusal's code (`maison_api::codes`: `device_unreachable`, `not_found`, …).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionResult {
    pub action: IrAction,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// What running actions answers (`POST /api/scenes/{id}/run`, `/api/ir/test`, and a key press
/// with no results, its actions running after): one entry per action, in the order they ran.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyResponse {
    pub success: bool,
    /// For logs and curl (« 1/2 actions ok »): the apps say their own words.
    pub message: String,
    pub results: Vec<ActionResult>,
}

impl KeyResponse {
    /// Nothing ran (yet): said why.
    pub fn nothing(message: String) -> Self {
        Self {
            success: true,
            message,
            results: Vec::new(),
        }
    }

    /// Actions ran: how many went.
    pub fn ran(results: Vec<ActionResult>) -> Self {
        let failures = results.iter().filter(|r| !r.ok).count();
        Self {
            success: failures == 0,
            message: format!("{}/{} actions ok", results.len() - failures, results.len()),
            results,
        }
    }

    /// The results that did not go.
    pub fn failures(&self) -> impl Iterator<Item = &ActionResult> {
        self.results.iter().filter(|r| !r.ok)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_says_each_action() {
        let off = IrAction::MerossPower {
            device: "p1".into(),
            state: SwitchState::Off,
        };
        let r = KeyResponse::ran(vec![
            ActionResult {
                action: off.clone(),
                ok: true,
                code: None,
            },
            ActionResult {
                action: IrAction::TvKey { key: TvKey::Mute },
                ok: false,
                code: Some("device_unreachable".into()),
            },
        ]);
        assert!(!r.success);
        assert_eq!(r.message, "1/2 actions ok");
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(
            json,
            r#"{"success":false,"message":"1/2 actions ok","results":[{"action":{"action":"meross_power","device":"p1","state":"off"},"ok":true},{"action":{"action":"tv_key","key":"mute"},"ok":false,"code":"device_unreachable"}]}"#
        );
        let back: KeyResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r);
        assert_eq!(
            back.failures()
                .map(|f| f.code.as_deref())
                .collect::<Vec<_>>(),
            [Some("device_unreachable")]
        );
    }

    #[test]
    fn actions_round_trip_and_old_names_load() {
        let json = r#"[{"action":"zigbee_power","lamp":"z1","state":"toggle"},{"action":"cover","cover":"c1","command":"position","position":40},{"action":"tv_power","state":"on","switchToBox":true},{"action":"androidtv_key","key":"play_pause"},{"action":"scene","scene":"nuit"}]"#;
        let a: Vec<IrAction> = serde_json::from_str(json).unwrap();
        assert_eq!(serde_json::to_string(&a).unwrap(), json);
        let old: IrAction =
            serde_json::from_str(r#"{"action":"broadlink_code","host":"h","code_id":"c"}"#)
                .unwrap();
        assert_eq!(
            old,
            IrAction::BroadlinkCode {
                host: "h".into(),
                code_id: "c".into()
            }
        );
        let tv: IrAction = serde_json::from_str(r#"{"action":"tv_power"}"#).unwrap();
        assert_eq!(
            tv,
            IrAction::TvPower {
                state: SwitchState::Toggle,
                switch_to_box: true
            }
        );
        let key: TvKey = serde_json::from_str(r#""watch_tv""#).unwrap();
        assert_eq!(key, TvKey::WatchTv);
    }
}
