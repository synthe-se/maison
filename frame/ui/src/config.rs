//! What the app is told at start, from its environment (the service's, or a simulation's):
//!
//! - Maison's address: the systemd credential `target` (`$CREDENTIALS_DIRECTORY/target`, root's
//!   `/android-data/maison/target`: `maison-target` adopts what `install.sh --target` left),
//!   else `MAISON_TARGET`; its fragment and path are dropped (the kiosk's `#lang=…`, an older
//!   target's token). HTTPS only, but for this computer (`localhost`, a loopback address) and a
//!   simulation that says `MAISON_ALLOW_HTTP=1` (the host seen from a container): the screen's
//!   cookie never crosses the Wi-Fi in clear;
//! - no address at all, or `MAISON_SETUP_SSID` said (the frame lost its network and is its own
//!   Wi-Fi; the unit's file, read as the app starts: `maison-wifi-watch` restarts it): the app
//!   starts on its setup view, with what `MAISON_SETUP_SSID`, `MAISON_SETUP_PASSWORD` and
//!   `MAISON_SETUP_URL` say of that Wi-Fi and its page;
//! - where it keeps its cookie: `MAISON_STATE` (the unit's `/android-data/maison/ui`), else
//!   `$XDG_DATA_HOME/maison-ui`;
//! - its language: `MAISON_LANG`, else `LANG` (`fr_FR.UTF-8`);
//! - `MAISON_ROTATE=180` (the app turns its picture, when the compositor does not; the touch
//!   is already the person's way up on the frame), `MAISON_SCALE` (CSS pixels to the panel's, 1
//!   as the kiosk drew), `MAISON_WINDOW` (`800x1280`: a window in place of the full screen, for
//!   a simulation);
//! - `MAISON_HEARTBEAT`: a file touched while the app runs, for the update system.

use std::path::PathBuf;

use crate::i18n::Lang;

/// The frame's panel, portrait, in its pixels (the full screen's size before SDL says it).
pub const PANEL: (u32, u32) = (800, 1280);

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// `https://maison.example` (no trailing slash); none: the setup view.
    pub origin: Option<String>,
    pub setup: Setup,
    pub state_dir: PathBuf,
    pub lang: Lang,
    pub rotate: bool,
    pub scale: f32,
    /// A window of this size, not the full screen.
    pub window: Option<(u32, u32)>,
    /// Touched while the app runs.
    pub heartbeat: Option<PathBuf>,
}

/// How a frame without an address is given one: its Wi-Fi and the page there, as far as told.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Setup {
    pub ssid: Option<String>,
    pub password: Option<String>,
    pub url: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let credential = env("CREDENTIALS_DIRECTORY").and_then(|d| {
            let file = PathBuf::from(d).join("target");
            match std::fs::read_to_string(&file) {
                Ok(t) => Some(t),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                // said, then MAISON_TARGET if there is one
                Err(e) => {
                    eprintln!("the credential {} could not be read: {e}", file.display());
                    None
                }
            }
        });
        Self::from(credential, env)
    }

    /// `credential`: the target file's contents, if any; `env`: the variables.
    pub fn from(
        credential: Option<String>,
        env: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, String> {
        let allow_http = env("MAISON_ALLOW_HTTP").as_deref() == Some("1");
        let origin = credential
            .or_else(|| env("MAISON_TARGET"))
            .filter(|t| !t.trim().is_empty())
            .map(|t| origin(t.trim(), allow_http))
            .transpose()?;
        let setup = Setup {
            ssid: env("MAISON_SETUP_SSID"),
            password: env("MAISON_SETUP_PASSWORD"),
            url: env("MAISON_SETUP_URL"),
        };
        let origin = origin.filter(|_| setup.ssid.is_none());
        let state_dir = env("MAISON_STATE")
            .map(PathBuf::from)
            .or_else(|| env("XDG_DATA_HOME").map(|d| PathBuf::from(d).join("maison-ui")))
            .ok_or("no state directory: MAISON_STATE or XDG_DATA_HOME")?;
        let lang = Lang::from_tag(
            &env("MAISON_LANG")
                .or_else(|| env("LANG"))
                .unwrap_or_default(),
        );
        let rotate = match env("MAISON_ROTATE").as_deref() {
            None | Some("0") => false,
            Some("180") => true,
            Some(other) => return Err(format!("MAISON_ROTATE={other}: 0 or 180")),
        };
        let scale = match env("MAISON_SCALE") {
            None => 1.0,
            Some(s) => s
                .parse::<f32>()
                .ok()
                .filter(|s| (0.5..=4.0).contains(s))
                .ok_or(format!("MAISON_SCALE={s}: a number from 0.5 to 4"))?,
        };
        let window = match env("MAISON_WINDOW") {
            None => None,
            Some(s) => Some(
                s.split_once('x')
                    .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
                    .ok_or(format!("MAISON_WINDOW={s}: WIDTHxHEIGHT"))?,
            ),
        };
        Ok(Self {
            origin,
            setup,
            state_dir,
            lang,
            rotate,
            scale,
            window,
            heartbeat: env("MAISON_HEARTBEAT").map(PathBuf::from),
        })
    }
}

