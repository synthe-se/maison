//! Scenes: a named list of IR binding actions (« Cinéma »: TV on, lamps dimmed), run from
//! the dashboard or bound to a remote key (`{"action": "scene", "scene": "<id>"}`). A scene
//! is no new engine: its actions are `ir::IrAction`s, run by `ir::run_actions` like a key's.
//!
//! Kept in `scenes.json`, a list in the order the dashboard shows them:
//! ```json
//! [{ "id": "cinema", "name": "Cinéma", "icon": "film",
//!    "actions": [{ "action": "tv_power", "state": "on" }] }]
//! ```

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::{
    error::AppError,
    ir::{self, IrAction},
    store::{self, Access, Corrupt},
};

/// A scene is something one taps: past twenty actions it is a script.
pub const MAX_ACTIONS: usize = 20;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub id: String,
    pub name: String,
    /// One of the web's icon names.
    pub icon: String,
    pub actions: Vec<IrAction>,
}

/// An id as kept: 1 to 32 of `a-z`, `0-9` and `-` (it goes in URLs and in the keymap).
pub fn valid_id(id: &str) -> bool {
    (1..=32).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// An icon name as the web spells them: 1 to 32 of `a-z` and `-`.
fn valid_icon(icon: &str) -> bool {
    (1..=32).contains(&icon.len()) && icon.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
}

#[derive(Clone)]
pub struct ScenesManager {
    path: Arc<PathBuf>,
    scenes: Arc<RwLock<Vec<Scene>>>,
}

impl ScenesManager {
    /// Missing file = no scenes (the web offers templates); an unreadable one is a startup
    /// error, like the keymap.
    pub fn new(path: &Path) -> Result<Self, AppError> {
        let scenes: Vec<Scene> = store::read_json(path, Corrupt::Fail)?;
        Ok(Self { path: Arc::new(path.to_path_buf()), scenes: Arc::new(RwLock::new(scenes)) })
    }

    /// In the order they were made.
    pub async fn list(&self) -> Vec<Scene> {
        self.scenes.read().await.clone()
    }

    pub async fn get(&self, id: &str) -> Option<Scene> {
        self.scenes.read().await.iter().find(|scene| scene.id == id).cloned()
    }

    /// Creates the scene `id` (at the end) or replaces it (in its place). Saved first, then
    /// kept: a failed save leaves memory as it is on disk.
    pub async fn put(&self, id: &str, name: &str, icon: &str, actions: Vec<IrAction>) -> Result<Scene, AppError> {
        let scene = checked(id, name, icon, actions)?;
        let mut scenes = self.scenes.write().await;
        let mut next = scenes.clone();
        match next.iter_mut().find(|kept| kept.id == scene.id) {
            Some(kept) => *kept = scene.clone(),
            None => next.push(scene.clone()),
        }
        store::write_json_async(&self.path, &next, Access::Shared).await?;
        *scenes = next;
        Ok(scene)
    }

    /// Returns `true` when the scene existed. Keys bound to it then fail at their press,
    /// saying so (`ir::run_actions`).
    pub async fn remove(&self, id: &str) -> Result<bool, AppError> {
        let mut scenes = self.scenes.write().await;
        if !scenes.iter().any(|scene| scene.id == id) {
            return Ok(false);
        }
        let next: Vec<Scene> = scenes.iter().filter(|scene| scene.id != id).cloned().collect();
        store::write_json_async(&self.path, &next, Access::Shared).await?;
        *scenes = next;
        Ok(true)
    }
}

/// The scene as kept, or why not.
fn checked(id: &str, name: &str, icon: &str, actions: Vec<IrAction>) -> Result<Scene, AppError> {
    if !valid_id(id) {
        return Err(AppError::bad_request("A scene id is 1 to 32 of a-z, 0-9 and -"));
    }
    let name = crate::util::name(name)?.to_string();
    if !valid_icon(icon) {
        return Err(AppError::bad_request("An icon name is 1 to 32 of a-z and -"));
    }
    if actions.is_empty() || actions.len() > MAX_ACTIONS {
        return Err(AppError::bad_request(format!("A scene holds 1 to {MAX_ACTIONS} actions")));
    }
    if actions.iter().any(|action| matches!(action, IrAction::Scene { .. })) {
        return Err(AppError::coded(StatusCode::BAD_REQUEST, "nested_scene", "A scene cannot run another scene"));
    }
    ir::validate_actions(&actions).map_err(AppError::bad_request)?;
    Ok(Scene { id: id.to_string(), name, icon: icon.to_string(), actions })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pings(n: usize) -> Vec<IrAction> {
        vec![IrAction::Nabaztag { command: "ping".into() }; n]
    }

    fn ping() -> Vec<IrAction> {
        pings(1)
    }

    fn manager() -> (tempfile::TempDir, ScenesManager) {
        let dir = crate::util::test_dir();
        let manager = ScenesManager::new(&dir.path().join("scenes.json")).expect("no scenes yet");
        (dir, manager)
    }

    #[test]
    fn ids_and_icons_follow_their_alphabet() {
        assert!(valid_id("cinema") && valid_id("soir-2") && valid_id(&"a".repeat(32)));
        assert!(!valid_id("") && !valid_id("Cinéma") && !valid_id("a_b") && !valid_id(&"a".repeat(33)));
        assert!(valid_icon("film") && valid_icon("moon-star"));
        assert!(!valid_icon("film2") && !valid_icon("") && !valid_icon("Film") && !valid_icon(&"a".repeat(33)));
    }

    #[test]
    fn what_a_scene_may_hold() {
        assert!(checked("soir", " Soirée ", "moon", ping()).is_ok_and(|s| s.name == "Soirée"));
        assert!(checked("Soir", "Soirée", "moon", ping()).is_err(), "id");
        assert!(checked("soir", "  ", "moon", ping()).is_err(), "name");
        assert!(checked("soir", "Soirée", "Moon!", ping()).is_err(), "icon");
        assert!(checked("soir", "Soirée", "moon", Vec::new()).is_err(), "no action");
        assert!(checked("soir", "Soirée", "moon", pings(MAX_ACTIONS)).is_ok());
        assert!(checked("soir", "Soirée", "moon", pings(MAX_ACTIONS + 1)).is_err(), "too many");
        let typo = vec![IrAction::AndroidTvApp { package: "not a package".into(), ensure_tv_on: true }];
        assert!(checked("soir", "Soirée", "moon", typo).is_err(), "validated as a key's actions");
        let nested = vec![IrAction::Scene { scene: "other".into() }];
        assert!(matches!(
            checked("soir", "Soirée", "moon", nested),
            Err(AppError::Coded { code: "nested_scene", .. })
        ));
    }

    #[tokio::test]
    async fn saved_in_order_replaced_in_place_and_removed() {
        let (dir, manager) = manager();
        manager.put("a", "A", "sun", ping()).await.unwrap();
        manager.put("b", "B", "moon", ping()).await.unwrap();
        manager.put("a", "A again", "sun", ping()).await.unwrap();
        let reloaded = ScenesManager::new(&dir.path().join("scenes.json")).unwrap();
        let names: Vec<String> = reloaded.list().await.into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["A again", "B"]);
        assert!(reloaded.remove("a").await.unwrap());
        assert!(!reloaded.remove("a").await.unwrap());
        let reloaded = ScenesManager::new(&dir.path().join("scenes.json")).unwrap();
        assert!(reloaded.get("a").await.is_none() && reloaded.get("b").await.is_some());
    }

    /// A failed save must not leave a scene that exists in memory only.
    #[tokio::test]
    async fn a_failed_save_changes_nothing() {
        let (dir, manager) = manager();
        std::fs::create_dir(dir.path().join("scenes.json")).expect("a directory where the file goes");
        assert!(manager.put("a", "A", "sun", ping()).await.is_err());
        assert!(manager.get("a").await.is_none());
    }

    #[test]
    fn an_empty_file_is_an_error_not_no_scenes() {
        let dir = crate::util::test_dir();
        let path = dir.path().join("scenes.json");
        std::fs::write(&path, "").unwrap();
        assert!(ScenesManager::new(&path).is_err());
    }
}
