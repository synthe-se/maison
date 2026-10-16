//! The app between SDL's events and the drawing: who the screen is (pairing), what the house
//! says (live), the gestures travelling (commands, sliders), the column's scroll and fling,
//! the toasts; and when it must be drawn again. Nothing here waits: the threads answer over
//! channels and wake the loop.

use std::sync::Arc;
use std::time::{Duration, Instant};

use maison_api::sun::Side;

use crate::client::Client;
use crate::commands::{Commands, Toast};
use crate::config::Config;
use crate::families::{self, Cx, Sources};
use crate::i18n::{Words, k};
use crate::live::Live;
use crate::pairing::{Ask, Screen as Who, Session};
use crate::theme::{self, Palette};
use crate::ui::model::{Act, Button, Screen};
use crate::ui::paint::{Hit, Target};
use crate::ui::slider::{Sliders, value_at};
use crate::ui::touch::{Finger, Meaning, Under};

/// How long a toast stays (the web app's: 4 s, a warning longer).
const TOAST: Duration = Duration::from_secs(4);
const TOAST_WARN: Duration = Duration::from_secs(8);
/// A frame while something moves (a fling): the panel's 60 Hz.
pub const FRAME: Duration = Duration::from_millis(16);

pub struct App {
    pub words: Words,
    live: Live,
    session: Session,
    commands: Commands,
    sliders: Sliders,
    sources: Sources,
    finger: Finger,
    /// The column's scroll, in the panel's pixels.
    pub scroll: f32,
    /// The column's height and the panel's, from the last drawing.
    content: f32,
    pub height: f32,
    hits: Vec<Hit>,
    toasts: Vec<(Toast, Instant)>,
    /// The sun's side, kept once known (a page signed out reads it too).
    side: Option<Side>,
    /// Something changed since the last drawing.
    pub dirty: bool,
}

impl App {
    pub fn new(config: &Config, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let client = Client::new(&config.origin, &config.state_dir, Duration::from_secs(15));
        Self {
            words: Words::new(config.lang),
            live: Live::new(client.clone(), wake.clone()),
            session: Session::start(client.clone(), wake.clone()),
            commands: Commands::new(client, wake),
            sliders: Sliders::default(),
            sources: Sources::new(),
            finger: Finger::default(),
            scroll: 0.0,
            content: 0.0,
            height: 0.0,
            hits: Vec::new(),
            toasts: Vec::new(),
            side: None,
            dirty: true,
        }
    }

    /// What the threads said since; the screen is drawn again if anything changed.
    pub fn take(&mut self) {
        let mut changed = self.session.take();
        let read = self.live.take();
        changed |= !read.is_empty();
        for (key, asked) in &read {
            self.commands.read(key, *asked);
        }
        let mut toasts = Vec::new();
        let live = &self.live;
        changed |= self
            .commands
            .take(&self.words, &mut |f| live.refresh(f), &mut toasts);
        let now = Instant::now();
        self.toasts.extend(toasts.into_iter().map(|t| (t, now)));
        if self.live.signed_out() {
            // the screen was removed: the house goes, the pairing says so
            self.live.clear();
            self.session.ask(Ask::SignedOut);
            changed = true;
        }
        self.dirty |= changed;
    }

    /// Time passed: owed slider sends, toasts gone, the gestures' looks (slow, late).
    pub fn tick(&mut self, now: Instant) {
        for o in self.sliders.due(now) {
            self.commands.run(o);
        }
        let before = self.toasts.len();
        self.toasts
            .retain(|(t, at)| now < *at + if t.warn { TOAST_WARN } else { TOAST });
        self.dirty |= self.toasts.len() != before;
        self.sliders.sweep(now);
        if self.finger.flinging() {
            let max = self.max_scroll();
            if let Some(next) = self.finger.step(self.scroll, max, now) {
                self.scroll = next;
                self.dirty = true;
            }
        }
    }

