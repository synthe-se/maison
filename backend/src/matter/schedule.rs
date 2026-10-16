//! The shutters' sun schedule, as pure functions of a place and a time: which order falls
//! due between two looks (the sun's times from `sun.rs`), what is retried, what a « not
//! tonight » skips. `MatterManager` looks every 30 s and sends what this says.

use std::collections::BTreeMap;

use chrono::{DateTime, TimeDelta, Utc};

use super::{CoverCommand, CoverConfig};
use crate::{
    sun::{self, Place},
    util::HOUSE_TZ,
};

pub use maison_api::matter::SunSchedule;

pub(super) const MAX_SUN_OFFSET_MIN: i16 = 180;

pub(super) fn offset(minutes: i16) -> TimeDelta {
    TimeDelta::minutes(i64::from(minutes))
}

/// Two times of one event (an opening, a closing) closer than this are the same occurrence:
/// the next one is a day away. A skip names its occurrence by time; this much slack lets it
/// survive a moved place or offset (a few minutes, three hours at most).
const SAME_EVENT: TimeDelta = TimeDelta::hours(12);

/// Whether `at` is the occurrence a skip set for `skipped` names.
pub(super) fn same_event(skipped: DateTime<Utc>, at: DateTime<Utc>) -> bool {
    (at - skipped).abs() < SAME_EVENT
}

/// A skip whose occurrence is over at `now` (passed while the Pi was down or its clock not
/// trusted): it would otherwise skip nothing, forever.
pub(super) fn skip_over(skipped: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    now - skipped >= SAME_EVENT
}

/// How far back the scheduler looks, whatever its last look: the Pi boots on the time it
/// last saw (swclock, no RTC) and NTP then steps the clock weeks ahead, which must not
/// replay every sunrise and sunset in between.
const CATCH_UP: TimeDelta = TimeDelta::minutes(5);

/// The command the schedule sends now: the latest whose time fell in (since, now], looking
/// back `CATCH_UP` at most (the shutter only needs its last order).
pub(super) fn due(
    schedule: &SunSchedule,
    place: &Place,
    since: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Option<(CoverCommand, DateTime<Utc>)> {
    let since = since.max(now - CATCH_UP);
    let mut out = Vec::new();
    // yesterday too: a late sunset plus an offset can fall after local midnight (the first
    // day there is has no yesterday)
    let since_day = since.with_timezone(&HOUSE_TZ).date_naive();
    let first = since_day.pred_opt().unwrap_or(since_day);
    let last = now.with_timezone(&HOUSE_TZ).date_naive();
    for day in first.iter_days().take_while(|d| *d <= last) {
        let (rise, set) = sun::sun_times(place, day);
        let events = [
            (
                schedule.open_at_sunrise,
                rise,
                schedule.sunrise_offset_min,
                CoverCommand::Open,
            ),
            (
                schedule.close_at_sunset,
                set,
                schedule.sunset_offset_min,
                CoverCommand::Close,
            ),
        ];
        for (enabled, time, minutes, command) in events {
            if let (true, Some(time)) = (enabled, time) {
                let at = time + offset(minutes);
                if at > since && at <= now {
                    out.push((command, at));
                }
            }
        }
    }
    out.into_iter().max_by_key(|(_, at)| *at)
}

/// How long a schedule order is retried (Wi-Fi off at sunset: the shutters still close when
/// it comes back, but not hours later).
pub(super) const RETRY_FOR: TimeDelta = TimeDelta::minutes(30);

/// A schedule order not sent yet: the event and when it fell due.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Order {
    pub(super) command: CoverCommand,
    pub(super) at: DateTime<Utc>,
}

/// What one look of the scheduler found: the orders to send, the events skipped.
pub(super) struct Look {
    pub(super) orders: Vec<(CoverConfig, Order)>,
    pub(super) skipped: Vec<(u64, CoverCommand)>,
}

/// The scheduler's memory: its last look (an event between it and now is due; none while the
/// clock is not trusted) and, per cover, the one order still to send.
#[derive(Debug, Default)]
pub(super) struct ScheduleState {
    last_tick: Option<DateTime<Utc>>,
    pub(super) pending: BTreeMap<u64, Order>,
}

impl ScheduleState {
    /// The orders to send at `now`, and the events skipped (their skip to clear). An
    /// untrusted clock (swclock's days-old time before NTP) sends nothing; the first trusted
    /// look starts from now, replaying nothing. A newer event replaces a cover's pending
    /// order (a skipped one leaves none); one older than `RETRY_FOR` is dropped.
    pub(super) fn tick(
        &mut self,
        now: DateTime<Utc>,
        trusted: bool,
        place: Option<&Place>,
        covers: &[CoverConfig],
    ) -> Look {
        let mut skipped = Vec::new();
        if !trusted {
            self.last_tick = None;
            return Look {
                orders: Vec::new(),
                skipped,
            };
        }
        let since = self.last_tick.replace(now).unwrap_or(now);
        if let Some(place) = place {
            for cover in covers {
                match due(&cover.schedule, place, since, now) {
                    Some((command, at)) if cover.skips(command, at) => {
                        self.pending.remove(&cover.node_id);
                        skipped.push((cover.node_id, command));
                    }
                    Some((command, at)) => {
                        self.pending.insert(cover.node_id, Order { command, at });
                    }
                    None => {}
                }
            }
        }
        self.pending.retain(|node_id, order| {
            let keep = now - order.at <= RETRY_FOR && covers.iter().any(|c| c.node_id == *node_id);
            if !keep {
                tracing::warn!(
                    node_id = format_args!("{node_id:016x}"),
                    ?order,
                    "sun schedule: order given up"
                );
            }
            keep
        });
        let orders = covers
            .iter()
            .filter_map(|cover| {
                self.pending
                    .get(&cover.node_id)
                    .map(|order| (cover.clone(), *order))
            })
            .collect();
        Look { orders, skipped }
    }

