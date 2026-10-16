//! The app between SDL's events and the drawing: who the screen is (pairing), what the house
//! says (live), the gestures travelling (commands, sliders), the column's scroll and fling,
//! the toasts; and when it must be drawn again. Nothing here waits: the threads answer over
//! channels and wake the loop.

use std::sync::Arc;
use std::time::{Duration, Instant};

use maison_api::paces;
use maison_api::sun::Side;

use crate::backlight::Backlight;
use crate::client::Client;
use crate::commands::{Commands, Toast};
use crate::config::Config;
use crate::families::{self, Cx, Sources};
use crate::i18n::{Words, k};
use crate::live::Live;
use crate::pairing::{Ask, Screen as Who, Session};
use crate::theme::{self, Palette};
use crate::ui::icon;
use crate::ui::layout::{Hit, REACH, Target};
use crate::ui::model::{Act, Button, Screen, View};
use crate::ui::slider::{Sliders, value_at};
use crate::ui::touch::{Finger, Meaning, Under};

/// How long a toast stays (`paces::TOAST_MS`, the web app's). A warning stays until dismissed
/// on a page; on a wall, which nobody dismisses, `WALL_WARNING_MS`.
const TOAST: Duration = Duration::from_millis(paces::TOAST_MS);
const TOAST_WARN: Duration = Duration::from_millis(paces::WALL_WARNING_MS);
/// A frame while something moves (a fling): the panel's 60 Hz.
pub const FRAME: Duration = Duration::from_millis(16);
/// A page left open goes back to the house after this long without a finger: a wall screen
/// shows the house to whoever passes by.
pub const PAGE_IDLE: Duration = Duration::from_secs(120);
/// Every API call's whole wait (a device's own limit is shorter: `commands::limit`).
const HTTP_WAIT: Duration = Duration::from_secs(15);

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
    /// What the finger went down on, as it was then: a drawing in between (a read came, the
    /// house redrew) moves the hits, never what the finger holds.
    pressed: Option<(Under, Hit)>,
    /// The house, or one of its pages; and when a finger last touched the panel.
    view: View,
    touched: Instant,
    /// Dimmed while nobody uses the frame; `waking`: the finger that woke it, which does nothing
    /// else until it lifts.
    backlight: Backlight,
    waking: bool,
    /// The column's height and the panel's, from the last drawing.
    content: f32,
    height: f32,
    hits: Vec<Hit>,
    /// The toasts shown, each until when.
    toasts: Vec<(Toast, Instant)>,
    /// The sun's side, kept once known (a page signed out reads it too).
    side: Option<Side>,
    /// Something changed since the last drawing.
    pub dirty: bool,
}