/// `https://maison.example/some/path#lang=fr` → `https://maison.example`; `http://` only to this
/// computer, or anywhere when `allow_http` (a simulation's).
fn origin(target: &str, allow_http: bool) -> Result<String, String> {
    let (scheme, rest) = target
        .split_once("://")
        .filter(|(s, _)| *s == "https" || *s == "http")
        .ok_or(format!("{target}: not an http(s) address"))?;
    let host = rest.split(['/', '#', '?']).next().unwrap_or_default();
    if host.is_empty() || host.contains('@') {
        return Err(format!("{target}: no host"));
    }
    if scheme == "http" && !allow_http && !loopback(host) {
        return Err(format!(
            "{target}: HTTPS (the screen's cookie never travels in clear), or this computer"
        ));
    }
    Ok(format!("{scheme}://{host}"))
}

/// `localhost`, `127.x.y.z`, `[::1]`, with or without a port.
fn loopback(host: &str) -> bool {
    let name = match host.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or_default(),
        None => host.rsplit_once(':').map_or(host, |(name, _)| name),
    };
    name.eq_ignore_ascii_case("localhost")
        || name
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k| pairs.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone())
    }

    #[test]
    fn the_credential_wins_and_loses_its_fragment() {
        let c = Config::from(
            Some("https://maison.example/#lang=fr&token=old\n".into()),
            vars(&[
                ("MAISON_TARGET", "http://other"),
                ("XDG_DATA_HOME", "/android-data/maison/ui"),
                ("LANG", "fr_FR.UTF-8"),
            ]),
        )
        .unwrap();
        assert_eq!(c.origin.as_deref(), Some("https://maison.example"));
        assert_eq!(
            c.state_dir,
            PathBuf::from("/android-data/maison/ui/maison-ui")
        );
        assert_eq!(c.lang, Lang::Fr);
        assert!(!c.rotate);
        assert_eq!(c.scale, 1.0);
        assert_eq!(c.window, None);
    }

    #[test]
    fn a_simulation_says_everything() {
        let c = Config::from(
            None,
            vars(&[
                ("MAISON_TARGET", "http://localhost:3999"),
                ("MAISON_STATE", "/tmp/s"),
                ("MAISON_LANG", "en"),
                ("MAISON_ROTATE", "180"),
                ("MAISON_SCALE", "1.5"),
                ("MAISON_WINDOW", "800x1280"),
                ("MAISON_HEARTBEAT", "/run/maison/beat"),
            ]),
        )
        .unwrap();
        assert_eq!(c.heartbeat, Some(PathBuf::from("/run/maison/beat")));
        assert_eq!(c.origin.as_deref(), Some("http://localhost:3999"));
        assert_eq!(c.lang, Lang::En);
        assert!(c.rotate);
        assert_eq!(c.scale, 1.5);
        assert_eq!(c.window, Some((800, 1280)));
    }

    #[test]
    fn no_address_is_no_error_the_setup_view_says_what_it_was_told() {
        let state = ("MAISON_STATE", "/tmp/s");
        let c = Config::from(None, vars(&[state])).unwrap();
        assert_eq!((c.origin, c.setup), (None, Setup::default()));
        let c = Config::from(
            Some(" \n".into()),
            vars(&[
                state,
                ("MAISON_SETUP_SSID", "maison-frame"),
                ("MAISON_SETUP_PASSWORD", "sel"),
                ("MAISON_SETUP_URL", "http://10.42.0.1"),
            ]),
        )
        .unwrap();
        assert_eq!(c.origin, None, "an empty credential says no address");
        assert_eq!(c.setup.ssid.as_deref(), Some("maison-frame"));
        assert_eq!(c.setup.password.as_deref(), Some("sel"));
        assert_eq!(c.setup.url.as_deref(), Some("http://10.42.0.1"));
    }

    #[test]
    fn a_frame_that_is_its_own_wifi_shows_its_setup_even_with_an_address() {
        let c = Config::from(
            Some("https://maison.example".into()),
            vars(&[
                ("MAISON_STATE", "/tmp/s"),
                ("MAISON_SETUP_SSID", "Maison-ab12"),
            ]),
        )
        .unwrap();
        assert_eq!(c.origin, None);
        assert_eq!(c.setup.ssid.as_deref(), Some("Maison-ab12"));
    }

    #[test]
    fn what_is_wrong_is_said() {
        let state = ("MAISON_STATE", "/tmp/s");
        assert!(Config::from(Some("ftp://x".into()), vars(&[state])).is_err());
        assert!(Config::from(Some("https://".into()), vars(&[state])).is_err());
        // in clear, only to this computer, or for a simulation that says so
        let http = |t: &str, allow: &str| {
            Config::from(Some(t.into()), vars(&[state, ("MAISON_ALLOW_HTTP", allow)]))
                .map(|c| c.origin.unwrap())
        };
        assert!(http("http://maison.lan", "").unwrap_err().contains("HTTPS"));
        assert!(http("http://192.168.1.103:3000", "0").is_err());
        assert_eq!(
            http("http://localhost:3999/x", "").unwrap(),
            "http://localhost:3999"
        );
        assert!(http("http://127.0.0.1:9", "").is_ok());
        assert!(http("http://[::1]:9", "").is_ok());
        assert!(http("http://localhost.evil.example", "").is_err());
        assert_eq!(
            http("http://host.docker.internal:4000", "1").unwrap(),
            "http://host.docker.internal:4000"
        );
        assert!(
            Config::from(Some("https://a".into()), vars(&[]))
                .unwrap_err()
                .contains("state")
        );
        let bad = |k, v| Config::from(Some("https://a".into()), vars(&[state, (k, v)])).is_err();
        assert!(bad("MAISON_ROTATE", "90"));
        assert!(bad("MAISON_SCALE", "9"));
        assert!(bad("MAISON_WINDOW", "800"));
    }
}
