//! The rabbit (garenne firmware, « Garenne » in the house): what its clients share with the
//! backend (`backend/src/nabaztag.rs`).

use serde::{Deserialize, Serialize};

/// The tai chi choreography: the rabbit's button on the dashboard and the remote's « Taichi »
/// key send it (`POST /api/nabaztag/ctl`, checked by the backend's allow-list).
pub const TAICHI: &str = "chor taichi";

/// `POST /api/nabaztag/ctl`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRequest {
    pub command: String,
}

/// `POST /api/nabaztag/tempo/push`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TempoPushRequest {
    #[serde(default)]
    pub force_refresh: bool,
}
