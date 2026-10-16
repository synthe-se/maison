//! Every path of Maison's API the frame asks, under `/api` (the client adds it), said once: the
//! sources (families/mod.rs), the gestures (families/*) and the pairing read them from here.

/// Tempo: today, tomorrow and their prices (`maison_api::tempo::Today`).
pub const TEMPO: &str = "/tempo";
/// Maison's forecast of the next days (`maison_api::tempo::Forecast`).
pub const TEMPO_FORECAST: &str = "/tempo/forecast";
/// Hue lamps over Bluetooth: the list, `/{id}/power`, `/{id}/brightness`.
pub const HUE_LAMPS: &str = "/hue-lamps";
/// Whether the server runs Bluetooth at all (`maison_api::lamps::LampStats`).
pub const HUE_LAMPS_STATS: &str = "/hue-lamps/stats";
/// Zigbee lamps: the list, `/{id}/power`, `/{id}/brightness`.
pub const ZIGBEE_LAMPS: &str = "/zigbee/lamps";
pub const ZIGBEE_LAMPS_STATS: &str = "/zigbee/lamps/stats";
/// Matter shutters: the list, `/{id}/{open,close,stop,position,skip}`.
pub const COVERS: &str = "/matter/covers";
/// The house's place and its sun (signed in).
pub const PLACE: &str = "/matter/place";
/// The sun's side alone (signed out).
pub const SUN: &str = "/sun";
/// Meross plugs: the list, `/{id}/toggle`, `/{id}/electricity`.
pub const MEROSS: &str = "/meross";
/// Scenes: the list, `/{id}/run`.
pub const SCENES: &str = "/scenes";
/// Who the screen is (its cookie renewed).
pub const SCREEN_ME: &str = "/screens/me";
/// A new pairing code.
pub const PAIRING: &str = "/screens/pairing";
/// Whether an admin paired the code yet.
pub const PAIRING_CLAIM: &str = "/screens/pairing/claim";

/// `{base}/{id}/{action}`, the id escaped (a lamp's `aa:bb` is one segment).
pub fn of(base: &str, id: &str, action: &str) -> String {
    format!("{base}/{}/{action}", segment(id))
}

/// An id in a path, as the web app's `path` tag writes it.
pub fn segment(id: &str) -> String {
    let mut out = String::with_capacity(id.len());
    for b in id.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The server's event stream: what changed, by name (`maison_api::events`).
pub const EVENTS: &str = "/events";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_go_in_paths_escaped() {
        assert_eq!(segment("ab-12_x.y~"), "ab-12_x.y~");
        assert_eq!(segment("a/b c"), "a%2Fb%20c");
        assert_eq!(segment("é"), "%C3%A9");
        assert_eq!(of(HUE_LAMPS, "aa:bb", "power"), "/hue-lamps/aa%3Abb/power");
    }
}
