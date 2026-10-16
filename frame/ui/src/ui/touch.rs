//! One finger on the screen, as a phone reads it: a tap acts where it went down and came up
//! (within a little slop), a drag past the slop scrolls the column and lets go with its speed
//! (a fling that slows down), a finger down on a slider holds it until it lifts. Pure: fed the
//! finger's points with their times, it says what they mean; the app draws and acts.

use std::time::{Duration, Instant};

/// Past this many pixels a finger is dragging, not tapping: Android's touch slop (8 dp) at 1.5 px
/// a dp (hdpi), in the panel's pixels.
pub const SLOP: f32 = 12.0;
/// A fling slower than this (px/s) stops at once.
const MIN_FLING: f32 = 60.0;
/// How fast a fling slows: its speed falls by e every this long (iOS's « normal » deceleration).
const FLING_DECAY: Duration = Duration::from_millis(325);
/// Speeds are measured over the last moments of a drag only.
const SPEED_WINDOW: Duration = Duration::from_millis(100);

/// What the finger went down on, as the last drawing laid it out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Under {
    Nothing,
    /// A control, by its index in the drawing's hits.
    Hit(usize),
    /// A slider, by its index: it holds the finger.
    Slider(usize),
}

/// What the finger means.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Meaning {
    None,
    /// The column scrolled to this offset.
    Scrolled(f32),
    /// Tapped: the hit it went down and came up on.
    Tap(usize),
    /// A slider held, at this x (start, move); let go at this x (end).
    SlideStart(usize, f32),
    Slide(usize, f32),
    SlideEnd(usize, f32),
}

#[derive(Debug, Clone, Copy)]
struct Down {
    x: f32,
    y: f32,
    under: Under,
    scroll: f32,
    dragging: bool,
    /// Where the drag last put the column: still `scroll` when nothing could move.
    scrolled: f32,
}

#[derive(Debug, Default)]
pub struct Finger {
    down: Option<Down>,
    /// Recent points of a drag: (time, y).
    trail: Vec<(Instant, f32)>,
    /// A fling's speed (px/s, positive scrolls down the content) and its last step.
    fling: Option<(f32, Instant)>,
}

impl Finger {
    /// Down at (x, y) on `under`, the column at `scroll`. A fling running stops (as a phone's).
    pub fn down(&mut self, x: f32, y: f32, under: Under, scroll: f32, at: Instant) -> Meaning {
        self.fling = None;
        self.trail.clear();
        self.trail.push((at, y));
        self.down = Some(Down {
            x,
            y,
            under,
            scroll,
            dragging: false,
            scrolled: scroll,
        });
        match under {
            Under::Slider(i) => Meaning::SlideStart(i, x),
            _ => Meaning::None,
        }
    }

    /// Moved to (x, y); `max` the furthest the column scrolls.
    pub fn motion(&mut self, x: f32, y: f32, max: f32, at: Instant) -> Meaning {
        let Some(d) = self.down.as_mut() else {
            return Meaning::None;
        };
        if let Under::Slider(i) = d.under {
            return Meaning::Slide(i, x);
        }
        if !d.dragging && ((x - d.x).powi(2) + (y - d.y).powi(2)).sqrt() > SLOP {
            d.dragging = true;
        }
        self.trail.push((at, y));
        self.trail
            .retain(|(t, _)| at.duration_since(*t) <= SPEED_WINDOW);
        if d.dragging {
            d.scrolled = (d.scroll - (y - d.y)).clamp(0.0, max.max(0.0));
            Meaning::Scrolled(d.scrolled)
        } else {
            Meaning::None
        }
    }

    /// Up at (x, y); `still_under`: what is under the finger now (a tap counts only on the
    /// control it went down on).
    pub fn up(&mut self, x: f32, y: f32, still_under: Under, at: Instant) -> Meaning {
        let Some(d) = self.down.take() else {
            return Meaning::None;
        };
        match d.under {
            Under::Slider(i) => Meaning::SlideEnd(i, x),
            // a finger that wandered past the slop but moved nothing (a column that does not
            // scroll, a touch panel's jitter) still tapped what it pressed
            Under::Hit(i) if still_under == d.under && (!d.dragging || d.scrolled == d.scroll) => {
                Meaning::Tap(i)
            }
            _ if d.dragging => {
                self.trail.push((at, y));
                self.trail
                    .retain(|(t, _)| at.duration_since(*t) <= SPEED_WINDOW);
                let speed = match (self.trail.first(), self.trail.last()) {
                    (Some((t0, y0)), Some((t1, y1))) if t1 > t0 => {
                        -(y1 - y0) / t1.duration_since(*t0).as_secs_f32()
                    }
                    _ => 0.0,
                };
                if speed.abs() >= MIN_FLING {
                    self.fling = Some((speed, at));
                }
                Meaning::None
            }
            _ => Meaning::None,
        }
    }

    /// The fling's next step at `now`, from `scroll` within 0..=max: where the column goes, or
    /// `None` once it stopped (slow enough, or at an end).
    pub fn step(&mut self, scroll: f32, max: f32, now: Instant) -> Option<f32> {
        let (speed, last) = self.fling?;
        let dt = now.saturating_duration_since(last).as_secs_f32();
        let decay = (-dt / FLING_DECAY.as_secs_f32()).exp();
        // the distance travelled while slowing from `speed` to `speed * decay`
        let next =
            (scroll + speed * FLING_DECAY.as_secs_f32() * (1.0 - decay)).clamp(0.0, max.max(0.0));
        let speed = speed * decay;
        let at_end = next <= 0.0 || next >= max;
        self.fling = (speed.abs() >= MIN_FLING && !at_end).then_some((speed, now));
        Some(next)
    }

