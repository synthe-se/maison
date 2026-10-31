//! What the panel shows of the photos, as a pure function of time and of what came: which one is
//! on it, when the next is read, when it is put on the panel, how far the fade has come, when a
//! texture goes. No thread and no GPU here: `Order`s say what the drawing thread must do.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use super::Picture;

/// How long a photo stays, from the start of its fade to the start of the next one's.
pub const HOLD: Duration = Duration::from_secs(20);
/// The crossfade.
pub const FADE: Duration = Duration::from_secs(1);
/// The next photo is read this long before its turn, never more: at most one is decoded ahead,
/// for this long. Far above what a decode takes, so its fade starts on time.
const LEAD: Duration = Duration::from_secs(5);

/// A shuffled deck: each photo once, in random order, before any comes again, and never the same
/// one twice in a row. Not secret, only varied: a small generator, no dependency.
#[derive(Debug)]
struct Bag {
    deck: Vec<String>,
    last: Option<String>,
    seed: u64,
}

impl Bag {
    fn new(seed: u64) -> Self {
        Self {
            deck: Vec::new(),
            last: None,
            // xorshift never leaves 0
            seed: seed | 1,
        }
    }

    /// Below `n`.
    fn roll(&mut self, n: usize) -> usize {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed % n as u64) as usize
    }

    /// The next of `kept` but `bad`; none left, none.
    fn next(&mut self, kept: &[String], bad: &HashSet<String>) -> Option<String> {
        let usable = |id: &String| kept.contains(id) && !bad.contains(id);
        self.deck.retain(usable);
        if self.deck.is_empty() {
            self.deck = kept.iter().filter(|id| usable(id)).cloned().collect();
            for i in (1..self.deck.len()).rev() {
                let j = self.roll(i + 1);
                self.deck.swap(i, j);
            }
            // the top of the deck is its end: a new deck never starts where the last one ended
            let end = self.deck.len().saturating_sub(1);
            if end > 0 && self.deck.last() == self.last.as_ref() {
                self.deck.swap(0, end);
            }
        }
        let id = self.deck.pop()?;
        self.last = Some(id.clone());
        Some(id)
    }
}

/// What the drawing thread must do with the panel's textures.
pub enum Order {
    /// Upload this picture over the one on the panel, then say `staged` (or `not_staged`).
    Load(Picture),
    /// The fade is over: the picture under it goes.
    Settle,
    /// The house is back: every picture goes.
    Clear,
}

/// The next photo, in a rest.
enum Next {
    /// Its time to be read has not come.
    Waiting,
    Asked(String),
    Ready(Picture),
}

enum State {
    Off,
    /// The first photo is being read: the house is still on the panel.
    First(String),
    /// A photo rests until its time is up.
    Rest {
        until: Instant,
        next: Next,
    },
    /// A picture went to the panel.
    Loading(String),
    Fade(Instant),
}

/// What a moment asks.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Step {
    /// Read this photo.
    pub ask: Option<String>,
    /// The same screen, drawn again (a fade's frame).
    pub redraw: bool,
    /// The screen changed.
    pub screen: bool,
}

pub struct Show {
    state: State,
    /// What the panel holds, once it holds a picture.
    on: Option<String>,
    bag: Bag,
    /// What could not be read or drawn: skipped until the app starts again.
    bad: HashSet<String>,
    orders: Vec<Order>,
}

impl Show {
    pub fn new(seed: u64) -> Self {
        Self {
            state: State::Off,
            on: None,
            bag: Bag::new(seed),
            bad: HashSet::new(),
            orders: Vec::new(),
        }
    }

    pub fn on(&self) -> Option<&str> {
        self.on.as_deref()
    }

    /// Nothing started, and there is something to show: the photo to read first.
    pub fn start(&mut self, kept: &[String]) -> Option<String> {
        if !matches!(self.state, State::Off) {
            return None;
        }
        let id = self.bag.next(kept, &self.bad)?;
        self.state = State::First(id.clone());
        Some(id)
    }

    /// The show ends: what the panel holds goes, what is being read will be dropped.
    pub fn stop(&mut self) {
        self.orders.clear();
        if self.on.take().is_some() {
            self.orders.push(Order::Clear);
        }
        self.state = State::Off;
    }