impl App {
    pub fn new(config: &Config, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let client = Client::new(&config.origin, &config.state_dir, HTTP_WAIT);
        let mut live = Live::new(client.clone(), wake.clone());
        // what the server says changed: read again at once, the polls a safety net
        live.listen();
        Self {
            words: Words::new(config.lang),
            live,
            session: Session::start(client.clone(), wake.clone()),
            commands: Commands::new(client, wake),
            sliders: Sliders::default(),
            sources: Sources::new(),
            finger: Finger::default(),
            scroll: 0.0,
            pressed: None,
            view: View::House,
            touched: Instant::now(),
            backlight: Backlight::open(std::path::Path::new("/sys/class/backlight")),
            waking: false,
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
        let paired = self.session.screen == Who::Paired;
        let mut changed = self.session.take();
        if !paired && self.session.screen == Who::Paired {
            // refused until now: the event stream opens at once, not after its wait
            self.live.reopen();
        }
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
        for t in toasts {
            self.toast(t, now);
        }
        if self.live.signed_out() {
            // the screen was removed: the house goes, the pairing says so
            self.live.clear();
            self.view = View::House;
            self.session.ask(Ask::SignedOut);
            changed = true;
        }
        self.dirty |= changed;
    }

    /// A toast, until its time is over; the same words again replace it (the web app's), and
    /// stay their whole time from now.
    fn toast(&mut self, t: Toast, now: Instant) {
        let until = now + if t.warn { TOAST_WARN } else { TOAST };
        self.toasts.retain(|(shown, _)| shown.text != t.text);
        self.toasts.push((t, until));
        self.dirty = true;
    }

    /// Time passed: owed slider sends, toasts gone, the gestures' looks (slow, late).
    pub fn tick(&mut self, now: Instant) {
        for o in self.sliders.due(now) {
            self.commands.run(o);
        }
        let before = self.toasts.len();
        self.toasts.retain(|(_, until)| now < *until);
        self.dirty |= self.toasts.len() != before;
        self.sliders.sweep(now);
        let night = self.side == Some(Side::Dark);
        self.backlight.idle(now, self.touched, night);
        if self.view != View::House && now.saturating_duration_since(self.touched) >= PAGE_IDLE {
            self.open(View::House);
        }
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
            self.toasts.iter().map(|(_, until)| *until).min(),
            (self.view != View::House).then(|| self.touched + PAGE_IDLE),
            self.backlight.next(self.touched),
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
        let mut cx = Cx {
            live: &mut self.live,
            words: &self.words,
            sources: &mut self.sources,
            now,
        };
        let (side, screen) = match &self.session.screen {
            Who::Paired => (
                families::sun_signed_in(&mut cx),
                match self.view {
                    View::House => {
                        let now = cx.house_now();
                        Screen::House {
                            now: families::now::chips(&mut cx, &now),
                            groups: families::house(&mut cx),
                        }
                    }
                    View::Tempo => families::tempo::page(&mut cx),
                },
            ),
            who => (families::sun_signed_out(&mut cx), door(&self.words, who)),
        };
        if side.is_some() {
            self.side = side;
        }
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
        let h = self.hits.iter().rev().find(|h| h.what.key() == Some(key))?;
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
            .filter_map(|h| Some((h.what.key()?.to_string(), h.x + h.w / 2.0, h.y + h.h / 2.0)))
            .collect()
    }

    fn under(&self, x: f32, y: f32) -> Under {
        // on a control (the last drawn wins), else the nearest within reach
        let on = self
            .hits
            .iter()
            .rposition(|h| h.contains(x, y))
            .or_else(|| {
                self.hits
                    .iter()
                    .enumerate()
                    .map(|(i, h)| (i, h.distance(x, y)))
                    .filter(|(_, d)| *d <= REACH)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(i, _)| i)
            });
        match on {
            Some(i) => match self.hits[i].what {
                Target::Slider { .. } => Under::Slider(i),
                Target::Act(_) => Under::Hit(i),
            },
            None => Under::Nothing,
        }
    }

    // ── the finger, in the panel's pixels as the picture shows them ──

    pub fn finger_down(&mut self, x: f32, y: f32, now: Instant) {
        self.touched = now;
        // asleep: this finger only wakes the panel
        if self.backlight.wake() {
            self.waking = true;
            self.dirty = true;
            return;
        }
        let under = self.under(x, y);
        self.pressed = match under {
            Under::Hit(i) | Under::Slider(i) => self.hits.get(i).map(|h| (under, h.clone())),
            Under::Nothing => None,
        };
        let m = self.finger.down(x, y, under, self.scroll, now);
        self.meant(m, now);
    }

    pub fn finger_motion(&mut self, x: f32, y: f32, now: Instant) {
        if self.waking {
            return;
        }
        let m = self.finger.motion(x, y, self.max_scroll(), now);
        self.meant(m, now);
    }

    pub fn finger_up(&mut self, x: f32, y: f32, now: Instant) {
        if std::mem::take(&mut self.waking) {
            return;
        }
        // still on what it pressed, wherever the hits are now
        let under = match &self.pressed {
            Some((u, h)) if h.distance(x, y) <= REACH => *u,
            _ => Under::Nothing,
        };
        let m = self.finger.up(x, y, under, now);
        self.meant(m, now);
        self.pressed = None;
        self.dirty = true;
    }

    /// What a finger at (x, y) would land on, by its key (a log's words).
    pub fn key_under(&self, x: f32, y: f32) -> String {
        match self.under(x, y) {
            Under::Hit(i) | Under::Slider(i) => self.hits[i]
                .what
                .key()
                .map_or_else(|| "a control".to_string(), str::to_string),
            Under::Nothing => "nothing".to_string(),
        }
    }

    /// What the finger holds (its target as it went down).
    fn held(&self) -> Option<Target> {
        self.pressed.as_ref().map(|(_, h)| h.what.clone())
    }

