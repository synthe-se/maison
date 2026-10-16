//! A slider's own state, as the web app's Range keeps it: while a finger holds it, the value is
//! the finger's; a live slider (a lamp's brightness) sends at most every `SLIDER_THROTTLE_MS` on
//! the way, any slider sends where it was let go; after that the value is held for
//! `SLIDER_HOLD_MS`, or until the device says one this near, so the thumb does not jump back to
//! the old reading while the order travels.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use maison_api::paces;

use crate::commands::{Order, Post, limit};
use crate::ui::model::Slider;

/// A live slider's orders are this far apart at least.
pub const THROTTLE: Duration = Duration::from_millis(paces::SLIDER_THROTTLE_MS);
/// How long a value let go is held against an older reading.
pub const HOLD: Duration = Duration::from_millis(paces::SLIDER_HOLD_MS);

#[derive(Debug, Clone)]
struct Held {
    value: f64,
    dragging: bool,
    /// Let go: held until then.
    until: Option<Instant>,
    sent_at: Option<Instant>,
    sent: Option<f64>,
    /// The slider, for a send owed once the throttle allows.
    slider: Slider,
}

#[derive(Debug, Default)]
pub struct Sliders {
    held: HashMap<String, Held>,
}

/// The value at `x` on a band from `x0` to `x1`, on the slider's steps.
pub fn value_at(s: &Slider, x: f32, x0: f32, x1: f32) -> f64 {
    let share = f64::from(((x - x0) / (x1 - x0).max(1.0)).clamp(0.0, 1.0));
    let raw = s.min + share * (s.max - s.min);
    let step = if s.step > 0.0 { s.step } else { 1.0 };
    (s.min + ((raw - s.min) / step).round() * step).clamp(s.min, s.max)
}

fn order(s: &Slider, value: f64) -> Order {
    let post = Post {
        path: s.path.clone(),
        body: Some((s.body)(value)),
    };
    Order::one(format!("{}:set", s.key), s.name.clone(), post)
        .within(limit::DEVICE)
        .then_refresh(&[s.refresh])
}

impl Sliders {
    /// What the thumb shows.
    pub fn shown(&self, s: &Slider, now: Instant) -> f64 {
        match self.held.get(&s.key) {
            Some(h) if h.dragging => h.value,
            Some(h) if h.until.is_some_and(|u| now < u) && (s.value - h.value).abs() > s.near => {
                h.value
            }
            _ => s.value,
        }
    }

    /// A finger on the slider at `value` (a touch on the band places the thumb).
    pub fn start(&mut self, s: &Slider, value: f64, now: Instant) -> Option<Order> {
        self.held.insert(
            s.key.clone(),
            Held {
                value,
                dragging: true,
                until: None,
                sent_at: None,
                sent: None,
                slider: s.clone(),
            },
        );
        self.send_if_live(&s.key, now)
    }

    pub fn moved(&mut self, s: &Slider, value: f64, now: Instant) -> Option<Order> {
        let h = self.held.get_mut(&s.key)?;
        h.value = value;
        self.send_if_live(&s.key, now)
    }

    /// Let go: the value is sent (unless just sent), then held.
    pub fn end(&mut self, s: &Slider, value: f64, now: Instant) -> Option<Order> {
        let h = self.held.get_mut(&s.key)?;
        h.value = value;
        h.dragging = false;
        h.until = Some(now + HOLD);
        if h.sent == Some(value) {
            return None;
        }
        h.sent = Some(value);
        h.sent_at = Some(now);
        Some(order(&h.slider, value))
    }

    fn send_if_live(&mut self, key: &str, now: Instant) -> Option<Order> {
        let h = self.held.get_mut(key)?;
        if !h.slider.live
            || h.sent == Some(h.value)
            || h.sent_at.is_some_and(|t| now < t + THROTTLE)
        {
            return None;
        }
        h.sent = Some(h.value);
        h.sent_at = Some(now);
        Some(order(&h.slider, h.value))
    }

    /// The sends the throttle held back, now due.
    pub fn due(&mut self, now: Instant) -> Vec<Order> {
        let keys: Vec<String> = self
            .held
            .iter()
            .filter(|(_, h)| h.dragging && h.slider.live)
            .map(|(k, _)| k.clone())
            .collect();
        keys.iter()
            .filter_map(|k| self.send_if_live(k, now))
            .collect()
    }

