//! The app between SDL's events and the drawing: who the screen is (pairing), what the house
//! says (live), the gestures travelling (commands, sliders), the column's scroll and fling,
//! the toasts; and when it must be drawn again. Nothing here waits: the threads answer over
//! channels and wake the loop.

use std::borrow::Cow;
use std::sync::Arc;
use std::time::{Duration, Instant};

use maison_api::paces;
use maison_api::sun::Side;

use crate::backlight::Backlight;
use crate::client::Client;
use crate::commands::{Commands, Toast};
use crate::config::{Config, Setup};
use crate::families::{self, Cx, Sources};
use crate::i18n::{Words, k};
use crate::live::Live;
use crate::pairing::{Ask, Screen as Who, Session};
use crate::photos::{Order, Photos};
use crate::theme::{self, Palette};
use crate::ui::icon;
use crate::ui::layout::{Hit, Over, REACH, Rect, Target};
use crate::ui::model::{Act, Asked, Button, Screen, View};
use crate::ui::slider::{Sliders, value_at};
use crate::ui::touch::{Finger, Meaning, Tracked, Under};
use crate::wifi::Wifi;

/// How long a toast stays (`paces::TOAST_MS`, the web app's). A warning stays until dismissed
/// on a page; on a wall, which nobody dismisses, `WALL_WARNING_MS`.
const TOAST: Duration = Duration::from_millis(paces::TOAST_MS);
const TOAST_WARN: Duration = Duration::from_millis(paces::WALL_WARNING_MS);
/// A press stays visible at least this long (docs/ux.md § 7).
const PRESSED: Duration = Duration::from_millis(paces::PRESSED_MS);
/// A remote's key held starts repeating after this long.
const REPEAT_DELAY: Duration = Duration::from_millis(paces::KEY_REPEAT_DELAY_MS);
/// The group a chip led to stays marked this long, in case it was on screen already.
const FLASH: Duration = Duration::from_millis(1500);
/// A row's head is at most this high (the panel's pixels): a simulated press lands in it.
const ROW_HEAD: f32 = 64.0;
/// The panel's 60 Hz: a simulated finger's points are this far apart.
pub const FRAME: Duration = Duration::from_millis(16);
/// Time-based words (« il y a 3 min ») turn with the wall clock's minute.
const MINUTE: i64 = 60;
/// A page left open goes back to the house after this long without a finger: a wall screen
/// shows the house to whoever passes by.
pub const PAGE_IDLE: Duration = Duration::from_secs(120);
/// Every API call's whole wait (a device's own limit is shorter: `commands::limit`).
const HTTP_WAIT: Duration = Duration::from_secs(15);

/// A remote's key the finger holds down: what it sends, and when next.
struct Held {
    order: crate::commands::Order,
    every: Duration,
    next: Instant,
    /// It repeated: the lift is no tap.
    repeated: bool,
}