    /// When the loop must wake by itself, if nothing else happens.
    pub fn next_wake(&self, now: Instant) -> Option<Instant> {
        if self.finger.flinging() {
            return Some(now + FRAME);
        }
        [
            self.live.next_stale(),
            self.commands.next_change(),
            self.sliders.next_change(now),
            self.toasts
                .iter()
                .map(|(t, at)| *at + if t.warn { TOAST_WARN } else { TOAST })
                .min(),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    pub fn palette(&self) -> &'static Palette {
        theme::palette(self.side)
    }

    pub fn commands(&self) -> &Commands {
        &self.commands
    }

    pub fn sliders(&self) -> &Sliders {
        &self.sliders
    }

    pub fn toasts(&self) -> Vec<Toast> {
        self.toasts.iter().map(|(t, _)| t.clone()).collect()
    }

    fn max_scroll(&self) -> f32 {
        (self.content - self.height).max(0.0)
    }

    /// What the screen shows now, from who it is and what the house says.
    pub fn screen(&mut self, now: Instant) -> Screen {
        self.live.begin();
        let w = &self.words;
        let pair = |label: &str, ask: Ask| {
            Some(Box::new(
                Button::new(Some("refresh-cw"), label, Act::Pairing(ask)).primary(true),
            ))
        };
        let screen = match &self.session.screen {
            Who::Paired { .. } => {
                let mut cx = Cx {
                    live: &mut self.live,
                    words: &self.words,
                    sources: &mut self.sources,
                    now,
                };
                if let Some(side) = families::sun_signed_in(&mut cx) {
                    self.side = Some(side);
                }
                let mut cx = Cx {
                    live: &mut self.live,
                    words: &self.words,
                    sources: &mut self.sources,
                    now,
                };
                Screen::House(families::house(&mut cx))
            }
            who => {
                let mut cx = Cx {
                    live: &mut self.live,
                    words: &self.words,
                    sources: &mut self.sources,
                    now,
                };
                if let Some(side) = families::sun_signed_out(&mut cx) {
                    self.side = Some(side);
                }
                match who {
                    Who::Checking => Screen::Door {
                        title: w.s(k::SCREEN_PAIR_TITLE),
                        lead: vec![w.s(k::COMMON_LOADING)],
                        code: None,
                        hint: None,
                        button: None,
                        warn: false,
                    },
                    Who::Code { code, minutes, .. } => Screen::Door {
                        title: w.s(k::SCREEN_PAIR_TITLE),
                        lead: vec![w.s(k::SCREEN_PAIR_INTRO)],
                        code: Some(code.clone()),
                        hint: Some(w.t(
                            k::SCREEN_PAIR_VALID,
                            &[("minutes", crate::i18n::Arg::Count(*minutes as f64))],
                        )),
                        button: None,
                        warn: false,
                    },
                    Who::Expired => Screen::Door {
                        title: w.s(k::SCREEN_PAIR_TITLE),
                        lead: vec![w.s(k::SCREEN_PAIR_EXPIRED)],
                        code: None,
                        hint: None,
                        button: pair(&w.s(k::SCREEN_PAIR_AGAIN), Ask::Again),
                        warn: false,
                    },
                    Who::Failed(f) => Screen::Door {
                        title: w.s(k::SCREEN_PAIR_TITLE),
                        lead: vec![w.failure(f)],
                        code: None,
                        hint: None,
                        button: pair(&w.s(k::SCREEN_PAIR_AGAIN), Ask::Again),
                        warn: true,
                    },
                    Who::Revoked => Screen::Door {
                        title: w.s(k::SCREEN_REVOKED_TITLE),
                        lead: vec![w.s(k::SCREEN_REVOKED_BODY)],
                        code: None,
                        hint: None,
                        button: pair(&w.s(k::SCREEN_PAIR_RETRY), Ask::Pair),
                        warn: false,
                    },
                    Who::Paired { .. } => unreachable!("matched above"),
                }
            }
        };
        self.live.sweep();
        screen
    }

    /// What the last drawing left: where the controls are, how tall the column is.
    pub fn drawn(&mut self, hits: Vec<Hit>, content: f32, height: f32) {
        self.hits = hits;
        self.content = content;
        self.height = height;
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());
    }

