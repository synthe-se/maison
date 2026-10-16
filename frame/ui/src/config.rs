//! What the app is told at start, from its environment (the service's, or a simulation's):
//!
//! - Maison's address: the systemd credential `target` (`$CREDENTIALS_DIRECTORY/target`, the
//!   one `install.sh --target` wrote for the kiosk), else `MAISON_TARGET`; its fragment and path
//!   are dropped (the kiosk's `#lang=…`, an older target's token);
//! - where it keeps its cookie: `MAISON_STATE`, else `$XDG_DATA_HOME/maison-ui` (userdata);
//! - its language: `MAISON_LANG`, else `LANG` (`fr_FR.UTF-8`);
//! - `MAISON_ROTATE=180` (the app turns its picture, when the compositor does not; the touch
//!   is already the person's way up on the frame), `MAISON_SCALE` (CSS pixels to the panel's, 1 as the kiosk drew), `MAISON_WINDOW`
//!   (`800x1280`: a window in place of the full screen, for a simulation).

use std::path::PathBuf;

use crate::i18n::Lang;

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// `https://maison.example` (no trailing slash).
    pub origin: String,
    pub state_dir: PathBuf,
    pub lang: Lang,
    pub rotate: bool,
    pub scale: f32,
    /// A window of this size, not the full screen.
    pub window: Option<(u32, u32)>,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let credential = env("CREDENTIALS_DIRECTORY")
            .and_then(|d| std::fs::read_to_string(PathBuf::from(d).join("target")).ok());
        Self::from(credential, env)
    }

    /// `credential`: the target file's contents, if any; `env`: the variables.
    pub fn from(
        credential: Option<String>,
        env: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, String> {
        let target = credential
            .or_else(|| env("MAISON_TARGET"))
            .ok_or("no Maison address: the credential `target` or MAISON_TARGET")?;
        let origin = origin(target.trim())?;
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
            state_dir,
            lang,
            rotate,
            scale,
            window,
        })
    }
}

/// `https://maison.example/some/path#lang=fr` → `https://maison.example`.
fn origin(target: &str) -> Result<String, String> {
    let (scheme, rest) = target
        .split_once("://")
        .filter(|(s, _)| *s == "https" || *s == "http")
        .ok_or(format!("{target}: not an http(s) address"))?;
    let host = rest.split(['/', '#', '?']).next().unwrap_or_default();
    if host.is_empty() || host.contains('@') {
        return Err(format!("{target}: no host"));
    }
    Ok(format!("{scheme}://{host}"))
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
        assert_eq!(c.origin, "https://maison.example");
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
            ]),
        )
        .unwrap();
        assert_eq!(c.origin, "http://localhost:3999");
        assert_eq!(c.lang, Lang::En);
        assert!(c.rotate);
        assert_eq!(c.scale, 1.5);
        assert_eq!(c.window, Some((800, 1280)));
    }

    #[test]
    fn what_is_wrong_is_said() {
        let state = ("MAISON_STATE", "/tmp/s");
        assert!(
            Config::from(None, vars(&[state]))
                .unwrap_err()
                .contains("no Maison address")
        );
        assert!(Config::from(Some("ftp://x".into()), vars(&[state])).is_err());
        assert!(Config::from(Some("https://".into()), vars(&[state])).is_err());
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
