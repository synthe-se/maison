//! Actions: what a scene runs, one device order each. Kept in the scenes' file too: each
//! renamed field reads its old snake_case name.

use serde::{Deserialize, Serialize};

use crate::{androidtv::AndroidKey, tv::TvKey};

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
/// whatever the current state is (the default).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwitchState {
    On,
    Off,
    #[default]
    Toggle,
}

/// The `action` values are snake_case, the fields camelCase like the rest of the API. Scenes
/// written before kept snake_case fields: each renamed one reads its old name too.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "action",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum Action {
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
    /// routing it to the box first.
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
    /// a toggle or a scene), without their sleep timer: that was for then, not now.
    ClimateOn {
        host: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
}

/// How one action went: the action itself (the apps name it in their words), whether it went,
/// and if not, the refusal's code (`maison_api::codes`: `device_unreachable`, `not_found`, …).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionResult {
    pub action: Action,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// What running a scene answers (`POST /api/scenes/{id}/run`): one entry per action, in the
/// order they ran.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunResponse {
    pub success: bool,
    /// For logs and curl (« 1/2 actions ok »): the apps say their own words.
    pub message: String,
    pub results: Vec<ActionResult>,
}

impl RunResponse {
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
        let off = Action::MerossPower {
            device: "p1".into(),
            state: SwitchState::Off,
        };
        let r = RunResponse::ran(vec![
            ActionResult {
                action: off.clone(),
                ok: true,
                code: None,
            },
            ActionResult {
                action: Action::TvKey { key: TvKey::Mute },
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
        let back: RunResponse = serde_json::from_str(&json).unwrap();
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
        let json = r#"[{"action":"zigbee_power","lamp":"z1","state":"toggle"},{"action":"cover","cover":"c1","command":"position","position":40},{"action":"tv_power","state":"on","switchToBox":true},{"action":"androidtv_key","key":"play_pause"}]"#;
        let a: Vec<Action> = serde_json::from_str(json).unwrap();
        assert_eq!(serde_json::to_string(&a).unwrap(), json);
        let old: Action =
            serde_json::from_str(r#"{"action":"broadlink_code","host":"h","code_id":"c"}"#)
                .unwrap();
        assert_eq!(
            old,
            Action::BroadlinkCode {
                host: "h".into(),
                code_id: "c".into()
            }
        );
        let tv: Action = serde_json::from_str(r#"{"action":"tv_power"}"#).unwrap();
        assert_eq!(
            tv,
            Action::TvPower {
                state: SwitchState::Toggle,
                switch_to_box: true
            }
        );
    }
}
