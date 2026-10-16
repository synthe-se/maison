//! The feeder's meal plan: what the web edits, and the bytes the feeder keeps in its
//! `MEAL_PLAN` data point (base64 of 5 bytes per meal: days, hour, minute, portions,
//! enabled). And the limits of feeding on demand (`Feeds`).

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::{DateTime, NaiveDate};
use maison_api::Tz;
use serde::{Deserialize, Serialize};

use crate::{
    error::{AppError, Refusal},
    net,
    util::{refused, HOUSE_TZ},
};

/// The most portions one meal may serve.
pub const MAX_PORTIONS: u8 = 10;
/// The most portions one manual feed may serve.
pub const MAX_FEED_PORTIONS: u8 = 12;
/// The most meals a plan holds.
pub const MAX_MEALS: usize = 10;

/// One manual feed per feeder within this (a double tap, a client in a loop).
pub const FEED_COOLDOWN: Duration = Duration::from_secs(5);
/// The most portions a feeder serves on demand in a day of the house's (the meal plan's
/// are the feeder's own, never counted): past it, a client in a loop cannot empty the
/// tank. Counted in memory: a restart forgets the day's count.
pub const DAILY_PORTIONS: u32 = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl Weekday {
    /// In the order of the days byte's bits (bit 0: Monday).
    const ALL: [Weekday; 7] = [
        Weekday::Monday,
        Weekday::Tuesday,
        Weekday::Wednesday,
        Weekday::Thursday,
        Weekday::Friday,
        Weekday::Saturday,
        Weekday::Sunday,
    ];

    fn bit(self) -> u8 {
        1 << self as u8
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MealStatus {
    Enabled,
    Disabled,
}

/// One meal, as the web sends and reads it (`{daysOfWeek, time: "08:30", portion, status}`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MealPlanEntry {
    pub days_of_week: Vec<Weekday>,
    pub time: String,
    pub portion: u8,
    pub status: MealStatus,
}

/// The plan as the feeder keeps it, every meal checked (`invalid`, the first bad one named
/// in the log).
pub fn encode(entries: &[MealPlanEntry]) -> Result<String, AppError> {
    if entries.is_empty() {
        return Err(refused(
            Refusal::Invalid,
            "a meal plan holds a meal at least",
        ));
    }
    if entries.len() > MAX_MEALS {
        return Err(refused(
            Refusal::Invalid,
            format!("a meal plan holds {MAX_MEALS} meals at most"),
        ));
    }
    let mut encoded = Vec::with_capacity(entries.len() * 5);
    for (index, entry) in entries.iter().enumerate() {
        let invalid = || refused(Refusal::Invalid, format!("meal {index}: {entry:?}"));
        let minutes = crate::util::minutes_of_day(&entry.time).ok_or_else(invalid)?;
        if entry.days_of_week.is_empty() || !(1..=MAX_PORTIONS).contains(&entry.portion) {
            return Err(invalid());
        }
        let days = entry
            .days_of_week
            .iter()
            .fold(0_u8, |bits, day| bits | day.bit());
        let enabled = u8::from(entry.status == MealStatus::Enabled);
        // below 24 × 60: both fit a byte
        let (hours, minutes) = ((minutes / 60) as u8, (minutes % 60) as u8);
        encoded.extend_from_slice(&[days, hours, minutes, entry.portion, enabled]);
    }
    Ok(STANDARD.encode(encoded))
}

/// The feeder's plan read back; a trailing partial meal is ignored.
pub fn decode(encoded: &str) -> Result<Vec<MealPlanEntry>, AppError> {
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|error| net::unreachable("Feeder", error))?;
    let (meals, _) = bytes.as_chunks::<5>();
    Ok(meals
        .iter()
        .map(|meal| MealPlanEntry {
            days_of_week: Weekday::ALL
                .into_iter()
                .filter(|day| meal[0] & day.bit() != 0)
                .collect(),
            time: format!("{:02}:{:02}", meal[1], meal[2]),
            portion: meal[3],
            status: if meal[4] == 1 {
                MealStatus::Enabled
            } else {
                MealStatus::Disabled
            },
        })
        .collect())
}

