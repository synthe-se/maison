//! The panel's backlight, dimmed while nobody uses the frame: after `DIM_AFTER` without a finger
//! it goes down (a quarter of its level by day, nearly nothing at night: the sun's side), and the
//! first touch brings it back, that touch doing nothing else (waking the panel never turns a
//! lamp off). The level it wakes to is the one it had when the app started. Written through
//! /sys/class/backlight (the `video` group may: the frame's udev rule); a panel without one (a
//! simulation) dims nothing and still says it is asleep.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long without a finger before the panel dims.
pub const DIM_AFTER: Duration = Duration::from_secs(120);
/// The dimmed level, a share of the awake one: by day, by night.
const DAY: f32 = 0.25;
const NIGHT: f32 = 0.04;

pub struct Backlight {
    /// `/sys/class/backlight/<name>/brightness`, when the panel has one.
    file: Option<PathBuf>,
    awake: u32,
    asleep: bool,
}

impl Backlight {
    /// The first backlight under `class` (`/sys/class/backlight`), at its present level.
    pub fn open(class: &Path) -> Self {
        let file = std::fs::read_dir(class)
            .ok()
            .and_then(|mut d| d.find_map(|e| e.ok()))
            .map(|e| e.path().join("brightness"));
        let awake = file
            .as_deref()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        Self {
            file,
            awake,
            asleep: false,
        }
    }

    #[cfg(test)]
    pub fn asleep(&self) -> bool {
        self.asleep
    }

    /// The level it sleeps at: a share of the awake one, never off (the panel stays readable).
    pub fn dimmed(&self, night: bool) -> u32 {
        let share = if night { NIGHT } else { DAY };
        ((self.awake as f32 * share).round() as u32).clamp(1, self.awake.max(1))
    }

    fn write(&self, level: u32) {
        if let Some(f) = &self.file
            && let Err(e) = std::fs::write(f, level.to_string())
        {
            eprintln!("backlight: {}: {e}", f.display());
        }
    }

    /// Asleep, `touched` long enough ago: dimmed (by `night` or day). True when it just fell asleep.
    pub fn idle(&mut self, now: Instant, touched: Instant, night: bool) -> bool {
        if self.asleep || now.saturating_duration_since(touched) < DIM_AFTER {
            return false;
        }
        self.asleep = true;
        self.write(self.dimmed(night));
        true
    }

    /// A finger: awake again. True when it was asleep (the touch then only wakes it).
    pub fn wake(&mut self) -> bool {
        if !self.asleep {
            return false;
        }
        self.asleep = false;
        self.write(self.awake);
        true
    }

    /// When it will fall asleep, if awake.
    pub fn next(&self, touched: Instant) -> Option<Instant> {
        (!self.asleep).then(|| touched + DIM_AFTER)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::temp_dir;

    fn panel(level: &str) -> (PathBuf, Backlight) {
        let class = temp_dir("backlight");
        std::fs::create_dir_all(class.join("backlight")).unwrap();
        std::fs::write(class.join("backlight/brightness"), level).unwrap();
        let b = Backlight::open(&class);
        (class.join("backlight/brightness"), b)
    }

    #[test]
    fn idle_long_enough_it_dims_and_a_touch_brings_it_back() {
        let (file, mut b) = panel("128\n");
        let t0 = Instant::now();
        assert!(!b.idle(t0 + Duration::from_secs(60), t0, false), "not yet");
        assert!(b.idle(t0 + DIM_AFTER, t0, false));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "32");
        assert!(b.asleep());
        assert!(!b.idle(t0 + DIM_AFTER * 2, t0, false), "once");
        assert!(b.wake(), "the touch only wakes it");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "128");
        assert!(!b.wake(), "awake: a touch acts");
    }

    #[test]
    fn at_night_nearly_nothing_never_off() {
        let (_, b) = panel("128");
        assert_eq!(b.dimmed(true), 5);
        let (_, low) = panel("10");
        assert_eq!(low.dimmed(true), 1);
    }

    #[test]
    fn without_a_backlight_it_still_sleeps_and_wakes() {
        let mut b = Backlight::open(Path::new("/nonexistent"));
        let t0 = Instant::now();
        assert!(b.idle(t0 + DIM_AFTER, t0, true));
        assert!(b.wake());
    }
}
