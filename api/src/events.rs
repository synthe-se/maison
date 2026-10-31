//! What the server says changed (`GET /api/events`, a server-sent event stream): each event's
//! data is the name of what changed, the prefix of the `live()` keys that read it (the web app's
//! `devices/*/data.ts`, the wall frame's `families`), which read it again at once; their polls
//! become a slow safety net while the stream is open. The data is never the value: one answer,
//! one route, one serialisation (DRY). `web/src/lib/events.ts` is generated from this table
//! (`cargo run --manifest-path api/Cargo.toml --bin shared-ts events > web/src/lib/events.ts`).

/// The SSE event name every change goes under.
pub const CHANGE: &str = "change";

/// Everything may have changed (the stream fell behind): every source reads again.
pub const EVERYTHING: &str = "";

macro_rules! events {
    ($($(#[doc = $doc:literal])* $name:ident = $key:literal;)*) => {
        $($(#[doc = $doc])* pub const $name: &str = $key;)*

        /// Every fixed event, for the web app's module.
        pub const ALL: &[(&str, &str)] = &[$((stringify!($name), $key)),*];
    };
}

events! {
    /// The Zigbee lamps (lists and pages).
    ZIGBEE_LAMPS = "zigbee-lamp";
    /// The shutters' list.
    SHUTTERS = "shutters:list";
    /// Tempo's colors and forecast.
    TEMPO = "tempo";
    /// The plugs (list, status, power).
    MEROSS = "meross";
    /// The last climate order.
    CLIMATE = "broadlink:climate-state";
    /// A key of the remote was pressed.
    IR_KEYS = "ir:recent";
    /// The Android TV box (its power).
    ANDROID_TV = "androidtv";
    /// The frame's photos (one added, one removed).
    PHOTOS = "photos";
}

/// What every cats' device's event (and `live()` key) starts with.
pub const TUYA: &str = "tuya:";

/// The cats' device `id` (Tuya): its status.
pub fn tuya(id: &str) -> String {
    format!("{TUYA}{id}:")
}

/// How long the stream stays quiet before a comment keeps it open (Cloudflare closes an idle
/// answer after about 100 s).
pub const KEEP_ALIVE_MS: u64 = 25_000;
const _: () = assert!(KEEP_ALIVE_MS < 100_000, "under Cloudflare's idle limit");

/// While the stream is open, a pushed source is read at least this often all the same.
pub const PUSHED_SAFETY_MS: u64 = 60_000;

/// The web app's module, as `web/src/lib/events.ts` holds it.
pub fn typescript() -> String {
    let mut ts = String::from(
        "// Generated from api/src/events.rs (`cargo run --manifest-path api/Cargo.toml --bin shared-ts events >\n\
         // web/src/lib/events.ts`): never edited. What GET /api/events says changed: the prefix of the\n\
         // live() keys that read it again.\n\n",
    );
    ts.push_str(&format!("export const CHANGE = '{CHANGE}';\n"));
    ts.push_str(&format!("export const KEEP_ALIVE_MS = {KEEP_ALIVE_MS};\n"));
    ts.push_str(&format!(
        "export const PUSHED_SAFETY_MS = {PUSHED_SAFETY_MS};\n\n"
    ));
    ts.push_str(
        "/** The fixed events; a cats' device says `tuya:<id>:`. */\nexport const EVENTS = {\n",
    );
    let rows: Vec<String> = ALL.iter().map(|(n, k)| format!("\t{n}: '{k}'")).collect();
    ts.push_str(&rows.join(",\n"));
    ts.push_str(&format!(
        "\n}} as const;\n\n/** The prefix a pushed source's key starts with. */\nexport const PUSHED: readonly string[] = [...Object.values(EVENTS), '{TUYA}'];\n"
    ));
    ts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_are_key_prefixes() {
        assert_eq!(tuya("abc"), "tuya:abc:");
        assert!(ALL.iter().all(|(_, k)| !k.is_empty() && !k.contains(' ')));
    }

    /// The web app's module is this table's: regenerate it when an event changes.
    #[test]
    fn the_web_apps_events_are_these() {
        assert_eq!(
            include_str!("../../web/src/lib/events.ts"),
            typescript(),
            "web/src/lib/events.ts is out of step: cargo run --manifest-path api/Cargo.toml --bin shared-ts events > web/src/lib/events.ts"
        );
    }
}