/// One line per meal, for the update's answer.
pub fn describe(entries: &[MealPlanEntry]) -> String {
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let days = entry
                .days_of_week
                .iter()
                .map(|day| format!("{day:?}"))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "{}. {days} a {} - {} serving(s) - {:?}",
                index + 1,
                entry.time,
                entry.portion,
                entry.status
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The manual feeds of each feeder: one per `FEED_COOLDOWN`, `DAILY_PORTIONS` a day.
#[derive(Debug, Default)]
pub struct Feeds(std::sync::Mutex<HashMap<String, FeedLog>>);

#[derive(Debug, Clone, Copy)]
struct FeedLog {
    last: Instant,
    day: NaiveDate,
    portions: u32,
}

/// A feed counted before it is sent: given back if the feeder did not take it.
#[derive(Debug)]
pub struct Claim {
    device: String,
    before: Option<FeedLog>,
}

impl Feeds {
    /// Counts `portions` for `device`, or says when to try again (`too_many_attempts`).
    pub fn claim(&self, device: &str, portions: u8) -> Result<Claim, AppError> {
        self.claim_at(
            device,
            portions,
            Instant::now(),
            chrono::Utc::now().with_timezone(&HOUSE_TZ),
        )
    }

    fn claim_at(
        &self,
        device: &str,
        portions: u8,
        now: Instant,
        clock: DateTime<Tz>,
    ) -> Result<Claim, AppError> {
        let mut feeds = crate::util::lock(&self.0);
        let before = feeds.get(device).copied();
        let today = clock.date_naive();
        let mut log = before.map_or(
            FeedLog {
                last: now,
                day: today,
                portions: 0,
            },
            |log| FeedLog {
                portions: if log.day == today { log.portions } else { 0 },
                day: today,
                ..log
            },
        );
        if let Some(earlier) = before {
            let since = now.saturating_duration_since(earlier.last);
            if since < FEED_COOLDOWN {
                let wait = (FEED_COOLDOWN - since).as_secs_f64().ceil() as u64;
                return Err(AppError::too_many(wait.max(1)));
            }
        }
        if log.portions + u32::from(portions) > DAILY_PORTIONS {
            tracing::warn!(
                device,
                portions = log.portions,
                "feeder: the day's portions are served"
            );
            return Err(AppError::too_many(seconds_to_midnight(clock)));
        }
        log.last = now;
        log.portions += u32::from(portions);
        feeds.insert(device.to_string(), log);
        Ok(Claim {
            device: device.to_string(),
            before,
        })
    }

    /// The feeder did not take the order: as if it was never asked.
    pub fn give_back(&self, claim: Claim) {
        let mut feeds = crate::util::lock(&self.0);
        match claim.before {
            Some(before) => feeds.insert(claim.device, before),
            None => feeds.remove(&claim.device),
        };
    }
}

/// Seconds from `clock` to the house's next midnight (1 at least).
fn seconds_to_midnight(clock: DateTime<Tz>) -> u64 {
    let next = clock
        .date_naive()
        .succ_opt()
        .and_then(|day| day.and_hms_opt(0, 0, 0))
        .and_then(|midnight| midnight.and_local_timezone(HOUSE_TZ).earliest());
    next.map_or(3600, |next| {
        u64::try_from((next - clock).num_seconds())
            .unwrap_or(1)
            .max(1)
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn meal(days: Vec<Weekday>, time: &str, portion: u8, status: MealStatus) -> MealPlanEntry {
        MealPlanEntry {
            days_of_week: days,
            time: time.into(),
            portion,
            status,
        }
    }

    #[test]
    fn the_json_is_camel_case_with_the_legacy_day_and_status_values() {
        let entry = meal(
            vec![Weekday::Monday, Weekday::Wednesday],
            "08:30",
            2,
            MealStatus::Enabled,
        );
        let value = json!({ "daysOfWeek": ["Monday", "Wednesday"], "time": "08:30", "portion": 2, "status": "Enabled" });
        assert_eq!(serde_json::to_value(&entry).unwrap(), value);
        assert_eq!(
            serde_json::from_value::<MealPlanEntry>(value).unwrap(),
            entry
        );
        assert!(serde_json::from_value::<MealPlanEntry>(
            json!({ "daysOfWeek": ["Funday"], "time": "08:30", "portion": 2, "status": "Enabled" })
        )
        .is_err());
    }

    #[test]
    fn encoding_matches_the_legacy_bytes() {
        let entry = meal(
            vec![Weekday::Monday, Weekday::Wednesday],
            "08:30",
            2,
            MealStatus::Enabled,
        );
        assert_eq!(encode(std::slice::from_ref(&entry)).unwrap(), "BQgeAgE=");
        assert_eq!(decode("BQgeAgE=").unwrap(), vec![entry]);
    }

    #[test]
    fn a_plan_round_trips() {
        let plan = vec![
            meal(
                vec![Weekday::Monday, Weekday::Friday],
                "07:15",
                1,
                MealStatus::Enabled,
            ),
            meal(vec![Weekday::Sunday], "18:45", 3, MealStatus::Disabled),
            meal(
                Weekday::ALL.to_vec(),
                "23:59",
                MAX_PORTIONS,
                MealStatus::Enabled,
            ),
        ];
        assert_eq!(decode(&encode(&plan).unwrap()).unwrap(), plan);
    }

    #[test]
    fn bad_meals_are_refused() {
        let good = meal(vec![Weekday::Monday], "08:30", 2, MealStatus::Enabled);
        let refused = |plan: &[MealPlanEntry]| {
            matches!(
                encode(plan),
                Err(AppError::Coded {
                    code: "invalid",
                    ..
                })
            )
        };
        assert!(refused(&[]));
        assert!(refused(&vec![good.clone(); MAX_MEALS + 1]));
        assert!(encode(&vec![good.clone(); MAX_MEALS]).is_ok());
        let cases = [
            meal(vec![], "08:30", 2, MealStatus::Enabled),
            meal(vec![Weekday::Monday], "24:00", 2, MealStatus::Enabled),
            meal(vec![Weekday::Monday], "8h", 2, MealStatus::Enabled),
            meal(vec![Weekday::Monday], "08:30", 0, MealStatus::Enabled),
            meal(vec![Weekday::Monday], "08:30", 11, MealStatus::Enabled),
        ];
        for bad in cases {
            assert!(refused(&[good.clone(), bad.clone()]), "{bad:?}");
        }
    }

    fn at(day: u32, hour: u32) -> DateTime<Tz> {
        use chrono::TimeZone;
        HOUSE_TZ.with_ymd_and_hms(2026, 3, day, hour, 0, 0).unwrap()
    }

    fn retry_after(error: AppError) -> Option<u64> {
        match error {
            AppError::Coded {
                code: "too_many_attempts",
                retry_after_s,
                ..
            } => retry_after_s,
            _ => None,
        }
    }

    /// A feeder feeds on demand once per cooldown, and up to the day's portions.
    #[test]
    fn feeds_on_demand_have_a_cooldown_and_a_daily_cap() {
        let feeds = Feeds::default();
        let start = Instant::now();
        let after = |s: u64| start + Duration::from_secs(s);
        feeds.claim_at("f", 12, start, at(10, 8)).unwrap();
        let soon = feeds.claim_at("f", 1, after(2), at(10, 8)).unwrap_err();
        assert_eq!(retry_after(soon), Some(3), "the cooldown's rest");
        feeds.claim_at("other", 1, after(2), at(10, 8)).unwrap();
        let mut t = 0;
        for _ in 0..3 {
            t += FEED_COOLDOWN.as_secs();
            feeds.claim_at("f", 12, after(t), at(10, 9)).unwrap();
        }
        t += FEED_COOLDOWN.as_secs();
        let full = feeds.claim_at("f", 1, after(t), at(10, 23)).unwrap_err();
        assert_eq!(retry_after(full), Some(3600), "until the house's midnight");
        feeds.claim_at("f", 12, after(t), at(11, 0)).unwrap();
    }

    /// An order the feeder did not take counts for nothing.
    #[test]
    fn a_feed_not_sent_is_given_back() {
        let feeds = Feeds::default();
        let now = Instant::now();
        let claim = feeds.claim_at("f", 12, now, at(10, 8)).unwrap();
        feeds.give_back(claim);
        feeds.claim_at("f", 12, now, at(10, 8)).unwrap();
    }

    #[test]
    fn a_feeder_answering_nonsense_says_only_that() {
        assert_eq!(decode("***").unwrap_err().to_string(), "Feeder unreachable");
        assert_eq!(
            decode("BQgeAg==").unwrap(),
            Vec::new(),
            "a partial meal is ignored"
        );
    }

    #[test]
    fn the_plan_is_described_line_by_line() {
        let plan = [
            meal(
                vec![Weekday::Monday, Weekday::Friday],
                "07:15",
                1,
                MealStatus::Enabled,
            ),
            meal(vec![Weekday::Sunday], "18:45", 3, MealStatus::Disabled),
        ];
        assert_eq!(
            describe(&plan),
            "1. Monday, Friday a 07:15 - 1 serving(s) - Enabled\n2. Sunday a 18:45 - 3 serving(s) - Disabled"
        );
    }
}
