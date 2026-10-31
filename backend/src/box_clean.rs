//! What the living-room box does not need, said once, and what its home is told of the house.
//!
//! Every package is cut for user 0 only: its APK stays on the system partition and all of it comes
//! back with one `restore`. Nothing here takes a name from a client: the table is the whole
//! surface. Android writes a package's state some seconds after the command: a reboot right after
//! brings the package back. `com.droidlogic` (Amlogic) re-enables the assistant, the
//! recommendations and Google's speech at each boot, whence `Remove` for those: its call then
//! finds nothing.

use std::{collections::HashSet, time::Duration};

use maison_api::{
    androidtv::{CleanState, CleanWhy, CleanedPackage, Cleanup},
    sun::{NextSun, Side},
    tempo::Today,
};

use crate::{
    androidtv::AndroidTvManager,
    error::{AppError, Refusal},
    util::refused,
    AppState,
};

/// The home that lets Google's go (`box/launcher`): never cut Google's without it.
const LAUNCHER: &str = "maison.box";
const LAUNCHER_HOME: &str = "maison.box/.Home";
const HOUSE_RECEIVER: &str = "maison.box/.HouseReceiver";
/// Under the two hours the launcher still trusts a word (`HOUSE_FRESH_MS`).
pub const HOUSE_EVERY: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum How {
    /// `pm disable-user`.
    Disable,
    /// `pm uninstall -k --user 0`: for what `com.droidlogic` re-enables at boot.
    Remove,
}

use CleanWhy::{Assistant, Casting, HomeScreen, UnusedApp, VendorAgent};
use How::{Disable, Remove};

const TABLE: &[(&str, CleanWhy, How)] = &[
    // MQTT remote management running as the system user (scripts, remote control, factory reset,
    // screen and logs), retrying its dead broker every second; the factory's test apps
    ("com.oemservice", VendorAgent, Disable),
    ("com.oem.qa", VendorAgent, Disable),
    ("com.sdt.producttest", VendorAgent, Disable),
    ("com.google.android.tvlauncher", HomeScreen, Disable),
    ("com.sdt.customization", HomeScreen, Disable),
    ("com.google.android.tvrecommendations", HomeScreen, Remove),
    ("com.google.android.katniss", Assistant, Remove),
    ("com.google.android.apps.mediashell", Casting, Disable),
    ("com.netflix.ninja", UnusedApp, Disable),
    ("com.netflix.tokenmanager", UnusedApp, Disable),
    ("com.amazon.amazonvideo.livingroom", UnusedApp, Disable),
    ("com.google.android.youtube.tv", UnusedApp, Disable),
    ("com.google.android.youtube.tvmusic", UnusedApp, Disable),
    ("com.google.android.play.games", UnusedApp, Disable),
    ("com.google.android.videos", UnusedApp, Disable),
    ("com.google.android.marvin.talkback", UnusedApp, Disable),
    ("com.google.android.feedback", UnusedApp, Disable),
    ("com.google.android.tts", UnusedApp, Remove),
    ("com.android.gallery3d", UnusedApp, Disable),
    ("com.android.printspooler", UnusedApp, Disable),
    (
        "com.google.android.syncadapters.calendar",
        UnusedApp,
        Disable,
    ),
];

const LIST: &str = "pm list packages -d; echo ---; pm list packages";

fn packages(listing: &str) -> HashSet<&str> {
    listing
        .lines()
        .filter_map(|line| line.trim().strip_prefix("package:"))
        .collect()
}

/// The table as `LIST` answers: the disabled ones, a line, every one the user has.
fn state_of(listing: &str) -> Cleanup {
    let (disabled, installed) = listing.split_once("---").unwrap_or(("", listing));
    let (disabled, installed) = (packages(disabled), packages(installed));
    let packages = TABLE
        .iter()
        .map(|&(package, why, how)| CleanedPackage {
            package: package.to_string(),
            why,
            state: match how {
                _ if disabled.contains(package) => CleanState::Cleaned,
                _ if installed.contains(package) => CleanState::Kept,
                How::Remove => CleanState::Cleaned,
                How::Disable => CleanState::Absent,
            },
        })
        .collect();
    Cleanup {
        packages,
        launcher: installed.contains(LAUNCHER),
    }
}

fn clean_command(package: &str, how: How) -> String {
    match how {
        How::Disable => format!("pm disable-user --user 0 {package}"),
        How::Remove => format!("pm uninstall -k --user 0 {package}"),
    }
}

fn restore_command(package: &str, how: How) -> String {
    match how {
        How::Disable => format!("pm enable {package}"),
        How::Remove => format!("cmd package install-existing --user 0 {package}"),
    }
}

pub async fn read(box_: &AndroidTvManager) -> Result<Cleanup, AppError> {
    Ok(state_of(&box_.shell(LIST).await?))
}

/// The commands that bring the box to `want`, from where it stands.
fn commands(now: &Cleanup, want: CleanState) -> Vec<String> {
    TABLE
        .iter()
        .zip(&now.packages)
        .filter(|(_, stands)| stands.state != CleanState::Absent && stands.state != want)
        .map(|(&(package, _, how), _)| match want {
            CleanState::Cleaned => clean_command(package, how),
            _ => restore_command(package, how),
        })
        .collect()
}

async fn run(box_: &AndroidTvManager, commands: Vec<String>) -> Result<Cleanup, AppError> {
    // one by one: what the box refuses stays visible in the state read after, the rest goes on
    for command in commands {
        let said = box_.shell(&command).await?;
        tracing::info!(command, said = said.trim(), "box cleanup");
    }
    read(box_).await
}