pub struct App {
    pub words: Words,
    live: Live,
    session: Session,
    commands: Commands,
    sliders: Sliders,
    sources: Sources,
    finger: Finger,
    /// Which of SDL's fingers `finger` follows.
    pub tracked: Tracked,
    /// The column's scroll, in the panel's pixels.
    pub scroll: f32,
    /// What the finger went down on, as it was then: a drawing in between (a read came, the
    /// house redrew) moves the hits, never what the finger holds.
    pressed: Option<(Under, Hit)>,
    /// Where the press is shown, and until when at least, however short the tap.
    look: Option<(Rect, Instant)>,
    held: Option<Held>,
    /// The question a dialog asks.
    asking: Option<Asked>,
    /// « Maintenant » shown whole.
    unfolded: bool,
    /// The group a chip led to, until when.
    flash: Option<(&'static str, Instant)>,
    /// How far the column scrolls to each anchored group.
    anchors: Vec<(&'static str, f32)>,
    /// The Tempo calendar's month, once another was asked (`year * 12 + month0`).
    month: Option<i32>,
    /// The house, or one of its pages; and when a finger last touched the panel.
    view: View,
    touched: Instant,
    /// Dimmed while nobody uses the frame; `waking`: the finger that woke it, which does nothing
    /// else until it lifts.
    backlight: Backlight,
    waking: bool,
    /// The family's photos, shown while the panel sleeps.
    photos: Photos,
    /// The frame's network, and the screens that choose it.
    wifi: Wifi,
    /// The way to them, for what offers it.
    wifi_entry: Option<Button>,
    /// The column's height and the panel's, from the last drawing.
    content: f32,
    height: f32,
    hits: Vec<Hit>,
    /// The toasts shown, each until when.
    toasts: Vec<(Toast, Instant)>,
    /// The sun's side, kept once known (a page signed out reads it too).
    side: Option<Side>,
    /// Something changed since the last drawing: the screen is made again.
    pub dirty: bool,
    /// Only the scroll changed: the last screen, drawn again.
    pub scrolled: bool,
    /// When the screen shown was made, and when its time-based words turn (signed in).
    looked: Instant,
    minute: Option<Instant>,
}

impl App {
    pub fn new(config: &Config, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let client = config.origin.as_ref().map_or_else(
            || Client::nowhere(HTTP_WAIT),
            |origin| Client::new(origin, &config.state_dir, HTTP_WAIT),
        );
        let photos = Photos::new(&client, &config.state_dir, wake.clone());
        let wifi = Wifi::new(config.wifi_socket.clone(), wake.clone());
        let mut live = Live::new(client.clone(), wake.clone());
        let session = if config.origin.is_some() {
            // what the server says changed: read again at once, the polls a safety net
            live.listen();
            Session::start(client.clone(), wake.clone())
        } else {
            Session::idle(if config.setup.ssid.is_some() {
                Who::Setup(config.setup.clone())
            } else {
                Who::NoAddress
            })
        };
        Self {
            words: Words::new(config.lang),
            live,
            session,
            commands: Commands::new(client, wake),
            sliders: Sliders::default(),
            sources: Sources::new(),
            finger: Finger::default(),
            tracked: Tracked::default(),
            scroll: 0.0,
            pressed: None,
            look: None,
            held: None,
            asking: None,
            unfolded: false,
            flash: None,
            anchors: Vec::new(),
            month: None,
            view: View::House,
            touched: Instant::now(),
            backlight: Backlight::open(std::path::Path::new("/sys/class/backlight")),
            waking: false,
            photos,
            wifi,
            wifi_entry: None,
            content: 0.0,
            height: 0.0,
            hits: Vec::new(),
            toasts: Vec::new(),
            side: None,
            dirty: true,
            scrolled: false,
            looked: Instant::now(),
            minute: None,
        }
    }

    /// A drawing is owed.
    pub fn owed(&self) -> bool {
        self.dirty || self.scrolled
    }

    /// What the threads said since; the screen is drawn again if anything changed.
    pub fn take(&mut self) {
        let (paired, revoked) = (
            self.session.screen == Who::Paired,
            self.session.screen == Who::Revoked,
        );
        let mut changed = self.session.take();
        let now = Instant::now();
        if paired != (self.session.screen == Who::Paired) {
            // what the finger held is on another screen now
            self.end_gesture(now);
        }
        if !paired && self.session.screen == Who::Paired {
            // refused until now: the event stream opens at once, not after its wait
            self.live.reopen();
        }
        if !revoked && self.session.screen == Who::Revoked {
            // the screen was removed: the house goes, its photos too, the pairing says so
            self.live.clear();
            self.photos.forget();
            self.open(View::House, now);
        }
        self.photos.take();
        let shown = self.wifi.shown();
        changed |= self.wifi.take(now);
        self.wifi_turned(shown, now);
        let came = self.live.take();
        changed |= came.changed;
        for (key, asked) in &came.read {
            changed |= self.commands.read(key, *asked);
        }
        let mut toasts = Vec::new();
        let live = &self.live;
        changed |= self
            .commands
            .take(&self.words, &mut |f| live.refresh(f), &mut toasts);
        for t in toasts {
            self.toast(t, now);
        }
        // one read's 401 is no verdict: the pairing asks who the screen is
        if self.live.signed_out() && self.session.screen == Who::Paired {
            self.session.recheck();
        }
        self.dirty |= changed;
    }

    /// The Wi-Fi's screens came or went: what the finger held, and any question, went with what was
    /// under them.
    fn wifi_turned(&mut self, shown: bool, now: Instant) {
        self.wifi_entry = self.wifi.entry(&self.words);
        if shown != self.wifi.shown() {
            self.scroll = 0.0;
            self.asking = None;
            self.end_gesture(now);
            self.dirty = true;
        }
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
        self.backlight.idle(now, self.touched, self.night());
        if now.saturating_duration_since(self.touched) >= PAGE_IDLE {
            if self.view != View::House {
                self.open(View::House, now);
            }
            self.dirty |= self.asking.take().is_some() || std::mem::take(&mut self.unfolded);
            if self.wifi.opened() {
                let shown = self.wifi.shown();
                self.wifi.leave(now);
                self.wifi_turned(shown, now);
                self.dirty = true;
            }
        }
        if self.pressed.is_none() && self.look.is_some_and(|(_, until)| now >= until) {
            self.look = None;
            self.scrolled = true;
        }
        if self.flash.is_some_and(|(_, until)| now >= until) {
            self.flash = None;
            self.scrolled = true;
        }
        self.repeat(now);
        if self.finger.flinging() {
            let max = self.max_scroll();
            if let Some(next) = self.finger.step(self.scroll, max, now) {
                self.scrolled |= next != self.scroll;
                self.scroll = next;
            }
        }
        self.dirty |= self.commands.sweep(now);
        for t in self.commands.overdue(&self.words, now) {
            self.toast(t, now);
        }
        let step = self
            .photos
            .tick(now, self.may_show_photos(), self.backlight.asleep());
        self.scrolled |= step.redraw;
        self.dirty |= step.screen;
        // what turns with time alone wakes the loop for this
        self.dirty |= self.next_look().is_some_and(|t| t <= now);
    }

    /// A remote's key still held: sent again, unless the last one is still on its way (a repeat
    /// never queues: the cursor stops when the finger does).
    fn repeat(&mut self, now: Instant) {
        let Some(h) = self.held.as_mut().filter(|h| now >= h.next) else {
            return;
        };
        h.next = now + h.every;
        h.repeated = true;
        if self.commands.state(&h.order.key) == crate::commands::State::Idle {
            self.commands.run(h.order.clone());
        }
    }

    /// The next moment what the screen shows changes by itself, since it was made.
    fn next_look(&self) -> Option<Instant> {
        [
            self.live.next_stale(self.looked),
            self.live.next_gone(self.looked),
            self.commands.next_change(self.looked),
            self.sliders.next_change(self.looked),
            self.minute,
        ]
        .into_iter()
        .flatten()
        .min()
    }

    /// When the loop must wake by itself, if nothing else happens. While a fling runs, at once:
    /// the present is paced by the panel.
    pub fn next_wake(&self, now: Instant) -> Option<Instant> {
        if self.finger.flinging() {
            return Some(now);
        }
        [
            self.next_look(),
            self.toasts.iter().map(|(_, until)| *until).min(),
            (self.view != View::House
                || self.asking.is_some()
                || self.unfolded
                || self.wifi.opened())
            .then(|| self.touched + PAGE_IDLE),
            self.backlight.next(self.touched),
            self.photos.next(now),
            self.look
                .filter(|_| self.pressed.is_none())
                .map(|(_, until)| until),
            self.flash.map(|(_, until)| until),
            self.held.as_ref().map(|h| h.next),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    fn night(&self) -> bool {
        self.side == Some(Side::Dark)
    }

    /// A screen in the house shows its photos, Maison there or not; a door, never.
    fn may_show_photos(&self) -> bool {
        matches!(self.session.screen, Who::Paired | Who::Waiting)
    }

    pub fn photo_orders(&mut self) -> Vec<Order> {
        self.photos.orders()
    }

    /// The panel took the photo: its fade starts, and the house's sources rest (`Screen::Photos`
    /// reads none of them).
    pub fn photo_staged(&mut self, now: Instant) {
        self.photos.staged(now);
        self.dirty = true;
    }

    pub fn photo_not_staged(&mut self, now: Instant) {
        self.photos.not_staged(now);
    }

    pub fn photo_fade(&self, now: Instant) -> f32 {
        self.photos.fade(now)
    }

    /// A simulation's hurry: nobody has touched the panel for long enough.
    pub fn nobody_since(&mut self, now: Instant) {
        self.touched = now
            .checked_sub(crate::backlight::DIM_AFTER)
            .unwrap_or(self.touched);
    }

    /// A simulation's hurry: the next photo now.
    pub fn photo_turn(&mut self, now: Instant) {
        self.photos.turn(now);
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

    /// What goes over the screen in a drawing at `now`.
    pub fn over(&self, now: Instant) -> Over<'_> {
        Over {
            pressed: self
                .look
                .filter(|(_, until)| self.pressed.is_some() || now < *until)
                .map(|(r, _)| r),
            gone: self.session.screen == Who::Paired && self.live.gone(now) && !self.wifi.shown(),
            asking: self.asking.as_ref(),
            unfolded: self.unfolded,
            flash: self.flash.map(|(id, _)| id),
            wifi: self.wifi_entry.as_ref(),
        }
    }

    pub fn toasts(&self) -> Vec<Toast> {
        self.toasts.iter().map(|(t, _)| t.clone()).collect()
    }

    fn max_scroll(&self) -> f32 {
        (self.content - self.height).max(0.0)
    }

    /// What the screen shows now, from who it is and what the house says.
    pub fn screen(&mut self, now: Instant) -> Screen {
        self.looked = now;
        let paired = self.session.screen == Who::Paired;
        let photo = self.photos.on().map(str::to_owned);
        self.minute = (paired || photo.is_some()).then(|| {
            let into = chrono::Utc::now().timestamp().rem_euclid(MINUTE);
            now + Duration::from_secs((MINUTE - into) as u64)
        });
        self.live.begin();
        let mut cx = Cx {
            live: &mut self.live,
            words: &self.words,
            sources: &mut self.sources,
            commands: &self.commands,
            month: self.month,
            now,
        };
        // the cache follows Maison's list, photos shown or not
        let list = paired.then(|| cx.live.want(&cx.sources.photos).data);
        let wifi = self.wifi.screen(&self.words);
        let (side, mut screen) = match (&self.session.screen, photo, wifi) {
            (Who::Paired | Who::Waiting, Some(photo), _) => {
                let side = if paired {
                    families::sun_signed_in(&mut cx)
                } else {
                    families::sun_signed_out(&mut cx)
                };
                let time = self.words.clock(cx.house_now().time());
                (side, Screen::Photos { time, photo })
            }
            // whoever the frame is: nothing behind it can be reached without a network
            (_, None, Some(wifi)) => (None, wifi),
            (Who::Paired, None, None) => (
                families::sun_signed_in(&mut cx),
                match &self.view {
                    View::House => {
                        let now = cx.house_now();
                        Screen::House {
                            time: self.words.clock(now.time()),
                            date: self.words.long_date(now.date_naive()),
                            now: families::now::chips(&mut cx, &now),
                            groups: families::house(&mut cx),
                        }
                    }
                    View::Tempo => families::tempo::page(&mut cx),
                    View::Device(id) => families::cats::page(&mut cx, id),
                    View::Climate => families::climate::page(&mut cx),
                    View::Tv => families::tv::tv_page(&mut cx),
                    View::Box => families::tv::box_page(&mut cx),
                    View::Lamp(family, id) => families::lamps::page(&mut cx, family, id),
                    View::Plug(id) => families::plugs::page(&mut cx, id),
                },
            ),
            (who @ (Who::Setup(_) | Who::NoAddress), ..) => (None, door(&self.words, who)),
            (who, ..) => (families::sun_signed_out(&mut cx), door(&self.words, who)),
        };
        if let Screen::Door { link, .. } = &mut screen {
            *link = self.wifi_entry.clone().map(Box::new);
        }
        self.photos.list(list.flatten());
        if side.is_some() {
            self.side = side;
            self.backlight.idle(now, self.touched, self.night());
        }
        self.live.sweep();
        screen
    }

    /// What the last drawing left: where the controls are, how tall the column is, where the
    /// groups a chip leads to are.
    pub fn drawn(&mut self, drawn: crate::ui::layout::Drawn, height: f32) {
        self.hits = drawn.hits;
        self.content = drawn.content;
        self.anchors = drawn.anchors;
        self.height = height;
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());
    }

    /// The centre of the control whose gesture is `key` (a simulation's `press`).
    pub fn center_of(&self, key: &str) -> Option<(f32, f32)> {
        let h = self
            .hits
            .iter()
            .rev()
            .find(|h| h.what.key().as_deref() == Some(key))?;
        // in a row, its head, past its words: what is under and in it has targets of its own
        Some((h.x + h.w * 0.75, h.y + h.h.min(ROW_HEAD) / 2.0))
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
            .filter_map(|h| Some((h.what.key()?.into_owned(), h.x + h.w / 2.0, h.y + h.h / 2.0)))
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
                    .filter(|(_, h)| !matches!(h.what, Target::Cover))
                    .map(|(i, h)| (i, h.distance(x, y)))
                    .filter(|(_, d)| *d <= REACH)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(i, _)| i)
            });
        match on {
            Some(i) => match self.hits[i].what {
                Target::Slider { .. } => Under::Slider(i),
                Target::Act(_) => Under::Hit(i),
                Target::Cover => Under::Nothing,
            },
            None => Under::Nothing,
        }
    }

