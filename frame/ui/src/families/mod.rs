//! The house as the dashboard shows it, family by family (web/src/lib/devices): each family
//! says its group from what it reads, in pure functions tested on the backend's answers. The
//! sources are declared once here, under the web app's keys (a family's prefix first, so
//! `refresh("meross")` reads every plug value again), at the paces of `maison_api::paces`.

pub mod lamps;
pub mod plugs;
pub mod scenes;
pub mod shutters;
pub mod tempo;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use maison_api::lamps::{HueLamp, LampList, LampStats, ZigbeeLamp};
use maison_api::matter::Covers;
use maison_api::meross::{ElectricityAnswer, PlugList};
use maison_api::scenes::SceneList;
use maison_api::sun::{Side, SunNow, Where};
use maison_api::tempo::{Forecast, Today};
use maison_api::{Answer, HOUSE_TZ, paces};

use crate::client::Client;
use crate::i18n::{Words, k};
use crate::live::{Live, Source, View};
use crate::ui::model::{Act, Body, Button, Group};

/// A source read once (the web app's sources without a pace): the frame has no editor that
/// would read them again, so once an hour.
const ONCE: Duration = Duration::from_secs(3600);

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// GET `path`, the envelope's body.
fn get<T: serde::de::DeserializeOwned>(
    path: &'static str,
) -> impl Fn(&Client) -> Result<T, crate::client::Failure> + Send + Sync {
    move |c: &Client| c.get::<Answer<T>>(path).map(|a| a.body)
}

/// Every source the dashboard reads, declared once.
pub struct Sources {
    pub tempo: Source<Today>,
    pub forecast: Source<Forecast>,
    pub hue_stats: Source<LampStats>,
    pub zigbee_stats: Source<LampStats>,
    pub hue: Source<LampList<HueLamp>>,
    pub zigbee: Source<LampList<ZigbeeLamp>>,
    pub covers: Source<Covers>,
    pub plugs: Source<PlugList>,
    electricity: HashMap<String, Source<ElectricityAnswer>>,
    pub scenes: Source<SceneList>,
    pub place: Source<Where>,
    pub sun: Source<SunNow>,
}

pub const HUE: &str = "hue-lamp";
pub const ZIGBEE: &str = "zigbee-lamp";
pub const MEROSS: &str = "meross";
pub const SHUTTERS: &str = "shutters";
pub const SCENES: &str = "scenes";
pub const TEMPO: &str = "tempo";

fn until_sun(sun: Option<&maison_api::sun::NextSun>) -> Duration {
    ms(paces::until_sun(sun, Utc::now().timestamp_millis()))
}

impl Sources {
    pub fn new() -> Self {
        Self {
            tempo: Source::every("tempo:today", ms(paces::TEMPO_MS), get("/tempo")),
            forecast: Source::every(
                "tempo:forecast",
                ms(paces::TEMPO_MS),
                get("/tempo/forecast"),
            ),
            hue_stats: Source::every("hue-lamp:stats", ONCE, get("/hue-lamps/stats")),
            zigbee_stats: Source::every("zigbee-lamp:stats", ONCE, get("/zigbee/lamps/stats")),
            hue: Source::every("hue-lamp:list", ms(paces::LAMP_LIST_MS), get("/hue-lamps")),
            zigbee: Source::every(
                "zigbee-lamp:list",
                ms(paces::LAMP_LIST_MS),
                get("/zigbee/lamps"),
            ),
            covers: Source::paced(
                "shutters:list",
                |d: Option<&Covers>| ms(paces::shutters(d)),
                get("/matter/covers"),
            ),
            plugs: Source::every("meross:list", ms(paces::MEROSS_LIST_MS), get("/meross")),
            electricity: HashMap::new(),
            scenes: Source::every(
                "scenes:list",
                ms(paces::SCENES_MS.unwrap_or(ONCE.as_millis() as u64)),
                get("/scenes"),
            ),
            place: Source::paced(
                "shutters:place",
                |d: Option<&Where>| until_sun(d.and_then(|w| w.sun.as_ref())),
                get("/matter/place"),
            ),
            // signed out, the side alone, every ten minutes (web/src/lib/sun/data.ts `sideNow`)
            sun: Source::every("sun:side", ms(paces::SUN_SIGNED_OUT_MS), get("/sun")),
        }
    }