    /// `order` went through: done, unless a newer one replaced it meanwhile.
    pub(super) fn sent(&mut self, node_id: u64, order: Order) {
        if self.pending.get(&node_id) == Some(&order) {
            self.pending.remove(&node_id);
        }
    }
}

/// Whether the system clock can be acted on. Linux: the kernel's NTP state (`adjtimex`
/// answers `TIME_ERROR` while `STA_UNSYNC` is set, i.e. until chrony has synchronised it;
/// the Pi has no RTC and boots on swclock's saved time). Elsewhere (a dev Mac): trusted.
#[cfg(target_os = "linux")]
pub(super) fn clock_trusted() -> bool {
    // Only the return value is read: `modes` (the first field) is 0, so the kernel changes
    // nothing, and the buffer is larger than any libc's `struct timex`.
    #[repr(C, align(8))]
    struct Timex([u8; 512]);
    extern "C" {
        fn adjtimex(buf: *mut Timex) -> std::ffi::c_int;
    }
    const TIME_ERROR: std::ffi::c_int = 5;
    let mut buf = Timex([0; 512]);
    // SAFETY: a zeroed, writable buffer larger than `struct timex`, read-only mode.
    let state = unsafe { adjtimex(&mut buf) };
    // -1 (no permission to ask): nothing better to go on than the clock itself
    state != TIME_ERROR
}

#[cfg(not(target_os = "linux"))]
pub(super) fn clock_trusted() -> bool {
    true
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    pub(in crate::matter) use crate::sun::tests::paris;

    pub(in crate::matter) fn utc(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    pub(in crate::matter) const BOTH: SunSchedule = SunSchedule {
        open_at_sunrise: true,
        close_at_sunset: true,
        sunrise_offset_min: 0,
        sunset_offset_min: 0,
    };

    #[test]
    fn a_sunrise_between_two_looks_opens_once() {
        let day = chrono::NaiveDate::from_ymd_opt(2026, 10, 2).unwrap();
        let rise = sun::sun_times(&paris(), day).0.unwrap();
        let look = TimeDelta::seconds(30);
        let fired = due(&BOTH, &paris(), rise - look, rise + look);
        assert_eq!(fired.map(|e| e.0), Some(CoverCommand::Open));
        // the next look does not repeat it
        assert!(due(&BOTH, &paris(), rise + look, rise + look * 2).is_none());
    }

    #[test]
    fn a_sunset_closes_and_its_offset_shifts_it() {
        let day = chrono::NaiveDate::from_ymd_opt(2026, 10, 2).unwrap();
        let set = sun::sun_times(&paris(), day).1.unwrap();
        let later = SunSchedule {
            sunset_offset_min: 30,
            ..BOTH
        };
        let look = TimeDelta::seconds(30);
        assert_eq!(
            due(&BOTH, &paris(), set - look, set + look).map(|e| e.0),
            Some(CoverCommand::Close)
        );
        assert!(
            due(&later, &paris(), set - look, set + look).is_none(),
            "30 min later: not yet"
        );
        let half_hour_on = set + offset(30);
        assert_eq!(
            due(&later, &paris(), half_hour_on - look, half_hour_on + look).map(|e| e.0),
            Some(CoverCommand::Close)
        );
    }

    #[test]
    fn a_disabled_event_never_fires() {
        let open_only = SunSchedule {
            close_at_sunset: false,
            ..BOTH
        };
        assert!(due(
            &open_only,
            &paris(),
            utc("2026-10-02T17:00:00Z"),
            utc("2026-10-02T18:00:00Z")
        )
        .is_none());
        assert!(due(
            &SunSchedule::default(),
            &paris(),
            utc("2026-10-02T00:00:00Z"),
            utc("2026-10-03T00:00:00Z")
        )
        .is_none());
    }

    #[test]
    fn a_clock_stepped_weeks_ahead_replays_nothing() {
        // the Pi booted on 14 August (swclock), then NTP jumped to 2 October at 21:25 UTC
        let since = utc("2026-08-14T20:43:37Z");
        assert!(due(&BOTH, &paris(), since, utc("2026-10-02T21:25:44Z")).is_none());
        // an event a minute before the jump is still kept, alone
        let fired = due(&BOTH, &paris(), since, utc("2026-10-02T17:29:00Z"));
        assert_eq!(fired.map(|e| e.0), Some(CoverCommand::Close));
    }

    #[test]
    fn a_skip_names_one_occurrence() {
        let set = utc("2026-10-02T17:20:00Z");
        assert!(
            same_event(set, set + TimeDelta::minutes(3)),
            "a moved place"
        );
        assert!(same_event(set, set - TimeDelta::hours(3)), "a moved offset");
        assert!(
            !same_event(set, set + TimeDelta::hours(24)),
            "the next day's is another"
        );
        assert!(!skip_over(set, set + TimeDelta::hours(1)));
        assert!(skip_over(set, set + TimeDelta::hours(12)));
    }
}