/// Cuts the table. Never without the new home: a box left without one shows a blank screen.
pub async fn clean(box_: &AndroidTvManager) -> Result<Cleanup, AppError> {
    let now = read(box_).await?;
    if !now.launcher {
        return Err(refused(
            Refusal::NotConfigured,
            "the box's home is not installed: Google's stays",
        ));
    }
    box_.shell(&format!("cmd package set-home-activity {LAUNCHER_HOME}"))
        .await?;
    run(box_, commands(&now, CleanState::Cleaned)).await
}

/// Gives everything back.
pub async fn restore(box_: &AndroidTvManager) -> Result<Cleanup, AppError> {
    let now = read(box_).await?;
    run(box_, commands(&now, CleanState::Kept)).await
}

/// What the home is told: numbers and enum names only, nothing that could break out of the shell.
fn house_broadcast(sun: Option<&NextSun>, today: Option<&Today>) -> Option<String> {
    let mut extras = Vec::new();
    if let Some(sun) = sun {
        extras.extend(sun.sunrise_ms.map(|ms| format!("--el sunriseMs {ms}")));
        extras.extend(sun.sunset_ms.map(|ms| format!("--el sunsetMs {ms}")));
        extras.extend(sun.side.map(|side| {
            let side = match side {
                Side::Light => "light",
                Side::Dark => "dark",
            };
            format!("--es side {side}")
        }));
    }
    if let Some(today) = today {
        let color = |day: &maison_api::tempo::Day| day.color.map(|color| color.as_str());
        extras.extend(color(&today.today).map(|c| format!("--es tempoToday {c}")));
        extras.extend(color(&today.tomorrow).map(|c| format!("--es tempoTomorrow {c}")));
    }
    (!extras.is_empty()).then(|| format!("am broadcast -n {HOUSE_RECEIVER} {}", extras.join(" ")))
}

/// Tells the box's home the sun and Tempo. A box asleep or away misses it, which is all right:
/// the home stops showing what it has not heard lately.
pub async fn push_house(state: &AppState) {
    let sun = state.matter.sun_now().await;
    // asked, not watched: the watch stays empty until a client reads Tempo after a start
    let today = state.tempo.today(false).await.ok();
    if let Some(command) = house_broadcast(sun.as_ref(), today.as_ref()) {
        if let Err(error) = state.androidtv.shell(&command).await {
            tracing::debug!(%error, "the box's home not told of the house");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: &str = "package:com.oemservice\npackage:maison.box\n";

    #[test]
    fn the_table_never_names_what_the_house_needs() {
        for needed in [
            "com.google.android.tv.remote.service", // Remote v2: Maison's keys
            "com.google.android.gms",
            "com.android.vending",
            "com.google.android.inputmethod.latin", // the only keyboard
            "com.google.android.webview",
            "com.google.android.tungsten.setupwraith",
            "com.sdt.leaps1.rcupair", // the remote's pairing
            "com.droidlogic",
            "com.android.tv.settings",
            "com.sdt.strong.leaps1.ota.update", // firmware updates stay on
            "com.sdt.strong.leaps1.rcuota.Activity",
            "com.sdt.strong.leaps1.remotecontrolota",
            "com.google.android.tv.dfuservice",
            LAUNCHER,
        ] {
            assert!(TABLE.iter().all(|row| row.0 != needed), "{needed}");
        }
    }

    #[test]
    fn each_package_is_named_once_and_is_a_safe_word() {
        let names: HashSet<_> = TABLE.iter().map(|row| row.0).collect();
        assert_eq!(names.len(), TABLE.len());
        for (package, ..) in TABLE {
            crate::androidtv::validate_package(package).expect(package);
        }
    }

    #[test]
    fn the_listing_says_where_each_package_stands() {
        let listing = format!("package:com.oemservice\n---\n{ALL}package:com.oem.qa\n");
        let read = state_of(&listing);
        let state = |name: &str| {
            read.packages
                .iter()
                .find(|p| p.package == name)
                .unwrap()
                .state
        };
        assert!(read.launcher);
        assert_eq!(state("com.oemservice"), CleanState::Cleaned);
        assert_eq!(state("com.oem.qa"), CleanState::Kept);
        assert_eq!(state("com.netflix.ninja"), CleanState::Absent);
        assert_eq!(
            state("com.google.android.katniss"),
            CleanState::Cleaned,
            "removed for the user, it is no longer listed"
        );
    }

    #[test]
    fn a_box_without_the_home_says_so() {
        assert!(!state_of("---\npackage:com.oemservice\n").launcher);
    }

    #[test]
    fn only_what_differs_is_run_and_each_way_undoes_the_other() {
        let listing = "package:com.oemservice\n---\npackage:com.oemservice\npackage:com.oem.qa\n";
        let now = state_of(listing);
        let cleaning = commands(&now, CleanState::Cleaned);
        assert!(cleaning.contains(&"pm disable-user --user 0 com.oem.qa".to_string()));
        assert!(!cleaning.iter().any(|c| c.contains("com.oemservice")));
        assert!(!cleaning.iter().any(|c| c.contains("netflix")), "absent");
        let restoring = commands(&now, CleanState::Kept);
        assert!(restoring.contains(&"pm enable com.oemservice".to_string()));
        assert!(restoring.contains(
            &"cmd package install-existing --user 0 com.google.android.katniss".to_string()
        ));
        assert!(!restoring.iter().any(|c| c.contains("com.oem.qa")));
    }

    #[test]
    fn the_house_is_said_in_numbers_and_enum_names() {
        let sun = NextSun {
            sunrise_ms: Some(1),
            sunset_ms: None,
            side: Some(Side::Dark),
        };
        assert_eq!(
            house_broadcast(Some(&sun), None).unwrap(),
            "am broadcast -n maison.box/.HouseReceiver --el sunriseMs 1 --es side dark"
        );
        assert_eq!(house_broadcast(None, None), None, "nothing to say");
    }
}