    /// The asked picture was read; one nobody waits for any more is dropped here.
    pub fn came(&mut self, picture: Picture) {
        if self.asked() != Some(picture.id.as_str()) {
            return;
        }
        if let State::Rest { next, .. } = &mut self.state {
            *next = Next::Ready(picture);
        } else {
            self.state = State::Loading(picture.id.clone());
            self.orders.push(Order::Load(picture));
        }
    }

    /// What the reader was asked for and has not answered.
    fn asked(&self) -> Option<&str> {
        match &self.state {
            State::First(id)
            | State::Rest {
                next: Next::Asked(id),
                ..
            } => Some(id),
            _ => None,
        }
    }

    /// `id` could not be read: another one, if any, to read in its place.
    pub fn failed(&mut self, id: &str, kept: &[String]) -> Option<String> {
        self.bad.insert(id.to_string());
        if self.asked() != Some(id) {
            return None;
        }
        if let State::Rest { next, .. } = &mut self.state {
            *next = Next::Waiting;
            None
        } else {
            self.first_again(kept)
        }
    }

    /// The first photo failed: the next one, else the show never started.
    fn first_again(&mut self, kept: &[String]) -> Option<String> {
        self.state = State::Off;
        self.start(kept)
    }

    /// The panel holds the new picture: its fade starts.
    pub fn staged(&mut self, now: Instant) {
        if let State::Loading(id) = &self.state {
            self.on = Some(id.clone());
            self.state = State::Fade(now);
        }
    }

    /// The panel could not take it: skipped; its turn goes to the next one.
    pub fn not_staged(&mut self, now: Instant, kept: &[String]) -> Option<String> {
        let State::Loading(id) = &self.state else {
            return None;
        };
        self.bad.insert(id.clone());
        if self.on.is_some() {
            self.state = State::Rest {
                until: now,
                next: Next::Waiting,
            };
            None
        } else {
            self.first_again(kept)
        }
    }

    pub fn tick(&mut self, now: Instant, kept: &[String]) -> Step {
        let mut step = Step::default();
        match std::mem::replace(&mut self.state, State::Off) {
            State::Fade(from) => {
                step.redraw = true;
                self.state = if now >= from + FADE {
                    self.orders.push(Order::Settle);
                    State::Rest {
                        until: from + HOLD,
                        next: Next::Waiting,
                    }
                } else {
                    State::Fade(from)
                };
            }
            State::Rest {
                until,
                next: Next::Ready(picture),
            } if now >= until => {
                self.state = State::Loading(picture.id.clone());
                self.orders.push(Order::Load(picture));
            }
            State::Rest {
                until,
                next: Next::Waiting,
            } if now + LEAD >= until => match self.bag.next(kept, &self.bad) {
                // the last one was removed: the house again
                None => {
                    self.stop();
                    step.screen = true;
                }
                // only this one: it stays
                Some(id) if self.on.as_ref() == Some(&id) => {
                    self.state = State::Rest {
                        until: now + HOLD,
                        next: Next::Waiting,
                    };
                }
                Some(id) => {
                    step.ask = Some(id.clone());
                    self.state = State::Rest {
                        until,
                        next: Next::Asked(id),
                    };
                }
            },
            other => self.state = other,
        }
        step
    }

    /// When time alone changes something: during a fade, now (the panel paces it); in a rest,
    /// when the next photo is read, then when it comes on. Else a thread's word will wake it.
    pub fn next(&self, now: Instant) -> Option<Instant> {
        match &self.state {
            State::Fade(_) => Some(now),
            State::Rest {
                until,
                next: Next::Waiting,
            } => Some(until.checked_sub(LEAD).unwrap_or(*until)),
            State::Rest {
                until,
                next: Next::Ready(_),
            } => Some(*until),
            _ => None,
        }
    }

    /// How far the picture on the panel has come in over the one under it, eased.
    pub fn fade(&self, now: Instant) -> f32 {
        let State::Fade(from) = self.state else {
            return 1.0;
        };
        let t = (now.saturating_duration_since(from).as_secs_f32() / FADE.as_secs_f32()).min(1.0);
        t * t * (3.0 - 2.0 * t)
    }