    /// The centre of the control whose gesture is `key` (a simulation's `press`).
    pub fn center_of(&self, key: &str) -> Option<(f32, f32)> {
        let h = self
            .hits
            .iter()
            .rev()
            .find(|h| crate::sim::key_of(&h.what) == Some(key))?;
        Some((h.x + h.w / 2.0, h.y + h.h / 2.0))
    }

    /// A slider's band: its ends' x and its middle's y.
    pub fn band_of(&self, key: &str) -> Option<(f32, f32, f32)> {
        self.hits.iter().rev().find_map(|h| match &h.what {
            Target::Slider { slider, x0, x1 } if slider.key == key => {
                Some((*x0, *x1, h.y + h.h / 2.0))
            }
            _ => None,
        })
    }

    /// Every control's key and centre.
    pub fn keys(&self) -> Vec<(String, f32, f32)> {
        self.hits
            .iter()
            .filter_map(|h| {
                Some((
                    crate::sim::key_of(&h.what)?.to_string(),
                    h.x + h.w / 2.0,
                    h.y + h.h / 2.0,
                ))
            })
            .collect()
    }

    fn under(&self, x: f32, y: f32) -> Under {
        match self.hits.iter().rposition(|h| h.contains(x, y)) {
            Some(i) => match self.hits[i].what {
                Target::Slider { .. } => Under::Slider(i),
                Target::Act(_) => Under::Hit(i),
            },
            None => Under::Nothing,
        }
    }

    // ── the finger, in the panel's pixels as the picture shows them ──

    pub fn finger_down(&mut self, x: f32, y: f32, now: Instant) {
        let under = self.under(x, y);
        let m = self.finger.down(x, y, under, self.scroll, now);
        self.meant(m, now);
    }

    pub fn finger_motion(&mut self, x: f32, y: f32, now: Instant) {
        let m = self.finger.motion(x, y, self.max_scroll(), now);
        self.meant(m, now);
    }

    pub fn finger_up(&mut self, x: f32, y: f32, now: Instant) {
        let under = self.under(x, y);
        let m = self.finger.up(x, y, under, now);
        self.meant(m, now);
        self.dirty = true;
    }

    fn meant(&mut self, m: Meaning, now: Instant) {
        match m {
            Meaning::None => {}
            Meaning::Scrolled(s) => {
                self.scroll = s;
                self.dirty = true;
            }
            Meaning::Tap(i) => {
                if let Some(Target::Act(act)) = self.hits.get(i).map(|h| h.what.clone()) {
                    self.act(act);
                }
            }
            Meaning::SlideStart(i, x) | Meaning::Slide(i, x) | Meaning::SlideEnd(i, x) => {
                let Some(Target::Slider { slider, x0, x1 }) =
                    self.hits.get(i).map(|h| h.what.clone())
                else {
                    return;
                };
                let v = value_at(&slider, x, x0, x1);
                let order = match m {
                    Meaning::SlideStart(..) => self.sliders.start(&slider, v, now),
                    Meaning::Slide(..) => self.sliders.moved(&slider, v, now),
                    _ => self.sliders.end(&slider, v, now),
                };
                if let Some(o) = order {
                    self.commands.run(o);
                }
                self.dirty = true;
            }
        }
    }

    fn act(&mut self, act: Act) {
        match act {
            Act::Order(o) => {
                // a new try clears what the tile said of the last one
                self.commands.forget(&o.key);
                self.commands.run(o);
            }
            Act::Refresh(prefixes) => {
                for p in prefixes {
                    self.live.refresh(p);
                }
            }
            Act::Pairing(ask) => self.session.ask(ask),
        }
        self.dirty = true;
    }
}
