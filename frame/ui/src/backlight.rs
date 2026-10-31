//! The panel's backlight, following the sun and whoever uses the frame: after `DIM_AFTER` without a
//! finger it goes down (a quarter of its level by day, nearly nothing at night: the sun's side), and
//! the first touch brings it back, that touch doing nothing else (waking the panel never turns a
//! lamp off). Night and day are followed asleep and awake: a touch in a dark room lights the panel
//! to `NIGHT_AWAKE`, never to the full level. Written through /sys/class/backlight (the `video`
//! group may: the frame's udev rule); a panel without one (a simulation) dims nothing and still
//! says it is asleep.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long without a finger before the panel dims.
pub const DIM_AFTER: Duration = Duration::from_secs(120);
/// The dimmed level, a share of the awake one: by day, by night.
const DAY: f32 = 0.25;
const NIGHT: f32 = 0.04;
/// Awake at night, a share of the awake level.
const NIGHT_AWAKE: f32 = 0.4;

pub struct Backlight {
    /// `/sys/class/backlight/<name>/brightness`, when the panel has one.
    file: Option<PathBuf>,
    awake: u32,
    /// What the panel was last told.
    level: u32,
    asleep: bool,
}

impl Backlight {
    /// The first backlight under `class` (`/sys/class/backlight`). Its awake level is the panel's
    /// default (half of `max_brightness`: the device tree's 128 of 255), or what it is at now if
    /// higher, never what it is at alone: a restart while dimmed would keep the dim for good.
    pub fn open(class: &Path) -> Self {
        let dir = std::fs::read_dir(class)
            .ok()
            .and_then(|mut d| d.find_map(|e| e.ok()))
            .map(|e| e.path());
        let read = |name: &str| {
            let text = std::fs::read_to_string(dir.as_ref()?.join(name)).ok()?;
            text.trim().parse::<u32>().ok()
        };
        let level = read("brightness").unwrap_or(0);
        let default = read("max_brightness").unwrap_or(0).div_ceil(2);
        Self {
            file: dir.as_ref().map(|d| d.join("brightness")),
            awake: level.max(default),
            level,
            asleep: false,
        }
    }

    pub fn asleep(&self) -> bool {
        self.asleep
    }

    /// `share` of the awake level, never off (the panel stays readable).
    fn share(&self, share: f32) -> u32 {
        ((self.awake as f32 * share).round() as u32).clamp(1, self.awake.max(1))
    }

    /// The level it sleeps at.
    pub fn dimmed(&self, night: bool) -> u32 {
        self.share(if night { NIGHT } else { DAY })
    }

    /// The level it wakes to.
    pub fn lit(&self, night: bool) -> u32 {
        self.share(if night { NIGHT_AWAKE } else { 1.0 })
    }

    fn set(&mut self, level: u32) {
        if self.level == level {
            return;
        }
        self.level = level;
        if let Some(f) = &self.file
            && let Err(e) = std::fs::write(f, level.to_string())
        {
            eprintln!("backlight: {}: {e}", f.display());
        }
    }

    /// Time passed, or the sun's side changed: asleep once `touched` long enough ago, at the level
    /// of the hour. True when it just fell asleep.
    pub fn idle(&mut self, now: Instant, touched: Instant, night: bool) -> bool {
        let fell = !self.asleep && now.saturating_duration_since(touched) >= DIM_AFTER;
        self.asleep |= fell;
        self.set(if self.asleep {
            self.dimmed(night)
        } else {
            self.lit(night)
        });
        fell
    }

    /// A finger: awake again. True when it was asleep (the touch then only wakes it).
    pub fn wake(&mut self, night: bool) -> bool {
        if !self.asleep {
            return false;
        }
        self.asleep = false;
        self.set(self.lit(night));
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

    fn level(file: &Path) -> String {
        std::fs::read_to_string(file).unwrap()
    }

    #[test]
    fn idle_long_enough_it_dims_and_a_touch_brings_it_back() {
        let (file, mut b) = panel("128\n");
        let t0 = Instant::now();
        assert!(!b.idle(t0 + Duration::from_secs(60), t0, false), "not yet");
        assert!(b.idle(t0 + DIM_AFTER, t0, false));
        assert_eq!(level(&file), "32");
        assert!(b.asleep());
        assert!(!b.idle(t0 + DIM_AFTER * 2, t0, false), "once");
        assert!(b.wake(false), "the touch only wakes it");
        assert_eq!(level(&file), "128");
        assert!(!b.wake(false), "awake: a touch acts");
    }

    #[test]
    fn at_night_nearly_nothing_never_off() {
        let (_, b) = panel("128");
        assert_eq!(b.dimmed(true), 5);
        let (_, low) = panel("10");
        assert_eq!(low.dimmed(true), 1);
    }

    #[test]
    fn asleep_it_follows_night_and_day() {
        let (file, mut b) = panel("128");
        let t0 = Instant::now();
        let asleep = t0 + DIM_AFTER;
        b.idle(asleep, t0, false);
        assert_eq!(level(&file), "32");
        assert!(!b.idle(asleep, t0, true), "still asleep");
        assert_eq!(level(&file), "5", "the sun went down");
        b.idle(asleep, t0, false);
        assert_eq!(level(&file), "32", "and came up");
    }

    #[test]
    fn a_touch_at_night_does_not_light_it_in_full() {
        let (file, mut b) = panel("128");
        let t0 = Instant::now();
        b.idle(t0 + DIM_AFTER, t0, true);
        assert!(b.wake(true));
        assert_eq!(level(&file), "51");
        // awake when the sun comes up
        b.idle(t0, t0, false);
        assert_eq!(level(&file), "128");
    }

    #[test]
    fn a_restart_while_dimmed_comes_back_to_the_panels_default() {
        let (file, b) = panel("32");
        assert_eq!(b.lit(false), 32, "no maximum said: what it found");
        std::fs::write(file.with_file_name("max_brightness"), "255\n").unwrap();
        let mut b = Backlight::open(file.parent().unwrap().parent().unwrap());
        assert_eq!(b.lit(false), 128);
        let t0 = Instant::now();
        b.idle(t0, t0, false);
        assert_eq!(level(&file), "128", "brought back at the first tick");
    }

    #[test]
    fn without_a_backlight_it_still_sleeps_and_wakes() {
        let mut b = Backlight::open(Path::new("/nonexistent"));
        let t0 = Instant::now();
        assert!(b.idle(t0 + DIM_AFTER, t0, true));
        assert!(b.wake(true));
    }
}
