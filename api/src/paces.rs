//! How often each value is read again, said once for every reader, in milliseconds: the wall
//! frame's native app reads these, the web app imports them from `web/src/lib/paces.ts`,
//! generated from this table (`cargo run --manifest-path api/Cargo.toml --bin paces-ts >
//! web/src/lib/paces.ts`; a test refuses a module out of step). The families the frame does
//! not read yet keep theirs in their `data.ts`.

use crate::matter::Covers;
use crate::sun::NextSun;

macro_rules! paces {
    ($($(#[doc = $doc:literal])* $name:ident = $value:expr;)*) => {
        $($(#[doc = $doc])* pub const $name: u64 = $value;)*

        /// Every pace: its name, its value and its words, for the web app's module.
        pub const ALL: &[(&str, u64, &[&str])] = &[$((stringify!($name), $name, &[$($doc),*])),*];
    };
}

paces! {
    /// Tempo: RTE publishes once a day (tomorrow around 10:40); the server asks it again every
    /// quarter of an hour until then: half an hour is plenty (`/tempo`, `/tempo/forecast`).
    TEMPO_MS = 30 * 60_000;
    /// The lamps' lists, Hue and Zigbee (as the React app did).
    LAMP_LIST_MS = 5_000;
    /// One lamp's page.
    LAMP_DETAIL_MS = 3_000;
    /// The shutters while a motor runs: the position follows the travel.
    SHUTTERS_MOVING_MS = 1_000;
    /// The shutters at rest.
    SHUTTERS_AT_REST_MS = 10_000;
    /// The plugs' list (docs/ux.md § 4: 5 s for plugs).
    MEROSS_LIST_MS = 5_000;
    /// One plug's status, on its page.
    MEROSS_STATUS_MS = 3_000;
    /// A plug's live power.
    MEROSS_ELECTRICITY_MS = 5_000;
    /// A plug's daily totals: they move slowly.
    MEROSS_CONSUMPTION_MS = 30_000;
    /// When the sun says nothing (no place set yet), how soon it is asked again: a place set
    /// later is followed without a reload.
    NO_SUN_MS = 60 * 60_000;
    /// The sun is read again this long after its next event: the answer then names the
    /// following ones.
    SUN_AFTER_MS = 5_000;
    /// Never sooner than this, an event already past.
    SUN_MIN_MS = 1_000;
    /// Signed out (a wall screen waiting to be paired), the sun's side alone, this often: it
    /// knows no hours.
    SUN_SIGNED_OUT_MS = 10 * 60_000;
}

/// The scenes change only from their editor, which reads them again: never polled.
pub const SCENES_MS: Option<u64> = None;

/// The shutters' pace from what was last read.
pub fn shutters(read: Option<&Covers>) -> u64 {
    if read.is_some_and(|d| d.covers.iter().any(|c| c.moving())) {
        SHUTTERS_MOVING_MS
    } else {
        SHUTTERS_AT_REST_MS
    }
}

/// How long until the sun's next event, and a little after; an hour when it says nothing.
/// `now_ms`: since the epoch (the web app's `untilSun`, from the same constants).
pub fn until_sun(sun: Option<&NextSun>, now_ms: i64) -> u64 {
    let next = [sun.and_then(|s| s.sunrise_ms), sun.and_then(|s| s.sunset_ms)]
        .into_iter()
        .flatten()
        .min();
    match next {
        Some(next) => (next - now_ms + SUN_AFTER_MS as i64).max(SUN_MIN_MS as i64) as u64,
        None => NO_SUN_MS,
    }
}

/// The web app's module, as `web/src/lib/paces.ts` holds it.
pub fn typescript() -> String {
    let mut ts = String::from(
        "// Generated from api/src/paces.rs (`cargo run --manifest-path api/Cargo.toml --bin paces-ts >\n\
         // web/src/lib/paces.ts`): never edited. How often each value is read again, in milliseconds,\n\
         // said once for the web app and the wall frame.\n",
    );
    for (name, value, doc) in ALL {
        ts.push('\n');
        match doc {
            [] => {}
            [one] => ts.push_str(&format!("/** {} */\n", one.trim())),
            lines => {
                ts.push_str("/**\n");
                for l in *lines {
                    ts.push_str(&format!(" * {}\n", l.trim()));
                }
                ts.push_str(" */\n");
            }
        }
        ts.push_str(&format!("export const {name} = {value};\n"));
    }
    ts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matter::{Cover, CoverMotion, SunSchedule};

    #[test]
    fn the_sun_is_read_again_just_after_its_next_event() {
        let sun = NextSun {
            sunrise_ms: Some(100_000),
            sunset_ms: Some(50_000),
            side: None,
        };
        assert_eq!(until_sun(Some(&sun), 40_000), 15_000);
        // already past: a second, not a negative wait
        assert_eq!(until_sun(Some(&sun), 60_000), 1_000);
        let only_rise = NextSun {
            sunset_ms: None,
            ..sun
        };
        assert_eq!(until_sun(Some(&only_rise), 0), 105_000);
        let nothing = NextSun {
            sunrise_ms: None,
            sunset_ms: None,
            side: None,
        };
        assert_eq!(until_sun(Some(&nothing), 0), NO_SUN_MS);
        assert_eq!(until_sun(None, 0), 3_600_000);
    }

    #[test]
    fn the_shutters_are_read_fast_while_a_motor_runs() {
        let cover = |motion| Cover {
            id: "c".into(),
            name: "c".into(),
            endpoint: 1,
            online: true,
            open_percent: Some(0),
            target_open_percent: None,
            motion,
            vendor_id: None,
            product_id: None,
            schedule: SunSchedule::default(),
            next_open: None,
            next_close: None,
            skip_next_open: false,
            skip_next_close: false,
            error: None,
        };
        let at_rest = Covers {
            covers: vec![cover(Some(CoverMotion::Stopped)), cover(None)],
        };
        assert_eq!(shutters(Some(&at_rest)), SHUTTERS_AT_REST_MS);
        let moving = Covers {
            covers: vec![cover(Some(CoverMotion::Closing))],
        };
        assert_eq!(shutters(Some(&moving)), 1_000);
        assert_eq!(shutters(None), 10_000);
        assert_eq!(TEMPO_MS, 1_800_000);
    }

    /// The web app's module is this table's: regenerate it when a pace changes.
    #[test]
    fn the_web_apps_paces_are_these() {
        assert_eq!(
            include_str!("../../web/src/lib/paces.ts"),
            typescript(),
            "web/src/lib/paces.ts is out of step: cargo run --manifest-path api/Cargo.toml --bin paces-ts > web/src/lib/paces.ts"
        );
        assert!(ALL.iter().any(|(n, v, _)| *n == "SUN_SIGNED_OUT_MS" && *v == 600_000));
    }
}
