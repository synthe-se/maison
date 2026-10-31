//! The Android TV box (MECOOL LEAP-S1): what its clients share with the backend
//! (`backend/src/androidtv.rs`).

use serde::{Deserialize, Serialize};

/// The apps worth a shortcut, `(package, label)`, when the box's settings name no favourites
/// (`favoriteApps`): its tile's chips and the remote's pickers offer these.
pub const SHORTCUTS: &[(&str, &str)] = &[
    ("org.smarttube.beta", "SmartTube"),
    ("studio.kahn.iris.tv", "Iris"),
];

/// Why a package of the box is cut: what `backend/src/box_clean.rs`'s table says of each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanWhy {
    /// The vendor's remote-management agent, its factory tests.
    VendorAgent,
    /// Google's home screen and what feeds it.
    HomeScreen,
    /// Google's voice assistant.
    Assistant,
    /// Cast.
    Casting,
    /// An app nobody here opens.
    UnusedApp,
}

/// Where a package of the table stands on the box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanState {
    /// Still running as it came.
    Kept,
    /// Cut, as asked.
    Cleaned,
    /// This box never had it.
    Absent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanedPackage {
    pub package: String,
    pub why: CleanWhy,
    pub state: CleanState,
}

/// `GET /api/androidtv/cleanup`: the table, as the box stands; `launcher`: the home that lets
/// Google's go is installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cleanup {
    pub packages: Vec<CleanedPackage>,
    pub launcher: bool,
}
