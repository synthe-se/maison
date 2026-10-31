//! The house as the dashboard shows it, family by family (web/src/lib/devices): each family
//! says its group from what it reads, in pure functions tested on the backend's answers. The
//! sources are declared once here, under the web app's keys (a family's prefix first, so
//! `refresh("meross")` reads every plug value again), at the paces of `maison_api::paces`, from
//! the paths of `crate::paths`.

pub mod cats;
pub mod climate;
pub mod lamps;
pub mod nabaztag;
pub mod now;
pub mod plugs;
pub mod scenes;
pub mod shutters;
pub mod tempo;
pub mod tv;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use maison_api::lamps::{HueLamp, LampList, LampStats, ZigbeeLamp};
use maison_api::matter::Covers;
use maison_api::meross::{ElectricityAnswer, PlugList};
use maison_api::scenes::SceneList;
use maison_api::sun::{Side, SunNow, Where};
use maison_api::tempo::{Forecast, Today};
use maison_api::{Answer, HOUSE_TZ, Tz, paces};

use crate::client::Client;
use crate::commands::{Outcome, Toast};
use crate::i18n::{Words, k};
use crate::live::{Live, Source, View};
use crate::paths;
use crate::ui::icon;
use crate::ui::model::{Act, Body, Button, Group};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// GET `path`, the envelope's body.
fn get<T: serde::de::DeserializeOwned>(
    path: impl Into<String>,
) -> impl Fn(&Client) -> Result<T, crate::client::Failure> + Send + Sync {
    let path = path.into();
    move |c: &Client| c.get::<Answer<T>>(&path).map(|a| a.body)
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
    /// The IR blasters found (`/broadlink/discover`), the air conditioner's last order.
    pub blasters: Source<serde_json::Value>,
    pub climate: Source<serde_json::Value>,
    /// The rabbit (`/nabaztag`): its address set or not, reachable, showing Tempo.
    pub rabbit: Source<serde_json::Value>,
    /// The cats' devices (`/devices`), and each one's parsed status, made on first ask.
    pub cats: Source<serde_json::Value>,
    cat_statuses: HashMap<String, Source<serde_json::Value>>,
    /// The TV and the box (`/tv`, `/androidtv`: `{config, status}`), read as slowly as the TV
    /// allows (its JointSPACE server dies under bursts).
    pub tv: Source<serde_json::Value>,
    pub androidtv: Source<serde_json::Value>,
    pub place: Source<Where>,
    pub sun: Source<SunNow>,
}

pub const HUE: &str = "hue-lamp";
pub const ZIGBEE: &str = "zigbee-lamp";
pub const MEROSS: &str = "meross";
pub const SHUTTERS: &str = "shutters";
pub const SCENES: &str = "scenes";
pub const TEMPO: &str = "tempo";
/// The air conditioner's last order: the event the server says when it changes.
pub const CLIMATE: &str = maison_api::events::CLIMATE;
pub const NABAZTAG: &str = "nabaztag";
/// Every cats' key starts with it: one refresh after a gesture reads them all again (the
/// server says `tuya:<id>:` when a device pushes).
pub const TUYA: &str = maison_api::events::TUYA;
pub const TV: &str = "tv";
/// The box: the event the server says when its power changes.
pub const BOX: &str = maison_api::events::ANDROID_TV;

fn until_sun(sun: Option<&maison_api::sun::NextSun>) -> Duration {
    ms(paces::until_sun(sun, Utc::now().timestamp_millis()))
}

impl Sources {
    pub fn new() -> Self {
        Self {
            tempo: Source::every("tempo:today", ms(paces::TEMPO_MS), get(paths::TEMPO)),
            forecast: Source::every(
                "tempo:forecast",
                ms(paces::TEMPO_MS),
                get(paths::TEMPO_FORECAST),
            ),
            // whether the server runs a radio changes only when it restarts: the web app reads
            // it once; a wall screen, which nobody reloads, as seldom as the scenes
            hue_stats: Source::every(
                "hue-lamp:stats",
                ms(paces::SCENES_MS),
                get(paths::HUE_LAMPS_STATS),
            ),
            zigbee_stats: Source::every(
                "zigbee-lamp:stats",
                ms(paces::SCENES_MS),
                get(paths::ZIGBEE_LAMPS_STATS),
            ),
            hue: Source::every(
                "hue-lamp:list",
                ms(paces::LAMP_LIST_MS),
                get(paths::HUE_LAMPS),
            ),
            zigbee: Source::every(
                "zigbee-lamp:list",
                ms(paces::LAMP_LIST_MS),
                get(paths::ZIGBEE_LAMPS),
            ),
            covers: Source::paced(
                "shutters:list",
                |d: Option<&Covers>| ms(paces::shutters(d)),
                get(paths::COVERS),
            ),
            plugs: Source::every("meross:list", ms(paces::MEROSS_LIST_MS), get(paths::MEROSS)),
            electricity: HashMap::new(),
            scenes: Source::every("scenes:list", ms(paces::SCENES_MS), get(paths::SCENES)),
            // asked often until the blaster answers (the web's search), then seldom
            blasters: Source::paced(
                "broadlink:discover",
                |d: Option<&serde_json::Value>| {
                    ms(match d.and_then(climate::blaster) {
                        Some(_) => paces::SCENES_MS,
                        None => paces::BROADLINK_SEARCH_MS,
                    })
                },
                get(paths::BROADLINK_DISCOVER),
            ),
            tv: Source::every("tv:status", ms(paces::TV_MS), get(paths::TV)),
            androidtv: Source::every(
                format!("{BOX}:status"),
                ms(paces::TV_MS),
                get(paths::ANDROIDTV),
            ),
            cats: Source::every(
                format!("{TUYA}devices"),
                ms(paces::TUYA_DEVICES_MS),
                get(paths::DEVICES),
            ),
            cat_statuses: HashMap::new(),
            rabbit: Source::every(
                "nabaztag:status",
                ms(paces::NABAZTAG_MS),
                get(paths::NABAZTAG),
            ),
            climate: Source::every(
                CLIMATE,
                ms(paces::CLIMATE_STATE_MS),
                get(paths::CLIMATE_STATE),
            ),
            place: Source::paced(
                "shutters:place",
                |d: Option<&Where>| until_sun(d.and_then(|w| w.sun.as_ref())),
                get(paths::PLACE),
            ),
            // signed out, the side alone (web/src/lib/sun/data.ts `sideNow`)
            sun: Source::every("sun:side", ms(paces::SUN_SIGNED_OUT_MS), get(paths::SUN)),
        }
    }

