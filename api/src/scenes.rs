//! Scenes (`/api/scenes`): a named list of the remote's actions, one tap away.

use serde::{Deserialize, Serialize};

use crate::remote::IrAction;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    /// A slug, `[a-z0-9-]{1,32}`.
    pub id: String,
    pub name: String,
    /// One of the web's icon names.
    pub icon: String,
    pub actions: Vec<IrAction>,
}

/// `GET /api/scenes`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneList {
    pub scenes: Vec<Scene>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Answer;

    #[test]
    fn the_scenes_round_trip() {
        let json = r#"{"success":true,"scenes":[{"id":"nuit","name":"Nuit","icon":"moon","actions":[{"action":"meross_power","device":"p1","state":"off"}]}]}"#;
        let s: Answer<SceneList> = serde_json::from_str(json).unwrap();
        assert_eq!(s.body.scenes[0].icon, "moon");
        assert_eq!(serde_json::to_string(&s).unwrap(), json);
    }
}
