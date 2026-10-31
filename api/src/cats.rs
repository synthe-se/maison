//! The cats' devices (Tuya): what their clients send (`POST /api/devices/{id}/…`).

use serde::{Deserialize, Serialize};

fn one_portion() -> u8 {
    1
}

/// `feeder/feed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedRequest {
    #[serde(default = "one_portion")]
    pub portion: u8,
}

/// `litter-box/settings`: what is said changes, the rest stays.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LitterBoxSettingsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clean_delay: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sleep_mode: Option<LitterSleepModeSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferences: Option<LitterPreferences>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actions: Option<LitterActions>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LitterSleepModeSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LitterPreferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_lock: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kitten_mode: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lighting: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_sound: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub automatic_homing: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LitterActions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_sand_level: Option<bool>,
    /// An admin's, through `POST …/litter-box/factory-reset`: refused here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_factory_settings: Option<bool>,
}

/// `fountain/uv`: what is said changes, the rest stays.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FountainUvSettingsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{from_str, json, to_value};

    #[test]
    fn a_request_says_only_what_it_changes() {
        let refilled = LitterBoxSettingsRequest {
            actions: Some(LitterActions {
                reset_sand_level: Some(true),
                ..LitterActions::default()
            }),
            ..LitterBoxSettingsRequest::default()
        };
        assert_eq!(
            to_value(&refilled).unwrap(),
            json!({ "actions": { "resetSandLevel": true } })
        );
        let uv = FountainUvSettingsRequest {
            enabled: Some(false),
            runtime: None,
        };
        assert_eq!(to_value(&uv).unwrap(), json!({ "enabled": false }));
    }

    #[test]
    fn a_feed_defaults_to_one_portion() {
        let feed: FeedRequest = from_str("{}").unwrap();
        assert_eq!(feed.portion, 1);
        assert_eq!(to_value(&feed).unwrap(), json!({ "portion": 1 }));
    }
}
