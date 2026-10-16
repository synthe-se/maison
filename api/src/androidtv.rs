//! The Android TV box (MECOOL LEAP-S1): what its clients share with the backend
//! (`backend/src/androidtv.rs`).

/// The apps worth a shortcut, `(package, label)`, when the box's settings name no favourites
/// (`favoriteApps`): its tile's chips and the remote's pickers offer these.
pub const SHORTCUTS: &[(&str, &str)] = &[
    ("org.smarttube.beta", "SmartTube"),
    ("studio.kahn.iris.tv", "Iris"),
];