    fn meant(&mut self, m: Meaning, now: Instant) {
        match m {
            Meaning::None => {}
            Meaning::Scrolled(s) => {
                self.scroll = s;
                self.dirty = true;
            }
            Meaning::Tap(_) => {
                if let Some(Target::Act(act)) = self.held() {
                    self.act(act);
                }
            }
            Meaning::SlideStart(_, x) | Meaning::Slide(_, x) | Meaning::SlideEnd(_, x) => {
                let Some(Target::Slider { slider, x0, x1 }) = self.held() else {
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
            Act::Open(view) => self.open(view),
        }
        self.dirty = true;
    }

    /// Shows `view`, from its top.
    fn open(&mut self, view: View) {
        self.view = view;
        self.scroll = 0.0;
        self.finger = Finger::default();
        self.dirty = true;
    }
}

/// The door a screen not (yet) paired shows: what the pairing says, a code, a button.
fn door(w: &Words, who: &Who) -> Screen {
    let pair = |label: &str, ask: Ask| {
        Some(Box::new(
            Button::new(Some(icon::REFRESH_CW), label, Act::Pairing(ask)).primary(true),
        ))
    };
    let (title, lead, code, hint, button, warn) = match who {
        // shown only once the house is: the door of a screen still asking who it is
        Who::Checking | Who::Paired => (
            k::SCREEN_PAIR_TITLE,
            w.s(k::COMMON_LOADING),
            None,
            None,
            None,
            false,
        ),
        Who::Code { code, minutes } => (
            k::SCREEN_PAIR_TITLE,
            w.s(k::SCREEN_PAIR_INTRO),
            Some(code.clone()),
            Some(w.t(
                k::SCREEN_PAIR_VALID,
                &[("minutes", crate::i18n::Arg::Count(*minutes as f64))],
            )),
            None,
            false,
        ),
        Who::Expired => (
            k::SCREEN_PAIR_TITLE,
            w.s(k::SCREEN_PAIR_EXPIRED),
            None,
            None,
            pair(&w.s(k::SCREEN_PAIR_AGAIN), Ask::Again),
            false,
        ),
        Who::Failed(f) => (
            k::SCREEN_PAIR_TITLE,
            w.failure(f),
            None,
            None,
            pair(&w.s(k::SCREEN_PAIR_AGAIN), Ask::Again),
            true,
        ),
        Who::Revoked => (
            k::SCREEN_REVOKED_TITLE,
            w.s(k::SCREEN_REVOKED_BODY),
            None,
            None,
            pair(&w.s(k::SCREEN_PAIR_RETRY), Ask::Pair),
            false,
        ),
    };
    Screen::Door {
        title: w.s(title),
        lead: vec![lead],
        code,
        hint,
        button,
        warn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::stub::serve;
    use crate::i18n::Lang;
    use crate::testing::{temp_dir, wait_until};
    use std::path::PathBuf;

    fn config(origin: &str, state_dir: PathBuf) -> Config {
        Config {
            origin: origin.into(),
            state_dir,
            lang: Lang::Fr,
            rotate: false,
            scale: 1.0,
            window: None,
        }
    }

    /// An app whose Maison answers nothing at all.
    fn alone() -> App {
        App::new(
            &config("http://127.0.0.1:9", temp_dir("app")),
            Arc::new(|| {}),
        )
    }

    fn toast(text: &str, warn: bool) -> Toast {
        Toast {
            text: text.into(),
            warn,
        }
    }

    #[test]
    fn the_scroll_stays_inside_the_column() {
        let mut app = alone();
        app.scroll = 5000.0;
        app.drawn(Vec::new(), 3000.0, 1280.0);
        assert_eq!(app.scroll, 3000.0 - 1280.0);
        app.scroll = -40.0;
        app.drawn(Vec::new(), 3000.0, 1280.0);
        assert_eq!(app.scroll, 0.0);
        // a column shorter than the panel does not scroll
        app.scroll = 100.0;
        app.drawn(Vec::new(), 600.0, 1280.0);
        assert_eq!(app.scroll, 0.0);
    }

    #[test]
    fn a_toast_stays_its_time_a_warning_longer_and_the_same_words_once() {
        let mut app = alone();
        let t0 = Instant::now();
        app.toast(toast("Fait", false), t0);
        app.toast(toast("Pas de réponse", true), t0);
        assert_eq!(app.next_wake(t0), Some(t0 + TOAST));
        app.dirty = false;
        app.tick(t0 + TOAST - Duration::from_millis(1));
        assert_eq!(app.toasts().len(), 2);
        assert!(!app.dirty);
        app.tick(t0 + TOAST);
        assert_eq!(app.toasts(), [toast("Pas de réponse", true)]);
        assert!(app.dirty, "a toast gone is drawn gone");
        assert_eq!(app.next_wake(t0 + TOAST), Some(t0 + TOAST_WARN));
        // the same words again: one toast, its time from now
        let t1 = t0 + TOAST;
        app.toast(toast("Pas de réponse", true), t1);
        assert_eq!(app.toasts().len(), 1);
        app.tick(t0 + TOAST_WARN);
        assert_eq!(app.toasts().len(), 1);
        app.tick(t1 + TOAST_WARN);
        assert!(app.toasts().is_empty());
        assert_eq!(TOAST, Duration::from_millis(paces::TOAST_MS));
        assert_eq!(TOAST_WARN, Duration::from_millis(paces::WALL_WARNING_MS));
    }

    #[test]
    fn the_finger_that_wakes_the_dimmed_panel_does_nothing_else() {
        let mut app = alone();
        app.drawn(
            vec![Hit {
                x: 0.0,
                y: 0.0,
                w: 800.0,
                h: 1280.0,
                what: Target::Act(Act::Open(View::Tempo)),
            }],
            1280.0,
            1280.0,
        );
        let asleep = app.touched + crate::backlight::DIM_AFTER;
        app.tick(asleep);
        app.finger_down(400.0, 600.0, asleep);
        app.finger_up(400.0, 600.0, asleep + FRAME);
        assert_eq!(app.view, View::House, "the waking tap opened nothing");
        // awake: the next tap acts
        app.finger_down(400.0, 600.0, asleep + FRAME * 10);
        app.finger_up(400.0, 600.0, asleep + FRAME * 11);
        assert_eq!(app.view, View::Tempo);
    }

    #[test]
    fn the_loop_wakes_for_a_fling_every_frame_else_when_something_turns() {
        let mut app = alone();
        let t0 = Instant::now();
        assert_eq!(
            app.next_wake(t0),
            Some(app.touched + crate::backlight::DIM_AFTER),
            "idle: only to dim the panel"
        );
        app.drawn(Vec::new(), 5000.0, 1280.0);
        // a quick drag up, let go: a fling, a frame at a time
        app.finger_down(10.0, 900.0, t0);
        for i in 1..=5 {
            app.finger_motion(10.0, 900.0 - 60.0 * i as f32, t0 + FRAME * i);
        }
        app.finger_up(10.0, 600.0, t0 + FRAME * 6);
        let now = t0 + FRAME * 6;
        assert_eq!(app.next_wake(now), Some(now + FRAME));
        assert!(app.scroll > 0.0);
    }

    #[test]
    fn a_removed_screen_leaves_the_house_for_its_door() {
        let dir = temp_dir("app-removed");
        let stub = serve(|line| match line {
            l if l.starts_with("POST /api/screens/me") => (
                200,
                vec![],
                r#"{"success":true,"screen":{"name":"Salon"}}"#.into(),
            ),
            // removed in « Mon compte »: every read now says 401
            _ => (401, vec![], r#"{"code":"not_signed_in"}"#.into()),
        });
        std::fs::write(
            dir.join("screen-cookie"),
            format!("{}\nmaison_screen=t\n", stub.origin),
        )
        .unwrap();
        let mut app = App::new(&config(&stub.origin, dir.clone()), Arc::new(|| {}));
        wait_until(|| {
            app.take();
            matches!(app.screen(Instant::now()), Screen::House { .. })
        });
        // the house's reads say 401: the pairing is told, the screen says it was removed
        wait_until(|| {
            app.take();
            app.session.screen == Who::Revoked
        });
        let Screen::Door { title, button, .. } = app.screen(Instant::now()) else {
            panic!("the door")
        };
        assert_eq!(title, app.words.s(k::SCREEN_REVOKED_TITLE));
        assert!(matches!(
            button.map(|b| b.act),
            Some(Act::Pairing(Ask::Pair))
        ));
        assert!(!dir.join("screen-cookie").exists());
    }

    #[test]
    fn every_door_says_something() {
        let w = Words::new(Lang::Fr);
        for who in [
            Who::Checking,
            Who::Code {
                code: "ABCD-2345".into(),
                minutes: 10,
            },
            Who::Expired,
            Who::Failed(crate::client::Failure::Unreachable("x".into())),
            Who::Revoked,
        ] {
            let Screen::Door { title, lead, .. } = door(&w, &who) else {
                panic!("a door")
            };
            assert!(!title.is_empty() && !lead[0].is_empty(), "{who:?}");
        }
    }
}
