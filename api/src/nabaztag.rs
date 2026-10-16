//! The rabbit (garenne firmware, « Garenne » in the house): what its clients share with the
//! backend (`backend/src/nabaztag.rs`).

/// The tai chi choreography: the rabbit's button on the dashboard and the remote's « Taichi »
/// key send it (`POST /api/nabaztag/ctl`, checked by the backend's allow-list).
pub const TAICHI: &str = "chor taichi";
