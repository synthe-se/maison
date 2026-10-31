//! Every path of Maison's API the frame asks, under `/api` (the client adds it), said once: the
//! sources (families/mod.rs), the gestures (families/*) and the pairing read them from here.

/// Tempo: today, tomorrow and their prices (`maison_api::tempo::Today`).
pub const TEMPO: &str = "/tempo";
/// Maison's forecast of the next days (`maison_api::tempo::Forecast`).
pub const TEMPO_FORECAST: &str = "/tempo/forecast";
/// A season's days (`?season=2026-2027`).
pub const TEMPO_CALENDAR: &str = "/tempo/calendar";
/// Hue lamps over Bluetooth: the list, `/{id}/{power,brightness,temperature}`.
pub const HUE_LAMPS: &str = "/hue-lamps";
/// Whether the server runs Bluetooth at all (`maison_api::lamps::LampStats`).
pub const HUE_LAMPS_STATS: &str = "/hue-lamps/stats";
/// Zigbee lamps: the list, `/{id}/{power,brightness,temperature,color,effect}`.
pub const ZIGBEE_LAMPS: &str = "/zigbee/lamps";
pub const ZIGBEE_LAMPS_STATS: &str = "/zigbee/lamps/stats";
/// Matter shutters: the list, `/{id}/{open,close,stop,position,skip}`.
pub const COVERS: &str = "/matter/covers";
/// The house's place and its sun (signed in).
pub const PLACE: &str = "/matter/place";
/// The sun's side alone (signed out).
pub const SUN: &str = "/sun";
/// Meross plugs: the list, `/{id}/{toggle,electricity,consumption}`.
pub const MEROSS: &str = "/meross";
/// The IR blasters on the network, as the server last found them.
pub const BROADLINK_DISCOVER: &str = "/broadlink/discover";
/// The air conditioner's last order, and where orders go (`{host, settings | command, model}`).
pub const CLIMATE_STATE: &str = "/broadlink/mitsubishi/state";
pub const CLIMATE_SEND: &str = "/broadlink/mitsubishi/send";
/// The rabbit: its status, `/ctl` (one of its commands), `/tempo/push` (today's colour again).
pub const NABAZTAG: &str = "/nabaztag";
pub const NABAZTAG_CTL: &str = "/nabaztag/ctl";
pub const NABAZTAG_TEMPO_PUSH: &str = "/nabaztag/tempo/push";
/// The Philips TV: its status and settings, `/power` (over infrared).
pub const TV: &str = "/tv";
pub const TV_POWER: &str = "/tv/power";
/// The TV's volume, set by PUT (`{level}` or `{muted}`).
pub const TV_VOLUME: &str = "/tv/volume";
/// One of its remote's keys.
pub const TV_KEY: &str = "/tv/key";
pub const TV_AMBILIGHT: &str = "/tv/ambilight";
pub const TV_AMBILIGHT_FOLLOW: &str = "/tv/ambilight/follow-video";
/// Its input to the box's HDMI.
pub const TV_SOURCE_BOX: &str = "/tv/source/box";
/// The Android box: its status, `/wake`, `/sleep`, `/launch` (an app, the TV turned on first).
pub const ANDROIDTV: &str = "/androidtv";
pub const BOX_WAKE: &str = "/androidtv/wake";
pub const BOX_SLEEP: &str = "/androidtv/sleep";
pub const BOX_LAUNCH: &str = "/androidtv/launch";
/// One of its remote's keys.
pub const BOX_KEY: &str = "/androidtv/key";
/// The cats' devices (Tuya): the list, `/{id}/{kind}/status`, `/{id}/feeder/feed`.
pub const DEVICES: &str = "/devices";
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
