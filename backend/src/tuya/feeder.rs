//! The feeder's meal plan: what the web edits, and the bytes the feeder keeps in its
//! `MEAL_PLAN` data point (base64 of 5 bytes per meal: days, hour, minute, portions,
//! enabled).

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};

use super::parse::parse_hhmm;
use crate::{error::AppError, net};

/// The most portions one meal may serve.
pub const MAX_PORTIONS: u8 = 10;
/// The most meals a plan holds.
pub const MAX_MEALS: usize = 10;

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

/// The plan as the feeder keeps it, every meal checked (400 naming the first bad one).
pub fn encode(entries: &[MealPlanEntry]) -> Result<String, AppError> {
    if entries.is_empty() {
        return Err(AppError::bad_request("mealPlan array is required"));
    }
    if entries.len() > MAX_MEALS {
        return Err(AppError::bad_request(format!("mealPlan supports at most {MAX_MEALS} entries")));
    }
    let mut encoded = Vec::with_capacity(entries.len() * 5);
    for (index, entry) in entries.iter().enumerate() {
        let invalid = || AppError::bad_request(format!("Invalid meal plan entry at index {index}"));
        let (hours, minutes) = parse_hhmm(&entry.time).map_err(|_| invalid())?;
        if entry.days_of_week.is_empty() || !(1..=MAX_PORTIONS).contains(&entry.portion) {
            return Err(invalid());
        }
        let days = entry.days_of_week.iter().fold(0_u8, |bits, day| bits | day.bit());
        let enabled = u8::from(entry.status == MealStatus::Enabled);
        encoded.extend_from_slice(&[days, hours, minutes, entry.portion, enabled]);
    }
    Ok(STANDARD.encode(encoded))
}

/// The feeder's plan read back; a trailing partial meal is ignored.
pub fn decode(encoded: &str) -> Result<Vec<MealPlanEntry>, AppError> {
    let bytes = STANDARD.decode(encoded).map_err(|error| net::unreachable("Feeder", error))?;
    let (meals, _) = bytes.as_chunks::<5>();
    Ok(meals
        .iter()
        .map(|meal| MealPlanEntry {
            days_of_week: Weekday::ALL.into_iter().filter(|day| meal[0] & day.bit() != 0).collect(),
            time: format!("{:02}:{:02}", meal[1], meal[2]),
            portion: meal[3],
            status: if meal[4] == 1 { MealStatus::Enabled } else { MealStatus::Disabled },
        })
        .collect())
}

/// One line per meal, for the update's answer.
pub fn describe(entries: &[MealPlanEntry]) -> String {
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let days = entry.days_of_week.iter().map(|day| format!("{day:?}")).collect::<Vec<_>>().join(", ");
            format!("{}. {days} a {} - {} serving(s) - {:?}", index + 1, entry.time, entry.portion, entry.status)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn meal(days: Vec<Weekday>, time: &str, portion: u8, status: MealStatus) -> MealPlanEntry {
        MealPlanEntry { days_of_week: days, time: time.into(), portion, status }
    }

    #[test]
    fn the_json_is_camel_case_with_the_legacy_day_and_status_values() {
        let entry = meal(vec![Weekday::Monday, Weekday::Wednesday], "08:30", 2, MealStatus::Enabled);
        let value = json!({ "daysOfWeek": ["Monday", "Wednesday"], "time": "08:30", "portion": 2, "status": "Enabled" });
        assert_eq!(serde_json::to_value(&entry).unwrap(), value);
        assert_eq!(serde_json::from_value::<MealPlanEntry>(value).unwrap(), entry);
        assert!(serde_json::from_value::<MealPlanEntry>(json!({ "daysOfWeek": ["Funday"], "time": "08:30", "portion": 2, "status": "Enabled" })).is_err());
    }

    #[test]
    fn encoding_matches_the_legacy_bytes() {
        let entry = meal(vec![Weekday::Monday, Weekday::Wednesday], "08:30", 2, MealStatus::Enabled);
        assert_eq!(encode(std::slice::from_ref(&entry)).unwrap(), "BQgeAgE=");
        assert_eq!(decode("BQgeAgE=").unwrap(), vec![entry]);
    }

    #[test]
    fn a_plan_round_trips() {
        let plan = vec![
            meal(vec![Weekday::Monday, Weekday::Friday], "07:15", 1, MealStatus::Enabled),
            meal(vec![Weekday::Sunday], "18:45", 3, MealStatus::Disabled),
            meal(Weekday::ALL.to_vec(), "23:59", MAX_PORTIONS, MealStatus::Enabled),
        ];
        assert_eq!(decode(&encode(&plan).unwrap()).unwrap(), plan);
    }

    #[test]
    fn bad_meals_are_named() {
        let good = meal(vec![Weekday::Monday], "08:30", 2, MealStatus::Enabled);
        let error = |plan: &[MealPlanEntry]| encode(plan).unwrap_err().to_string();
        assert_eq!(error(&[]), "mealPlan array is required");
        assert_eq!(error(&vec![good.clone(); 11]), "mealPlan supports at most 10 entries");
        let cases = [
            meal(vec![], "08:30", 2, MealStatus::Enabled),
            meal(vec![Weekday::Monday], "24:00", 2, MealStatus::Enabled),
            meal(vec![Weekday::Monday], "8h", 2, MealStatus::Enabled),
            meal(vec![Weekday::Monday], "08:30", 0, MealStatus::Enabled),
            meal(vec![Weekday::Monday], "08:30", 11, MealStatus::Enabled),
        ];
        for bad in cases {
            assert_eq!(error(&[good.clone(), bad.clone()]), "Invalid meal plan entry at index 1", "{bad:?}");
        }
    }

    #[test]
    fn a_feeder_answering_nonsense_says_only_that() {
        assert_eq!(decode("***").unwrap_err().to_string(), "Feeder unreachable");
        assert_eq!(decode("BQgeAg==").unwrap(), Vec::new(), "a partial meal is ignored");
    }

    #[test]
    fn the_plan_is_described_line_by_line() {
        let plan = [
            meal(vec![Weekday::Monday, Weekday::Friday], "07:15", 1, MealStatus::Enabled),
            meal(vec![Weekday::Sunday], "18:45", 3, MealStatus::Disabled),
        ];
        assert_eq!(
            describe(&plan),
            "1. Monday, Friday a 07:15 - 1 serving(s) - Enabled\n2. Sunday a 18:45 - 3 serving(s) - Disabled"
        );
    }
}