    /// A cat device's parsed status (`/devices/{id}/{kind}/status`), at its kind's pace.
    pub fn cat_status(&mut self, id: &str, kind: &str) -> &Source<serde_json::Value> {
        self.cat_statuses.entry(id.to_string()).or_insert_with(|| {
            let pace = match kind {
                "feeder" => paces::FEEDER_MS,
                "fountain" => paces::FOUNTAIN_MS,
                _ => paces::LITTER_BOX_MS,
            };
            let all =
                get::<serde_json::Value>(paths::of(paths::DEVICES, id, &format!("{kind}/status")));
            Source::every(format!("{TUYA}{id}:status"), ms(pace), move |c: &Client| {
                all(c).map(|b| b["parsedStatus"].clone())
            })
        })
    }

    /// A plug's live power, its source made on first ask.
    pub fn electricity(&mut self, id: &str) -> &Source<ElectricityAnswer> {
        self.electricity.entry(id.to_string()).or_insert_with(|| {
            Source::every(
                format!("meross:electricity:{id}"),
                ms(paces::MEROSS_ELECTRICITY_MS),
                get(paths::of(paths::MEROSS, id, "electricity")),
            )
        })
    }
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
/// the cats' corner, the air conditioning, shutters, plugs, the rabbit, « Télé ».
pub fn house(cx: &mut Cx) -> Vec<Group> {
    [
        scenes::group(cx),
        tempo::group(cx),
        lamps::group(cx),
        cats::group(cx),
        climate::group(cx),
        shutters::group(cx),
        plugs::group(cx),
        nabaztag::group(cx),
        tv::group(cx),
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

/// Loaded.svelte: what a group shows while it reads, when it cannot (the failure in words),
/// when it read nothing; else `body` of what it read.
pub fn loaded<T>(
    w: &Words,
    v: &View<T>,
    refresh: &'static [&'static str],
    none: (&str, &str),
    body: impl FnOnce(&T) -> Option<Body>,
) -> Body {
    let Some(data) = v.data.as_deref() else {
        return match &v.failure {
            Some(f) => Body::Failed {
                text: w.failure(f),
                retry: Button::new(
                    Some(icon::REFRESH_CW),
                    w.s(k::COMMON_RETRY),
                    Act::Refresh(refresh),
                ),
            },
            None => Body::Loading,
        };
    };
    body(data).unwrap_or_else(|| Body::Empty {
        text: w.s(none.0),
        hint: w.s(none.1),
    })
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
pub fn outcome(w: &Words, o: &Outcome, said: impl Fn(usize) -> String) -> Toast {
    if o.failed.is_empty() {
        return Toast {
            text: said(o.done.len()),
            warn: false,
        };
    }
    Toast {
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
    use crate::client::Failure;
    use crate::i18n::Lang;
    use maison_api::codes::Code;

    #[test]
    fn a_group_reads_loading_failed_empty_then_its_body() {
        let w = Words::new(Lang::Fr);
        let none = (k::MEROSS_NONE, k::MEROSS_NONE_HINT);
        let body = |v: &View<Vec<u8>>| {
            loaded(&w, v, &[MEROSS], none, |d: &Vec<u8>| {
                (!d.is_empty()).then(|| Body::Buttons(Vec::new()))
            })
        };
        let loading = View::<Vec<u8>> {
            data: None,
            loading: true,
            failure: None,
            stale: false,
            at: None,
        };
        assert!(matches!(body(&loading), Body::Loading));
        // the failure said as the web app says it: by its code when it has one
        let refused = View::<Vec<u8>> {
            failure: Some(Failure::Refused {
                status: 409,
                code: Some(Code::NotConfigured),
            }),
            loading: false,
            ..loading
        };
        let Body::Failed { text, retry } = body(&refused) else {
            panic!("failed")
        };
        assert_eq!(text, w.failure(refused.failure.as_ref().unwrap()));
        assert_ne!(text, w.s(k::ERROR_NETWORK));
        assert!(matches!(retry.act, Act::Refresh([MEROSS])));
        let offline = View::<Vec<u8>> {
            failure: Some(Failure::Unreachable("x".into())),
            ..refused
        };
        let Body::Failed { text, .. } = body(&offline) else {
            panic!("failed")
        };
        assert_eq!(text, w.s(k::ERROR_NETWORK));
        assert!(matches!(body(&View::ready(Vec::new())), Body::Empty { .. }));
        assert!(matches!(body(&View::ready(vec![1])), Body::Buttons(_)));
    }
}