    // ── the finger, in the panel's pixels as the picture shows them ──

    pub fn finger_down(&mut self, x: f32, y: f32, now: Instant) {
        self.touched = now;
        // asleep: this finger only wakes the panel
        if self.backlight.wake(self.night()) {
            self.photos.stop();
            self.waking = true;
            self.dirty = true;
            self.wifi.touched(now);
            return;
        }
        self.wifi.touched(now);
        let under = self.under(x, y);
        self.pressed = match under {
            Under::Hit(i) | Under::Slider(i) => self.hits.get(i).map(|h| (under, h.clone())),
            Under::Nothing => None,
        };
        if let Some((Under::Hit(_), h)) = &self.pressed {
            self.look = Some(((h.x, h.y, h.w, h.h), now + PRESSED));
            self.scrolled = true;
            if let Target::Act(Act::Key(order, Some(every))) = &h.what {
                self.held = Some(Held {
                    order: order.clone(),
                    every: *every,
                    next: now + REPEAT_DELAY,
                    repeated: false,
                });
            }
        }
        self.finger.down(x, y, under, self.scroll, now);
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
        self.held = None;
        self.dirty = true;
    }

    /// The system took the finger back: whatever it was doing ends, nothing is sent.
    pub fn finger_cancel(&mut self, now: Instant) {
        self.end_gesture(now);
        self.dirty = true;
    }

    /// No finger held any more, whatever it held.
    fn end_gesture(&mut self, now: Instant) {
        self.finger = Finger::default();
        self.pressed = None;
        self.look = None;
        self.held = None;
        self.waking = false;
        self.sliders.cancel(now);
    }

    /// What a finger at (x, y) would land on, by its key (a log's words).
    pub fn key_under(&self, x: f32, y: f32) -> String {
        match self.under(x, y) {
            // what a finger types stays out of the log
            Under::Hit(i) if matches!(self.hits[i].what, Target::Act(Act::Keyboard(_))) => {
                "a key".to_string()
            }
            Under::Hit(i) | Under::Slider(i) => self.hits[i]
                .what
                .key()
                .map_or_else(|| "a control".to_string(), Cow::into_owned),
            Under::Nothing => "nothing".to_string(),
        }
    }

    /// What the finger holds (its target as it went down).
    fn held_target(&self) -> Option<Target> {
        self.pressed.as_ref().map(|(_, h)| h.what.clone())
    }