    pub fn orders(&mut self) -> Vec<Order> {
        std::mem::take(&mut self.orders)
    }

    /// This photo's time is up now.
    pub fn turn(&mut self, now: Instant) {
        if let State::Rest { until, .. } = &mut self.state {
            *until = now;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    fn picture(id: &str) -> Picture {
        Picture {
            id: id.into(),
            width: 1,
            height: 1,
            rgba: vec![0; 4],
        }
    }

    /// Runs what the drawing thread would: every order obeyed, a load taken at once.
    fn obey(show: &mut Show, now: Instant) -> (usize, usize, usize) {
        let (mut loads, mut settles, mut clears) = (0, 0, 0);
        for order in show.orders() {
            match order {
                Order::Load(_) => {
                    loads += 1;
                    show.staged(now);
                }
                Order::Settle => settles += 1,
                Order::Clear => clears += 1,
            }
        }
        (loads, settles, clears)
    }

    /// A show on its first photo, faded in: its id, and when its fade started.
    fn resting(show: &mut Show, kept: &[String], t0: Instant) -> String {
        let id = show.start(kept).unwrap();
        show.came(picture(&id));
        assert_eq!(obey(show, t0), (1, 0, 0));
        show.tick(t0 + FADE, kept);
        assert_eq!(obey(show, t0 + FADE), (0, 1, 0));
        id
    }

    #[test]
    fn every_photo_comes_once_before_any_comes_again() {
        let kept = ids(&["a", "b", "c", "d", "e"]);
        let (mut bag, bad) = (Bag::new(7), HashSet::new());
        for _ in 0..50 {
            let mut round: Vec<_> = (0..5).map(|_| bag.next(&kept, &bad).unwrap()).collect();
            round.sort();
            assert_eq!(round, kept);
        }
    }

    #[test]
    fn the_same_photo_never_comes_twice_in_a_row() {
        let kept = ids(&["a", "b"]);
        let (mut bag, bad) = (Bag::new(3), HashSet::new());
        let mut last = bag.next(&kept, &bad).unwrap();
        for _ in 0..200 {
            let id = bag.next(&kept, &bad).unwrap();
            assert_ne!(id, last);
            last = id;
        }
    }

    #[test]
    fn the_order_is_random_and_each_run_its_own() {
        let kept: Vec<String> = (0..20).map(|n| n.to_string()).collect();
        let deck = |seed| {
            let (mut bag, bad) = (Bag::new(seed), HashSet::new());
            (0..20)
                .map(|_| bag.next(&kept, &bad).unwrap())
                .collect::<Vec<_>>()
        };
        assert_ne!(deck(1), deck(2));
        assert_ne!(deck(1), kept);
    }

    #[test]
    fn what_was_removed_or_cannot_be_read_is_not_drawn() {
        let mut bag = Bag::new(5);
        let mut bad = HashSet::new();
        let first = bag.next(&ids(&["a", "b", "c"]), &bad).unwrap();
        bad.insert("c".to_string());
        let left = ids(&["a", "b", "c"]);
        for _ in 0..10 {
            let id = bag.next(&left, &bad).unwrap();
            assert_ne!(id, "c", "after {first}");
        }
        assert_eq!(bag.next(&ids(&["c"]), &bad), None);
        assert_eq!(bag.next(&[], &bad), None);
    }

    #[test]
    fn nothing_kept_nothing_starts() {
        let mut show = Show::new(1);
        assert_eq!(show.start(&[]), None);
        assert_eq!(show.on(), None);
        let now = Instant::now();
        assert_eq!(show.tick(now, &[]), Step::default());
        assert_eq!(show.next(now), None);
        assert!(show.orders().is_empty());
    }

    #[test]
    fn the_house_stays_until_the_first_picture_is_on_the_panel() {
        let kept = ids(&["a"]);
        let mut show = Show::new(1);
        let t0 = Instant::now();
        assert_eq!(show.start(&kept).as_deref(), Some("a"));
        assert_eq!(show.start(&kept), None, "started once");
        assert_eq!(show.on(), None);
        assert_eq!(show.next(t0), None, "the reader's word wakes it");
        show.came(picture("a"));
        assert_eq!(show.on(), None, "not yet uploaded");
        obey(&mut show, t0);
        assert_eq!(show.on(), Some("a"));
    }

    #[test]
    fn a_fade_takes_its_second_each_frame_drawn_then_nothing_runs() {
        let kept = ids(&["a", "b"]);
        let mut show = Show::new(1);
        let t0 = Instant::now();
        let id = show.start(&kept).unwrap();
        show.came(picture(&id));
        obey(&mut show, t0);
        assert_eq!(show.fade(t0), 0.0);
        assert_eq!(show.next(t0), Some(t0), "paced by the panel");
        let half = t0 + FADE / 2;
        assert!(show.tick(half, &kept).redraw);
        assert!((show.fade(half) - 0.5).abs() < 0.01);
        assert_eq!(obey(&mut show, half), (0, 0, 0));

        let end = t0 + FADE;
        assert!(show.tick(end, &kept).redraw, "its last frame is drawn");
        assert_eq!(show.fade(end), 1.0);
        assert_eq!(obey(&mut show, end), (0, 1, 0), "the picture under it goes");

        // resting: no drawing, the next wake is the next photo's reading
        let read_at = t0 + HOLD - LEAD;
        assert_eq!(show.next(end), Some(read_at));
        assert_eq!(show.tick(read_at - FADE, &kept), Step::default());
    }

    #[test]
    fn a_photo_stays_its_minute_and_the_next_is_read_just_before_its_turn() {
        let kept = ids(&["a", "b"]);
        let mut show = Show::new(1);
        let t0 = Instant::now();
        let first = resting(&mut show, &kept, t0);
        let second = kept.iter().find(|id| **id != first).unwrap();

        let read_at = t0 + HOLD - LEAD;
        let step = show.tick(read_at, &kept);
        assert_eq!(step.ask.as_ref(), Some(second), "one decoded ahead");
        assert_eq!(show.next(read_at), None, "the reader's word wakes it");
        show.came(picture(second));
        assert_eq!(show.next(read_at), Some(t0 + HOLD));
        assert_eq!(show.tick(read_at, &kept), Step::default(), "not yet");
        assert_eq!(show.on(), Some(first.as_str()), "the panel keeps its photo");

        show.tick(t0 + HOLD, &kept);
        let (loads, settles, _) = obey(&mut show, t0 + HOLD);
        assert_eq!((loads, settles), (1, 0), "the picture under it stays");
        assert_eq!(show.on(), Some(second.as_str()));
        assert_eq!(show.fade(t0 + HOLD), 0.0);
        assert_eq!(show.next(t0 + HOLD), Some(t0 + HOLD));
    }

    #[test]
    fn a_decode_that_ends_late_starts_the_fade_when_it_is_ready() {
        let kept = ids(&["a", "b"]);
        let mut show = Show::new(1);
        let t0 = Instant::now();
        resting(&mut show, &kept, t0);
        let late = t0 + HOLD + LEAD;
        let next = show.tick(t0 + HOLD - LEAD, &kept).ask.unwrap();
        assert_eq!(show.tick(late, &kept), Step::default(), "still reading");
        show.came(picture(&next));
        show.tick(late, &kept);
        assert_eq!(obey(&mut show, late).0, 1);
        assert_eq!(show.on(), Some(next.as_str()));
    }

    #[test]
    fn a_failed_read_skips_that_photo_and_the_next_takes_its_turn() {
        let kept = ids(&["a", "b", "c", "d"]);
        let mut show = Show::new(1);
        let t0 = Instant::now();
        // the first one
        let first = show.start(&kept).unwrap();
        let again = show.failed(&first, &kept).expect("another one");
        assert_ne!(again, first);
        show.came(picture(&again));
        obey(&mut show, t0);
        show.tick(t0 + FADE, &kept);
        obey(&mut show, t0 + FADE);

        // a later one: the same turn asks another
        let at = t0 + HOLD - LEAD;
        let bad = show.tick(at, &kept).ask.unwrap();
        assert_eq!(show.failed(&bad, &kept), None);
        let other = show.tick(at, &kept).ask.unwrap();
        assert!(other != bad && other != first);
    }

    #[test]
    fn nothing_readable_the_show_never_starts() {
        let kept = ids(&["a", "b"]);
        let mut show = Show::new(1);
        let first = show.start(&kept).unwrap();
        let second = show.failed(&first, &kept).unwrap();
        assert_eq!(show.failed(&second, &kept), None);
        assert_eq!(show.on(), None);
        assert_eq!(show.start(&kept), None, "never tried again until a restart");
    }

    #[test]
    fn a_picture_the_panel_refuses_is_skipped() {
        let kept = ids(&["a", "b"]);
        let mut show = Show::new(1);
        let t0 = Instant::now();
        let first = show.start(&kept).unwrap();
        show.came(picture(&first));
        assert!(matches!(show.orders()[..], [Order::Load(_)]));
        let next = show.not_staged(t0, &kept).expect("the other one");
        assert_ne!(next, first);
        assert_eq!(show.on(), None);

        show.came(picture(&next));
        obey(&mut show, t0);
        show.tick(t0 + FADE, &kept);
        obey(&mut show, t0 + FADE);
        // only one left: it stays
        let at = t0 + HOLD - LEAD;
        assert_eq!(show.tick(at, &kept), Step::default());
        assert_eq!(show.next(at), Some(at + HOLD - LEAD));
    }

    #[test]
    fn alone_a_photo_stays_without_a_fade() {
        let kept = ids(&["a"]);
        let mut show = Show::new(1);
        let t0 = Instant::now();
        resting(&mut show, &kept, t0);
        let at = t0 + HOLD - LEAD;
        assert_eq!(show.tick(at, &kept), Step::default());
        assert_eq!(obey(&mut show, at), (0, 0, 0));
        assert_eq!(show.on(), Some("a"));
    }

    #[test]
    fn the_last_photo_removed_the_house_comes_back_at_its_turn() {
        let kept = ids(&["a"]);
        let mut show = Show::new(1);
        let t0 = Instant::now();
        resting(&mut show, &kept, t0);
        let step = show.tick(t0 + HOLD - LEAD, &[]);
        assert!(step.screen);
        assert_eq!(show.on(), None);
        assert_eq!(obey(&mut show, t0), (0, 0, 1), "its texture goes");
    }

    #[test]
    fn a_finger_ends_it_and_what_was_being_read_is_dropped() {
        let kept = ids(&["a", "b"]);
        let mut show = Show::new(1);
        let t0 = Instant::now();
        resting(&mut show, &kept, t0);
        let next = show.tick(t0 + HOLD - LEAD, &kept).ask.unwrap();
        show.stop();
        assert_eq!(show.on(), None);
        assert_eq!(obey(&mut show, t0), (0, 0, 1));
        show.came(picture(&next));
        assert!(show.orders().is_empty(), "too late: dropped");
        assert_eq!(show.next(t0), None);
        // nothing on the panel, nothing to clear
        show.stop();
        assert!(show.orders().is_empty());
        // and the next sleep starts again
        assert!(show.start(&kept).is_some());
    }

    #[test]
    fn a_finger_during_a_fade_drops_the_picture_on_its_way() {
        let kept = ids(&["a", "b"]);
        let mut show = Show::new(1);
        let t0 = Instant::now();
        resting(&mut show, &kept, t0);
        let next = show.tick(t0 + HOLD - LEAD, &kept).ask.unwrap();
        show.came(picture(&next));
        show.tick(t0 + HOLD, &kept);
        show.stop();
        assert_eq!(obey(&mut show, t0), (0, 0, 1), "no upload, only the clear");
    }

    #[test]
    fn a_turn_asked_comes_now() {
        let kept = ids(&["a", "b"]);
        let mut show = Show::new(1);
        let t0 = Instant::now();
        resting(&mut show, &kept, t0);
        let now = t0 + FADE;
        show.turn(now);
        let next = show.tick(now, &kept).ask.unwrap();
        show.came(picture(&next));
        show.tick(now, &kept);
        assert_eq!(obey(&mut show, now).0, 1);
    }
}