    /// A plug's live power, its source made on first ask.
    pub fn electricity(&mut self, id: &str) -> &Source<ElectricityAnswer> {
        self.electricity.entry(id.to_string()).or_insert_with(|| {
            let path = format!("/meross/{}/electricity", path_segment(id));
            Source::every(
                format!("meross:electricity:{id}"),
                ms(paces::MEROSS_ELECTRICITY_MS),
                move |c: &Client| c.get::<Answer<ElectricityAnswer>>(&path).map(|a| a.body),
            )
        })
    }
}

/// An id in a path, as the web app's `path` tag writes it.
pub fn path_segment(id: &str) -> String {
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

/// What a family reads the house with.
pub struct Cx<'a> {
    pub live: &'a mut Live,
    pub words: &'a Words,
    pub sources: &'a mut Sources,
    pub now: Instant,
}

impl Cx<'_> {
    /// The house's clock now.
    pub fn house_now(&self) -> DateTime<Tz> {
        Utc::now().with_timezone(&HOUSE_TZ)
    }

    /// When a value shown was read, on the house's clock.
    pub fn read_at<T>(&self, v: &View<T>) -> Option<DateTime<Tz>> {
        let at = v.at?;
        let ago = chrono::Duration::from_std(self.now.saturating_duration_since(at)).ok()?;
        Some(self.house_now() - ago)
    }
}

/// The dashboard's groups, in its order (web/src/routes/+page.svelte): scenes, Tempo, lamps,
/// shutters, plugs.
pub fn house(cx: &mut Cx) -> Vec<Group> {
    [
        scenes::group(cx),
        tempo::group(cx),
        lamps::group(cx),
        shutters::group(cx),
        plugs::group(cx),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// The sun's side signed in: the house's place says it.
pub fn sun_signed_in(cx: &mut Cx) -> Option<Side> {
    let v = cx.live.want(&cx.sources.place);
    v.data.and_then(|w| w.sun.as_ref().and_then(|s| s.side))
}

/// Signed out (pairing): the side alone.
pub fn sun_signed_out(cx: &mut Cx) -> Option<Side> {
    cx.live.want(&cx.sources.sun).data.and_then(|s| s.side)
}

/// Loaded.svelte: what a group shows while it reads, when it cannot, when it read nothing.
pub fn loaded<T>(
    w: &Words,
    v: &View<T>,
    refresh: &'static [&'static str],
    empty: bool,
    none: (&str, &str),
    body: impl FnOnce() -> Body,
) -> Body {
    if v.data.is_none() {
        if v.failed {
            return Body::Failed {
                text: w.s(k::ERROR_NETWORK),
                retry: Button::new(
                    Some("refresh-cw"),
                    w.s(k::COMMON_RETRY),
                    Act::Refresh(refresh),
                ),
            };
        }
        return Body::Loading;
    }
    if empty {
        return Body::Empty {
            text: w.s(none.0),
            hint: w.s(none.1),
        };
    }
    body()
}

/// Stale.svelte: « Dernière lecture 14:02 · Réessayer » when the value is old.
pub fn stale<T>(
    cx: &Cx,
    v: &View<T>,
    refresh: &'static [&'static str],
) -> Option<(String, Button)> {
    if !v.stale {
        return None;
    }
    let now = cx.house_now();
    let time = cx
        .read_at(v)
        .map_or_else(String::new, |at| cx.words.when(&at, &now));
    Some((
        cx.words.t(k::LIVE_STALE, &[("time", time.into())]),
        Button::new(None, cx.words.s(k::COMMON_RETRY), Act::Refresh(refresh)),
    ))
}

/// How a group's « Tout … » went (everyone.ts `sayOutcome`): all done in `said(count)`; else a
/// warning naming who did not answer.
pub fn outcome(
    w: &Words,
    o: &crate::commands::Outcome,
    said: impl Fn(usize) -> String,
) -> crate::commands::Toast {
    if o.failed.is_empty() {
        return crate::commands::Toast {
            text: said(o.done.len()),
            warn: false,
        };
    }
    crate::commands::Toast {
        text: w.t(
            k::GROUP_PARTIAL,
            &[
                ("done", said(o.done.len()).into()),
                ("names", w.list(&o.failed).into()),
            ],
        ),
        warn: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_go_in_paths_escaped() {
        assert_eq!(path_segment("ab-12_x.y~"), "ab-12_x.y~");
        assert_eq!(path_segment("a/b c"), "a%2Fb%20c");
        assert_eq!(path_segment("é"), "%C3%A9");
    }
}