    fn meant(&mut self, m: Meaning, now: Instant) {
        match m {
            Meaning::None => {}
            Meaning::Scrolled(s) => {
                if s != self.scroll {
                    // a drag: what it started on is no longer pressed
                    self.scrolled = true;
                    self.look = None;
                    self.held = None;
                }
                self.scroll = s;
            }
            Meaning::Tap(_) => {
                let repeated = self.held.as_ref().is_some_and(|h| h.repeated);
                if let Some(Target::Act(act)) = self.held_target()
                    && !(repeated && matches!(act, Act::Key(..)))
                {
                    self.act(act, now);
                }
            }
            Meaning::SlideStart(_, x) | Meaning::Slide(_, x) | Meaning::SlideEnd(_, x) => {
                let Some(Target::Slider { slider, x0, x1 }) = self.held_target() else {
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

    fn act(&mut self, act: Act, now: Instant) {
        match act {
            Act::Order(o) | Act::Key(o, _) => {
                // a new try clears what the tile said of the last one
                self.commands.forget(&o.key);
                self.commands.run(o);
            }
            Act::Ask(a) => self.asking = Some(*a),
            Act::Keep => self.asking = None,
            Act::Sure => {
                if let Some(a) = self.asking.take() {
                    self.commands.forget(&a.order.key);
                    self.commands.run(a.order);
                }
            }
            Act::Jump(to) => {
                if let Some((_, y)) = self.anchors.iter().find(|(id, _)| *id == to) {
                    self.scroll = y.clamp(0.0, self.max_scroll());
                    self.flash = Some((to, now + FLASH));
                }
            }
            Act::Unfold => self.unfolded = true,
            Act::Month(m) => self.month = Some(m),
            Act::Refresh(prefixes) => {
                for p in prefixes {
                    self.live.refresh(p);
                }
            }
            Act::Pairing(ask) => self.session.ask(ask),
            Act::Open(view) => self.open(view, now),
            Act::Wifi(act) => {
                let shown = self.wifi.shown();
                self.wifi.act(&act, now);
                self.wifi_turned(shown, now);
            }
            Act::Keyboard(key) => self.wifi.key(key),
        }
        self.dirty = true;
    }

    /// A simulation's fingers on the keyboard.
    pub fn type_text(&mut self, text: &str) {
        self.wifi.type_text(text);
    }

    /// Shows `view`, from its top; a finger held on the last one holds nothing here.
    fn open(&mut self, view: View, now: Instant) {
        self.view = view;
        self.scroll = 0.0;
        self.month = None;
        self.asking = None;
        self.end_gesture(now);
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
            k::SCREEN_WAITING_TITLE,
            vec![w.s(k::COMMON_LOADING)],
            None,
            None,
            None,
            false,
        ),
        Who::Waiting => (
            k::SCREEN_WAITING_TITLE,
            vec![w.s(k::SCREEN_WAITING_BODY)],
            None,
            None,
            None,
            false,
        ),
        Who::Setup(s) => (
            k::SCREEN_SETUP_TITLE,
            setup_lead(w, s),
            None,
            None,
            None,
            false,
        ),
        Who::NoAddress => (
            k::SCREEN_NO_ADDRESS_TITLE,
            vec![w.s(k::SCREEN_NO_ADDRESS_BODY)],
            None,
            None,
            None,
            false,
        ),
        Who::Code { code, minutes } => (
            k::SCREEN_PAIR_TITLE,
            vec![w.s(k::SCREEN_PAIR_INTRO)],
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
            vec![w.s(k::SCREEN_PAIR_EXPIRED)],
            None,
            None,
            pair(&w.s(k::SCREEN_PAIR_AGAIN), Ask::Again),
            false,
        ),
        Who::Failed(f) => (
            k::SCREEN_PAIR_TITLE,
            vec![w.failure(f)],
            None,
            None,
            pair(&w.s(k::SCREEN_PAIR_AGAIN), Ask::Again),
            true,
        ),
        Who::Revoked => (
            k::SCREEN_REVOKED_TITLE,
            vec![w.s(k::SCREEN_REVOKED_BODY)],
            None,
            None,
            pair(&w.s(k::SCREEN_PAIR_RETRY), Ask::Pair),
            false,
        ),
    };
    Screen::Door {
        title: w.s(title),
        lead,
        code,
        hint,
        button,
        warn,
        link: None,
    }
}

/// What the setup view says: join the frame's Wi-Fi, and each thing the frame was told of it.
fn setup_lead(w: &Words, s: &Setup) -> Vec<String> {
    let line = |key: &str, name: &str, value: &Option<String>| {
        value.as_deref().map(|v| w.t(key, &[(name, v.into())]))
    };
    [
        Some(w.s(k::SCREEN_SETUP_JOIN)),
        line(k::SCREEN_SETUP_NETWORK, "ssid", &s.ssid),
        line(k::SCREEN_SETUP_PASSWORD, "password", &s.password),
        line(k::SCREEN_SETUP_OPEN, "url", &s.url),
    ]
    .into_iter()
    .flatten()
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::stub::serve;
    use crate::i18n::Lang;
    use crate::photos::cache::Cache;
    use crate::photos::decode::tests::RED;
    use crate::photos::show::FADE;
    use crate::testing::{temp_dir, wait_until};
    use crate::ui::keyboard::Key;
    use crate::ui::model::{Slider, WifiAct};
    use crate::wifi::tests::fake;
    use maison_frame_tools::wifi::Status;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn config(origin: &str, state_dir: PathBuf) -> Config {
        Config {
            origin: Some(origin.into()),
            setup: Setup::default(),
            wifi_socket: "/nowhere/socket".into(),
            state_dir,
            lang: Lang::Fr,
            rotate: false,
            scale: 1.0,
            window: None,
            heartbeat: None,
        }
    }

    /// An app whose Maison answers nothing at all.
    fn alone() -> App {
        App::new(
            &config("http://127.0.0.1:9", temp_dir("app")),
            Arc::new(|| {}),
        )
    }

    fn drawing(hits: Vec<Hit>, content: f32) -> crate::ui::layout::Drawn {
        crate::ui::layout::Drawn {
            hits,
            content,
            anchors: Vec::new(),
        }
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
        app.drawn(drawing(Vec::new(), 3000.0), 1280.0);
        assert_eq!(app.scroll, 3000.0 - 1280.0);
        app.scroll = -40.0;
        app.drawn(drawing(Vec::new(), 3000.0), 1280.0);
        assert_eq!(app.scroll, 0.0);
        // a column shorter than the panel does not scroll
        app.scroll = 100.0;
        app.drawn(drawing(Vec::new(), 600.0), 1280.0);
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
            drawing(
                vec![Hit {
                    x: 0.0,
                    y: 0.0,
                    w: 800.0,
                    h: 1280.0,
                    what: Target::Act(Act::Open(View::Tempo)),
                }],
                1280.0,
            ),
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

    /// A slider's band across the panel (20..780 at y 100..144), a lamp's.
    fn slider_hit() -> Hit {
        Hit {
            x: 0.0,
            y: 100.0,
            w: 800.0,
            h: 44.0,
            what: Target::Slider {
                slider: Slider {
                    key: "l".into(),
                    value: 50.0,
                    min: 0.0,
                    max: 100.0,
                    step: 1.0,
                    mark: None,
                    live: true,
                    put: false,
                    off: false,
                    near: 2.0,
                    ends: None,
                    path: "/l".into(),
                    body: |_| serde_json::Value::Null,
                    refresh: "l",
                    name: "l".into(),
                },
                x0: 20.0,
                x1: 780.0,
            },
        }
    }

    #[test]
    fn neither_a_tap_nor_a_swipe_moves_a_slider() {
        let mut app = alone();
        app.drawn(drawing(vec![slider_hit()], 5000.0), 1280.0);
        app.scroll = 500.0;
        let Target::Slider { slider: s, .. } = app.hits[0].what.clone() else {
            unreachable!()
        };
        let t0 = Instant::now();
        app.finger_down(600.0, 120.0, t0);
        app.finger_up(600.0, 120.0, t0 + FRAME);
        assert_eq!(app.sliders.shown(&s, t0 + FRAME), 50.0, "a tap");
        app.finger_down(600.0, 120.0, t0);
        app.finger_motion(604.0, 300.0, t0 + FRAME);
        app.finger_up(604.0, 320.0, t0 + FRAME * 2);
        assert_eq!(app.sliders.shown(&s, t0 + FRAME * 2), 50.0, "a swipe");
        assert_eq!(app.commands.state("l:set"), crate::commands::State::Idle);
        assert_eq!(app.scroll, 320.0, "the swipe scrolled the column");
    }

    #[test]
    fn a_cancel_or_another_view_lets_the_slider_go() {
        let mut app = alone();
        app.drawn(drawing(vec![slider_hit()], 5000.0), 1280.0);
        let Target::Slider { slider: s, .. } = app.hits[0].what.clone() else {
            unreachable!()
        };
        let t0 = Instant::now();
        app.finger_down(100.0, 120.0, t0);
        app.finger_motion(600.0, 120.0, t0 + FRAME);
        assert_ne!(app.sliders.shown(&s, t0 + FRAME), 50.0);
        app.finger_cancel(t0 + FRAME);
        assert!(app.pressed.is_none());
        app.finger_motion(700.0, 120.0, t0 + FRAME * 2);
        app.finger_up(700.0, 120.0, t0 + FRAME * 3);
        assert!(!app.finger.held(), "its motion and its lift mean nothing");
        // a page gone from under the finger
        app.finger_down(100.0, 120.0, t0);
        app.finger_motion(400.0, 120.0, t0 + FRAME);
        app.open(View::Tempo, t0 + FRAME);
        assert!(app.pressed.is_none());
        let dragging = app.sliders.next_change(t0 + crate::ui::slider::HOLD * 2);
        assert_eq!(dragging, None, "nothing held for ever");
    }

    #[test]
    fn a_cancelled_wake_lets_the_next_finger_act() {
        let mut app = alone();
        app.drawn(
            drawing(
                vec![Hit {
                    x: 0.0,
                    y: 0.0,
                    w: 800.0,
                    h: 1280.0,
                    what: Target::Act(Act::Open(View::Tempo)),
                }],
                1280.0,
            ),
            1280.0,
        );
        let asleep = app.touched + crate::backlight::DIM_AFTER;
        app.tick(asleep);
        app.finger_down(400.0, 600.0, asleep);
        app.finger_cancel(asleep);
        app.finger_down(400.0, 600.0, asleep + FRAME);
        app.finger_up(400.0, 600.0, asleep + FRAME * 2);
        assert_eq!(app.view, View::Tempo);
    }

    fn hit_of(what: Target) -> Hit {
        Hit {
            x: 100.0,
            y: 100.0,
            w: 200.0,
            h: 60.0,
            what,
        }
    }

    fn order(key: &str) -> crate::commands::Order {
        crate::commands::Order::one(key, "x", crate::commands::Post::empty("/x"))
    }

    #[test]
    fn a_press_is_shown_its_time_however_short_the_tap() {
        let mut app = alone();
        app.drawn(
            drawing(vec![hit_of(Target::Act(Act::Refresh(&[])))], 1280.0),
            1280.0,
        );
        let t0 = app.touched + Duration::from_millis(1);
        app.scrolled = false;
        app.finger_down(150.0, 120.0, t0);
        assert!(app.owed(), "drawn pressed at once");
        assert_eq!(app.over(t0).pressed, Some((100.0, 100.0, 200.0, 60.0)));
        app.finger_up(150.0, 120.0, t0 + FRAME);
        assert!(app.over(t0 + FRAME).pressed.is_some(), "a 16 ms tap");
        assert_eq!(app.next_wake(t0 + FRAME), Some(t0 + PRESSED));
        assert!(app.over(t0 + PRESSED).pressed.is_none());
        app.scrolled = false;
        app.tick(t0 + PRESSED);
        assert!(app.scrolled, "drawn released, the screen not remade");
        assert!(app.look.is_none());
        // a drag lets go of what it started on; a slider and a cover show no press
        app.drawn(
            drawing(vec![hit_of(Target::Act(Act::Refresh(&[])))], 5000.0),
            1280.0,
        );
        app.finger_down(150.0, 120.0, t0);
        app.finger_motion(150.0, 60.0, t0 + FRAME);
        assert!(app.over(t0 + FRAME).pressed.is_none());
        app.finger_up(150.0, 60.0, t0 + FRAME * 2);
        app.drawn(drawing(vec![hit_of(Target::Cover)], 1280.0), 1280.0);
        app.finger_down(150.0, 120.0, t0);
        assert!(app.look.is_none());
    }

    #[test]
    fn a_held_key_repeats_paced_by_its_answers_and_a_tap_sends_once() {
        let stub = serve(|_| (200, vec![], "{}".into()));
        let mut app = App::new(&config(&stub.origin, temp_dir("app-key")), Arc::new(|| {}));
        let every = Duration::from_millis(100);
        let key = Act::Key(order("tv:key:up"), Some(every));
        app.drawn(drawing(vec![hit_of(Target::Act(key))], 1280.0), 1280.0);
        let sent = || {
            let asked = stub.asked.lock().unwrap();
            asked.iter().filter(|a| a.line.contains("/api/x ")).count()
        };
        // a tap: one, on the lift
        let t0 = app.touched + Duration::from_millis(1);
        app.finger_down(150.0, 120.0, t0);
        assert_eq!(sent(), 0);
        app.finger_up(150.0, 120.0, t0 + FRAME);
        wait_until(|| {
            app.take();
            sent() == 1 && app.commands.state("tv:key:up") == crate::commands::State::Idle
        });
        // held: nothing until the delay, then every `every`, the lift adds nothing
        app.finger_down(150.0, 120.0, t0);
        assert_eq!(app.next_wake(t0), Some(t0 + REPEAT_DELAY));
        app.tick(t0 + REPEAT_DELAY / 2);
        assert_eq!(sent(), 1);
        app.tick(t0 + REPEAT_DELAY);
        wait_until(|| {
            app.take();
            sent() == 2 && app.commands.state("tv:key:up") == crate::commands::State::Idle
        });
        // on its way, a repeat is dropped, never queued
        app.commands.run(order("tv:key:up"));
        let before = sent();
        app.tick(t0 + REPEAT_DELAY + every);
        app.finger_up(150.0, 120.0, t0 + REPEAT_DELAY + every);
        wait_until(|| {
            app.take();
            app.commands.state("tv:key:up") == crate::commands::State::Idle
        });
        assert!(sent() <= before + 1, "only the one already sent");
        assert!(app.held.is_none());
    }

    #[test]
    fn what_cannot_be_undone_is_asked_first() {
        let stub = serve(|_| (200, vec![], "{}".into()));
        let mut app = App::new(&config(&stub.origin, temp_dir("app-ask")), Arc::new(|| {}));
        let ask = |key: &str| {
            Act::Ask(Box::new(Asked {
                title: "Remettre à zéro ?".into(),
                text: "Le temps compté sera perdu.".into(),
                action: "Remettre à zéro".into(),
                order: order(key),
            }))
        };
        let t0 = Instant::now();
        app.act(ask("tuya:f1:water"), t0);
        assert!(app.over(t0).asking.is_some());
        assert_eq!(
            app.commands.state("tuya:f1:water"),
            crate::commands::State::Idle
        );
        // « Garder »: nothing went
        app.act(Act::Keep, t0);
        assert!(app.asking.is_none());
        let asked = stub.asked.lock().unwrap();
        assert!(!asked.iter().any(|a| a.line.contains("/api/x ")));
        drop(asked);
        app.act(ask("tuya:f1:water"), t0);
        app.act(Act::Sure, t0);
        assert!(app.asking.is_none());
        assert_ne!(
            app.commands.state("tuya:f1:water"),
            crate::commands::State::Idle
        );
        // left open, a wall's dialog goes by itself
        app.act(ask("tuya:f1:filter"), t0);
        app.tick(app.touched + PAGE_IDLE);
        assert!(app.asking.is_none());
    }

    #[test]
    fn a_chip_scrolls_to_its_group_and_more_opens() {
        let mut app = alone();
        app.drawn(
            crate::ui::layout::Drawn {
                hits: Vec::new(),
                content: 3000.0,
                anchors: vec![("lamps", 400.0), ("plugs", 9000.0)],
            },
            1280.0,
        );
        let t0 = app.touched;
        app.act(Act::Jump("lamps"), t0);
        assert_eq!(app.scroll, 400.0);
        assert_eq!(
            app.over(t0).flash,
            Some("lamps"),
            "marked, were it on screen"
        );
        assert_eq!(app.next_wake(t0), Some(t0 + FLASH));
        app.tick(t0 + FLASH);
        assert_eq!(app.over(t0 + FLASH).flash, None);
        app.act(Act::Jump("plugs"), t0);
        assert_eq!(app.scroll, 3000.0 - 1280.0, "inside the column");
        app.act(Act::Jump("nowhere"), t0);
        assert_eq!(app.scroll, 3000.0 - 1280.0);
        assert!(!app.over(t0).unfolded);
        app.act(Act::Unfold, t0);
        assert!(app.over(t0).unfolded);
        app.tick(t0 + PAGE_IDLE);
        assert!(!app.over(t0).unfolded, "folded again once nobody is there");
    }

    #[test]
    fn a_cover_stops_a_finger_and_reaches_nothing() {
        let mut app = alone();
        let band = |y: f32, what: Target| Hit {
            x: 0.0,
            y,
            w: 800.0,
            h: 60.0,
            what,
        };
        let open = Target::Act(Act::Open(View::Tempo));
        app.drawn(
            drawing(vec![band(60.0, open), band(0.0, Target::Cover)], 1280.0),
            1280.0,
        );
        assert_eq!(app.under(400.0, 30.0), Under::Nothing, "on the header");
        assert_eq!(app.under(400.0, 70.0), Under::Hit(0));
        // just past the covered band's edge, a control is still within reach
        app.hits.reverse();
        app.hits[1].y = 80.0;
        assert_eq!(app.under(400.0, 70.0), Under::Hit(1));
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
        app.drawn(drawing(Vec::new(), 5000.0), 1280.0);
        // a quick drag up, let go: a fling, a frame at a time
        app.finger_down(10.0, 900.0, t0);
        for i in 1..=5 {
            app.finger_motion(10.0, 900.0 - 60.0 * i as f32, t0 + FRAME * i);
        }
        app.finger_up(10.0, 600.0, t0 + FRAME * 6);
        let now = t0 + FRAME * 6;
        assert_eq!(app.next_wake(now), Some(now), "the present paces it");
        assert!(app.scroll > 0.0);
    }

    #[test]
    fn only_the_scroll_moved_the_last_screen_is_drawn_again() {
        let mut app = alone();
        app.drawn(drawing(Vec::new(), 5000.0), 1280.0);
        app.dirty = false;
        let t0 = Instant::now();
        app.finger_down(10.0, 900.0, t0);
        app.finger_motion(10.0, 700.0, t0 + FRAME);
        assert!(app.scrolled && !app.dirty && app.owed());
        // a finger that moves nothing owes no drawing
        app.scrolled = false;
        app.finger_motion(10.0, 700.0, t0 + FRAME * 2);
        assert!(!app.owed());
    }

    #[test]
    fn what_turns_with_time_is_drawn_when_it_turns() {
        let mut app = alone();
        let t0 = Instant::now();
        app.looked = t0;
        let order = crate::commands::Order::one("k", "x", crate::commands::Post::empty("/x"));
        app.commands.run(order);
        let slow = crate::commands::SLOW;
        assert_eq!(
            app.next_look().map(|t| t.duration_since(t0) >= slow),
            Some(true)
        );
        app.dirty = false;
        let due = app.next_look().unwrap();
        app.tick(due - Duration::from_millis(1));
        assert!(!app.dirty, "not yet");
        app.tick(due);
        assert!(app.dirty, "the gesture went slow: drawn, no read needed");
        // drawn at that moment: the next change is the next one
        app.looked = due;
        assert!(app.next_look().is_some_and(|t| t > due));
    }

    #[test]
    fn signed_in_the_words_turn_with_the_minute() {
        let mut app = alone();
        assert_eq!(app.minute, None);
        app.session.screen = Who::Paired;
        let t0 = Instant::now();
        app.screen(t0);
        let next = app.minute.expect("a minute's tick");
        assert!(next > t0 && next <= t0 + Duration::from_secs(60));
        app.dirty = false;
        app.tick(next);
        assert!(app.dirty);
    }

    fn photo_id(n: u8) -> String {
        format!("{n:0>64}")
    }

    /// An app in the house, with one photo in its cache.
    fn with_photo(name: &str, who: Who) -> App {
        let dir = temp_dir(name);
        Cache::new(&dir).save(&photo_id(1), RED).unwrap();
        let mut app = App::new(&config("http://127.0.0.1:9", dir), Arc::new(|| {}));
        app.session = Session::idle(who);
        app
    }

    /// Asleep at `now`, as the loop does it: time passes, the reader answers, the panel takes
    /// the picture.
    fn photos_on(app: &mut App, now: Instant) {
        app.tick(now);
        wait_until(|| {
            app.take();
            let loaded = app
                .photo_orders()
                .iter()
                .any(|o| matches!(o, Order::Load(_)));
            if loaded {
                app.photo_staged(now);
            }
            loaded
        });
    }

    #[test]
    fn nobody_at_the_frame_the_photos_show_and_a_finger_only_brings_the_house_back() {
        let mut app = with_photo("app-photos", Who::Paired);
        let open = Target::Act(Act::Open(View::Tempo));
        let whole = Hit {
            x: 0.0,
            y: 0.0,
            w: 800.0,
            h: 1280.0,
            what: open,
        };
        app.drawn(drawing(vec![whole], 1280.0), 1280.0);
        let before = app.touched + crate::backlight::DIM_AFTER - FRAME;
        app.tick(before);
        assert!(matches!(app.screen(before), Screen::House { .. }));
        let asleep = app.touched + crate::backlight::DIM_AFTER;
        assert!(app.next_wake(asleep) <= Some(asleep), "it wakes to dim");

        photos_on(&mut app, asleep);
        assert!(app.dirty, "the screen changes");
        let Screen::Photos { time, photo } = app.screen(asleep) else {
            panic!("the photos")
        };
        assert_eq!(photo, photo_id(1));
        assert!(time.contains(':'), "{time}");
        assert_eq!(app.photo_fade(asleep), 0.0);

        let touch = asleep + Duration::from_secs(5);
        app.drawn(drawing(Vec::new(), 0.0), 1280.0);
        app.finger_down(400.0, 600.0, touch);
        app.finger_up(400.0, 600.0, touch + FRAME);
        assert_eq!(app.view, View::House, "the waking tap opened nothing");
        assert!(app.owed());
        assert!(matches!(app.screen(touch), Screen::House { .. }));
        assert!(
            app.photo_orders().iter().any(|o| matches!(o, Order::Clear)),
            "its textures go"
        );
    }

    #[test]
    fn between_two_fades_nothing_is_drawn_but_the_minute() {
        let mut app = with_photo("app-photos-idle", Who::Paired);
        let asleep = app.touched + crate::backlight::DIM_AFTER;
        photos_on(&mut app, asleep);
        app.screen(asleep);
        app.dirty = false;

        // the fade: every frame, the same screen
        assert_eq!(app.next_wake(asleep), Some(asleep), "paced by the panel");
        app.tick(asleep + FADE / 2);
        assert!(app.scrolled && !app.dirty);
        app.scrolled = false;
        app.tick(asleep + FADE);
        assert!(app.scrolled, "its last frame");
        assert!(
            app.photo_orders()
                .iter()
                .any(|o| matches!(o, Order::Settle))
        );

        // then only the minute, and this one photo's turn
        app.scrolled = false;
        let rest = asleep + FADE * 2;
        app.tick(rest);
        assert!(!app.owed(), "at rest, no drawing");
        let next = app.next_wake(rest).unwrap();
        assert!(next > rest, "at rest, no wake");
        assert_eq!(Some(next), app.minute.min(app.photos.next(rest)));
        let minute = app.minute.unwrap();
        app.tick(minute);
        assert!(app.dirty, "the clock's minute");
        assert!(matches!(app.screen(minute), Screen::Photos { .. }));
    }

    #[test]
    fn while_the_photos_show_the_house_is_not_read() {
        let mut app = with_photo("app-photos-rest", Who::Paired);
        let t0 = app.touched;
        app.screen(t0);
        assert!(app.live.reading().contains(&"meross:list"));
        let asleep = t0 + crate::backlight::DIM_AFTER;
        photos_on(&mut app, asleep);
        app.screen(asleep);
        let mut reading = app.live.reading();
        reading.sort_unstable();
        assert_eq!(
            reading,
            ["photos:list", "shutters:place"],
            "the sun's side for the backlight, the list for the cache"
        );
        app.finger_down(1.0, 1.0, asleep);
        app.screen(asleep);
        assert!(app.live.reading().contains(&"meross:list"), "and back");
    }

    #[test]
    fn with_no_photo_or_at_a_door_the_panel_only_dims() {
        let dir = temp_dir("app-no-photo");
        let mut none = App::new(&config("http://127.0.0.1:9", dir), Arc::new(|| {}));
        none.session = Session::idle(Who::Paired);
        let asleep = none.touched + crate::backlight::DIM_AFTER;
        none.tick(asleep);
        none.take();
        assert!(none.backlight.asleep());
        assert!(matches!(none.screen(asleep), Screen::House { .. }));
        assert_eq!(none.photos.next(asleep), None);

        let mut door = with_photo("app-photo-door", Who::Expired);
        let asleep = door.touched + crate::backlight::DIM_AFTER;
        door.tick(asleep);
        assert!(door.backlight.asleep());
        assert_eq!(door.photos.on(), None);
        assert!(door.photo_orders().is_empty());
    }

    #[test]
    fn maison_away_the_photos_go_on_from_the_cache() {
        let mut app = with_photo("app-photos-away", Who::Waiting);
        let asleep = app.touched + crate::backlight::DIM_AFTER;
        photos_on(&mut app, asleep);
        assert!(matches!(app.screen(asleep), Screen::Photos { .. }));
        assert!(app.minute.is_some(), "the clock still turns");
        // told to pair again, the photos stop there
        app.session = Session::idle(Who::Expired);
        app.tick(asleep + FRAME);
        assert_eq!(app.photos.on(), None);
        assert!(app.dirty);
        assert!(matches!(app.screen(asleep), Screen::Door { .. }));
    }

    #[test]
    fn the_cache_follows_maisons_list_and_the_show_with_it() {
        let id = photo_id(2);
        let list = format!(r#"{{"success":true,"photos":[{{"id":"{id}","bytes":777}}]}}"#);
        let stub = serve(move |line| match line {
            l if l.starts_with("GET /api/photos ") => (200, vec![], list.clone()),
            l if l.starts_with("GET /api/photos/") => {
                let jpeg = String::from_utf8_lossy(RED).into_owned();
                (200, vec![], jpeg)
            }
            _ => (503, vec![], String::new()),
        });
        let dir = temp_dir("app-photos-list");
        let cache = Cache::new(&dir);
        cache.save(&photo_id(1), RED).unwrap();
        let mut app = App::new(&config(&stub.origin, dir), Arc::new(|| {}));
        app.session = Session::idle(Who::Paired);
        wait_until(|| {
            app.screen(Instant::now());
            app.take();
            cache.ids() == [id.as_str()]
        });
        wait_until(|| {
            app.take();
            app.photos.kept() == [id.as_str()]
        });
    }

    #[test]
    fn a_removed_screen_leaves_the_house_for_its_door() {
        let dir = temp_dir("app-removed");
        let cache = Cache::new(&dir);
        cache.save(&photo_id(1), RED).unwrap();
        let removed = Arc::new(AtomicBool::new(false));
        let gone = removed.clone();
        let stub = serve(move |line| match line {
            l if l.starts_with("POST /api/screens/me") && !gone.load(Ordering::SeqCst) => (
                200,
                vec![],
                r#"{"success":true,"screen":{"name":"Salon"}}"#.into(),
            ),
            // removed in « Mon compte »: every read now says 401, and `/me` once it is asked
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
        // one read's 401 alone removes nothing: `/me` still knows the screen
        let asked_me = || {
            let asked = stub.asked.lock().unwrap();
            asked.iter().filter(|a| a.line.contains("/me")).count()
        };
        wait_until(|| {
            app.take();
            asked_me() >= 2
        });
        assert_eq!(app.session.screen, Who::Paired);
        assert!(dir.join("screen-cookie").exists());
        // the house's reads say 401 and `/me` agrees: the screen says it was removed
        removed.store(true, Ordering::SeqCst);
        wait_until(|| {
            app.screen(Instant::now());
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
        wait_until(|| cache.ids().is_empty());
    }

    #[test]
    fn every_door_says_something() {
        let w = Words::new(Lang::Fr);
        for who in [
            Who::Checking,
            Who::Waiting,
            Who::Setup(Setup::default()),
            Who::NoAddress,
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

    #[test]
    fn a_paired_screen_that_waits_for_maison_is_offered_no_new_code() {
        let w = Words::new(Lang::Fr);
        let Screen::Door {
            title,
            lead,
            button,
            warn,
            ..
        } = door(&w, &Who::Waiting)
        else {
            panic!("a door")
        };
        assert_eq!(title, w.s(k::SCREEN_WAITING_TITLE));
        assert_eq!(lead, [w.s(k::SCREEN_WAITING_BODY)]);
        assert!(button.is_none() && !warn, "calm, and nothing to do");
    }

    /// An app whose `maison-wifid` is `f`, once it heard that the frame is `status`.
    fn with_wifi(name: &str, f: &crate::wifi::tests::Fake, status: Status) -> App {
        *f.status.lock().unwrap() = status;
        let mut c = config("http://127.0.0.1:9", temp_dir(name));
        c.wifi_socket = f.socket.clone();
        let mut app = App::new(&c, Arc::new(|| {}));
        wait_until(|| {
            app.take();
            app.wifi_entry.is_some()
        });
        app
    }

    fn lost() -> Status {
        Status {
            configured: true,
            lost: true,
            ..Status::default()
        }
    }

    fn on_a_network() -> Status {
        Status {
            connected: Some("Ma Box".into()),
            configured: true,
            ..Status::default()
        }
    }

    #[test]
    fn a_frame_that_lost_its_network_shows_its_wifi_whoever_it_is_and_nothing_over_it() {
        let f = fake();
        let mut app = with_wifi("app-wifi", &f, lost());
        app.session = Session::idle(Who::Paired);
        let t0 = Instant::now();
        assert!(matches!(app.screen(t0), Screen::Wifi { .. }));
        assert!(!app.over(t0).gone, "no band over the keyboard");
        app.session = Session::idle(Who::Expired);
        assert!(matches!(app.screen(t0), Screen::Wifi { .. }));
        // the network is back: the house, by itself
        *f.status.lock().unwrap() = on_a_network();
        app.session = Session::idle(Who::Paired);
        wait_until_status(&mut app);
        assert!(matches!(app.screen(t0), Screen::House { .. }));
    }

    /// The thread's next word, at its own 15 s pace: a rescan asks it at once.
    fn wait_until_status(app: &mut App) {
        let now = Instant::now();
        app.act(Act::Wifi(WifiAct::Rescan), now);
        wait_until(|| {
            app.take();
            !app.wifi.shown()
        });
    }

    #[test]
    fn every_door_offers_the_wifi_but_the_frames_own_and_a_house_that_answers_does_not() {
        let f = fake();
        let mut app = with_wifi("app-wifi-door", &f, on_a_network());
        app.session = Session::idle(Who::Waiting);
        let t0 = Instant::now();
        let link = |s: Screen| match s {
            Screen::Door { link, .. } => link.map(|b| b.act),
            _ => panic!("a door"),
        };
        let open = link(app.screen(t0)).unwrap();
        app.session = Session::idle(Who::NoAddress);
        assert!(link(app.screen(t0)).is_some());
        assert!(app.over(t0).wifi.is_some());
        app.act(open, t0);
        assert!(matches!(app.screen(t0), Screen::Wifi { .. }));
        // the way back, to the door it left
        app.act(Act::Wifi(WifiAct::Back), t0);
        assert!(matches!(app.screen(t0), Screen::Door { .. }));
        // without a service, nothing to offer
        let mut plain = alone();
        plain.session = Session::idle(Who::Waiting);
        assert!(link(plain.screen(t0)).is_none());
    }

    #[test]
    fn a_hand_on_the_wifi_is_let_go_after_a_pages_time_with_what_it_typed() {
        let f = fake();
        let mut app = with_wifi("app-wifi-idle", &f, on_a_network());
        app.session = Session::idle(Who::Waiting);
        let t0 = app.touched;
        app.act(Act::Wifi(WifiAct::Open), t0);
        let choose = WifiAct::Choose {
            ssid: "Autre".into(),
            locked: true,
        };
        app.act(Act::Wifi(choose), t0);
        app.type_text("sel de mer");
        assert!(matches!(app.screen(t0), Screen::Password(_)));
        assert_eq!(app.next_wake(t0), Some(t0 + PAGE_IDLE));
        app.tick(t0 + PAGE_IDLE);
        assert!(app.dirty);
        assert!(matches!(app.screen(t0), Screen::Door { .. }));
        assert!(!app.wifi.opened(), "nothing else to wait for");
    }

    #[test]
    fn the_keys_a_finger_presses_are_typed_and_never_logged() {
        let f = fake();
        let mut app = with_wifi("app-wifi-key", &f, on_a_network());
        let t0 = app.touched;
        app.act(Act::Wifi(WifiAct::Open), t0);
        let choose = WifiAct::Choose {
            ssid: "Autre".into(),
            locked: true,
        };
        app.act(Act::Wifi(choose), t0);
        let key = Key::Char('a');
        app.drawn(
            drawing(vec![hit_of(Target::Act(Act::Keyboard(key)))], 1280.0),
            1280.0,
        );
        assert_eq!(app.key_under(150.0, 120.0), "a key");
        assert_eq!(
            app.center_of("kb:a"),
            Some((250.0, 130.0)),
            "a simulation's"
        );
        let t1 = t0 + Duration::from_millis(1);
        app.finger_down(150.0, 120.0, t1);
        app.finger_up(150.0, 120.0, t1 + FRAME);
        let Screen::Password(p) = app.screen(t1) else {
            panic!("the password")
        };
        assert_eq!(p.field, "•");
    }

    #[test]
    fn nobody_there_the_photos_go_on_over_a_lost_network_and_a_finger_finds_the_wifi() {
        let f = fake();
        let dir = temp_dir("app-wifi-photos");
        Cache::new(&dir).save(&photo_id(1), RED).unwrap();
        *f.status.lock().unwrap() = lost();
        let mut c = config("http://127.0.0.1:9", dir);
        c.wifi_socket = f.socket.clone();
        let mut app = App::new(&c, Arc::new(|| {}));
        wait_until(|| {
            app.take();
            app.wifi.shown()
        });
        app.session = Session::idle(Who::Waiting);
        let asleep = app.touched + crate::backlight::DIM_AFTER;
        photos_on(&mut app, asleep);
        assert!(matches!(app.screen(asleep), Screen::Photos { .. }));
        app.drawn(drawing(Vec::new(), 0.0), 1280.0);
        let touch = asleep + Duration::from_secs(5);
        app.finger_down(400.0, 600.0, touch);
        app.finger_up(400.0, 600.0, touch + FRAME);
        assert!(matches!(app.screen(touch), Screen::Wifi { .. }));
    }

    #[test]
    fn without_a_target_and_its_own_wifi_the_app_says_so_and_never_asks_for_it() {
        let mut c = config("", temp_dir("app-no-address"));
        c.origin = None;
        let mut app = App::new(&c, Arc::new(|| {}));
        let Screen::Door {
            title,
            lead,
            button,
            ..
        } = app.screen(Instant::now())
        else {
            panic!("a door")
        };
        assert_eq!(title, app.words.s(k::SCREEN_NO_ADDRESS_TITLE));
        assert_eq!(lead, [app.words.s(k::SCREEN_NO_ADDRESS_BODY)]);
        assert!(button.is_none());
    }

    #[test]
    fn its_own_wifi_shows_the_setup_view_and_offers_no_other() {
        let mut c = config("", temp_dir("app-setup"));
        c.origin = None;
        c.setup = Setup {
            ssid: Some("maison-frame".into()),
            password: Some("sel-de-mer".into()),
            url: Some("http://10.42.0.1".into()),
        };
        let mut app = App::new(&c, Arc::new(|| {}));
        let Screen::Door { title, lead, .. } = app.screen(Instant::now()) else {
            panic!("a door")
        };
        assert_eq!(title, app.words.s(k::SCREEN_SETUP_TITLE));
        assert_eq!(lead.len(), 4);
        assert!(lead[1].contains("maison-frame"));
        assert!(lead[2].contains("sel-de-mer"));
        assert!(lead[3].contains("http://10.42.0.1"));
        // what it was not told, it does not say
        let bare = setup_lead(&app.words, &Setup::default());
        assert_eq!(bare, [app.words.s(k::SCREEN_SETUP_JOIN)]);
        // and it reads nothing
        app.take();
        assert_eq!(app.next_wake(app.touched), app.backlight.next(app.touched));
    }
}