    /// When the look changes by itself: a held send due, a hold ending.
    pub fn next_change(&self, now: Instant) -> Option<Instant> {
        self.held
            .values()
            .filter_map(|h| {
                if h.dragging && h.slider.live && h.sent != Some(h.value) {
                    Some(h.sent_at.map_or(now, |t| t + THROTTLE))
                } else {
                    h.until.filter(|u| *u > now)
                }
            })
            .min()
    }

    /// Holds that ended are forgotten.
    pub fn sweep(&mut self, now: Instant) {
        self.held
            .retain(|_, h| h.dragging || h.until.is_some_and(|u| u > now));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn brightness(v: f64) -> serde_json::Value {
        crate::commands::json(&maison_api::lamps::BrightnessRequest {
            brightness: v as u8,
        })
    }

    fn lamp(value: f64) -> Slider {
        Slider {
            key: "hue-lamp:1:brightness".into(),
            value,
            min: 1.0,
            max: 100.0,
            step: 1.0,
            mark: None,
            live: true,
            near: 2.0,
            ends: None,
            path: "/hue-lamps/1/brightness".into(),
            body: brightness,
            refresh: "hue-lamp",
            name: "Salon".into(),
        }
    }

    #[test]
    fn a_value_from_where_the_finger_is() {
        let s = lamp(50.0);
        assert_eq!(value_at(&s, 0.0, 0.0, 100.0), 1.0);
        assert_eq!(value_at(&s, 50.0, 0.0, 100.0), 51.0);
        assert_eq!(value_at(&s, 500.0, 0.0, 100.0), 100.0);
        let shutter = Slider {
            min: 0.0,
            step: 5.0,
            ..lamp(0.0)
        };
        assert_eq!(value_at(&shutter, 43.0, 0.0, 100.0), 45.0);
    }

    #[test]
    fn a_live_slider_sends_on_the_way_at_most_every_300_ms() {
        let t0 = Instant::now();
        let s = lamp(50.0);
        let mut sl = Sliders::default();
        let first = sl.start(&s, 60.0, t0).unwrap();
        assert_eq!(first.posts[0].1.path, "/hue-lamps/1/brightness");
        assert_eq!(first.posts[0].1.body, Some(json!({"brightness": 60})));
        assert_eq!(first.key, "hue-lamp:1:brightness:set");
        assert!(
            sl.moved(&s, 65.0, t0 + Duration::from_millis(100))
                .is_none()
        );
        assert_eq!(sl.shown(&s, t0), 65.0);
        assert_eq!(
            sl.next_change(t0 + Duration::from_millis(100)),
            Some(t0 + THROTTLE)
        );
        let owed = sl.due(t0 + THROTTLE);
        assert_eq!(owed.len(), 1);
        assert_eq!(owed[0].posts[0].1.body, Some(json!({"brightness": 65})));
        // let go where it was last sent: nothing more
        assert!(sl.end(&s, 65.0, t0 + Duration::from_millis(400)).is_none());
    }

    #[test]
    fn let_go_the_value_holds_until_the_device_says_it_or_three_seconds() {
        let t0 = Instant::now();
        let mut sl = Sliders::default();
        let shutter = Slider {
            live: false,
            min: 0.0,
            step: 5.0,
            near: 0.0,
            ..lamp(20.0)
        };
        assert!(
            sl.start(&shutter, 40.0, t0).is_none(),
            "not live: nothing on the way"
        );
        let o = sl.end(&shutter, 80.0, t0).unwrap();
        assert_eq!(o.posts[0].1.body, Some(json!({"brightness": 80})));
        // the old reading does not pull the thumb back
        assert_eq!(sl.shown(&shutter, t0 + Duration::from_secs(1)), 80.0);
        // the device says it: its reading again
        assert_eq!(
            sl.shown(
                &Slider {
                    value: 80.0,
                    ..shutter.clone()
                },
                t0 + Duration::from_secs(1)
            ),
            80.0
        );
        // three seconds on, whatever it says
        assert_eq!(sl.shown(&shutter, t0 + HOLD), 20.0);
        sl.sweep(t0 + HOLD);
        assert!(sl.held.is_empty());
    }
}