    /// A fling is running: the screen draws every frame until it stops.
    pub fn flinging(&self) -> bool {
        self.fling.is_some()
    }

    /// A finger is down.
    #[cfg(test)]
    pub fn held(&self) -> bool {
        self.down.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(t0: Instant, n: u64) -> Instant {
        t0 + Duration::from_millis(n)
    }

    #[test]
    fn a_finger_that_wanders_but_scrolls_nothing_still_taps() {
        let t0 = Instant::now();
        let mut f = Finger::default();
        f.down(100.0, 100.0, Under::Hit(3), 0.0, t0);
        // a touch panel's jitter, past the slop, on a column that does not scroll
        assert_eq!(
            f.motion(118.0, 110.0, 0.0, ms(t0, 40)),
            Meaning::Scrolled(0.0)
        );
        assert_eq!(
            f.up(118.0, 110.0, Under::Hit(3), ms(t0, 80)),
            Meaning::Tap(3)
        );
        // the same move on a column that scrolls is a drag: no tap
        f.down(100.0, 100.0, Under::Hit(3), 50.0, t0);
        f.motion(100.0, 80.0, 400.0, ms(t0, 40));
        assert_eq!(f.up(100.0, 80.0, Under::Hit(3), ms(t0, 80)), Meaning::None);
    }

    #[test]
    fn a_tap_acts_on_what_it_went_down_and_came_up_on() {
        let t0 = Instant::now();
        let mut f = Finger::default();
        assert_eq!(f.down(100.0, 200.0, Under::Hit(3), 0.0, t0), Meaning::None);
        // a small wobble is still a tap
        assert_eq!(f.motion(104.0, 205.0, 1000.0, ms(t0, 30)), Meaning::None);
        assert_eq!(
            f.up(104.0, 205.0, Under::Hit(3), ms(t0, 60)),
            Meaning::Tap(3)
        );
        // up elsewhere: nothing
        f.down(100.0, 200.0, Under::Hit(3), 0.0, t0);
        assert_eq!(f.up(100.0, 200.0, Under::Hit(4), ms(t0, 60)), Meaning::None);
    }

    #[test]
    fn a_drag_scrolls_and_never_taps() {
        let t0 = Instant::now();
        let mut f = Finger::default();
        f.down(100.0, 500.0, Under::Hit(1), 300.0, t0);
        assert_eq!(
            f.motion(100.0, 450.0, 1000.0, ms(t0, 20)),
            Meaning::Scrolled(350.0)
        );
        // within bounds
        assert_eq!(
            f.motion(100.0, -2000.0, 1000.0, ms(t0, 40)),
            Meaning::Scrolled(1000.0)
        );
        assert_eq!(
            f.motion(100.0, 2000.0, 1000.0, ms(t0, 60)),
            Meaning::Scrolled(0.0)
        );
        assert_eq!(
            f.up(100.0, 2000.0, Under::Hit(1), ms(t0, 80)),
            Meaning::None
        );
    }

    #[test]
    fn a_fling_goes_on_slows_and_stops() {
        let t0 = Instant::now();
        let mut f = Finger::default();
        f.down(0.0, 800.0, Under::Nothing, 0.0, t0);
        // 600 px up in 100 ms: 6000 px/s down the content
        for i in 1..=5 {
            f.motion(0.0, 800.0 - 120.0 * i as f32, 10_000.0, ms(t0, 20 * i));
        }
        f.up(0.0, 200.0, Under::Nothing, ms(t0, 100));
        assert!(f.flinging());
        let mut scroll = 600.0;
        let mut t = 100;
        let mut steps = 0;
        while let Some(next) = f.step(scroll, 10_000.0, ms(t0, t)) {
            assert!(next >= scroll, "a fling goes one way");
            scroll = next;
            t += 16;
            steps += 1;
            assert!(steps < 1000, "a fling stops");
        }
        // about speed × decay time further
        assert!((scroll - 600.0 - 6000.0 * 0.325).abs() < 200.0, "{scroll}");
        assert!(!f.flinging());
        // a finger down stops a fling at once
        f.fling = Some((5000.0, ms(t0, 0)));
        f.down(0.0, 0.0, Under::Nothing, scroll, ms(t0, 5));
        assert!(!f.flinging());
    }

    #[test]
    fn a_fling_stops_at_the_end() {
        let t0 = Instant::now();
        let mut f = Finger {
            fling: Some((5000.0, t0)),
            ..Default::default()
        };
        assert_eq!(f.step(990.0, 1000.0, ms(t0, 100)), Some(1000.0));
        assert!(!f.flinging());
    }

    #[test]
    fn a_slider_holds_the_finger() {
        let t0 = Instant::now();
        let mut f = Finger::default();
        assert_eq!(
            f.down(10.0, 10.0, Under::Slider(2), 0.0, t0),
            Meaning::SlideStart(2, 10.0)
        );
        // a vertical drag on a slider never scrolls
        assert_eq!(
            f.motion(60.0, 300.0, 1000.0, ms(t0, 10)),
            Meaning::Slide(2, 60.0)
        );
        assert_eq!(
            f.up(70.0, 300.0, Under::Nothing, ms(t0, 20)),
            Meaning::SlideEnd(2, 70.0)
        );
        assert!(!f.held());
    }
}
