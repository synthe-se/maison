//! The sun over the house: where the house is (found once by name through Open-Meteo's
//! geocoding, the same provider as Tempo's weather), and when the sun rises and sets there,
//! computed locally (NOAA solar equations, `sunrise` crate). The schedule never needs the
//! network at the moment it acts: a shutter still opens at dawn when the internet is down.

use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;
use sunrise::{Coordinates, SolarDay, SolarEvent};

use crate::error::AppError;

const GEOCODING_API: &str = "https://geocoding-api.open-meteo.com/v1/search";

/// A named place, and the side the sun says: the API crate's, shared with the frame.
pub use maison_api::sun::{Place, Side};

/// Sunrise and sunset on a local calendar day; `None` in a polar day or night.
pub fn sun_times(place: &Place, day: NaiveDate) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
    let Some(coordinates) = Coordinates::new(place.latitude, place.longitude) else {
        return (None, None);
    };
    let solar = SolarDay::new(coordinates, day);
    (
        solar.event_time(SolarEvent::Sunrise),
        solar.event_time(SolarEvent::Sunset),
    )
}

/// The side now, from the next sunrise and sunset: dark when the sun rises next, light when it
/// sets next; a polar day or night has only one of the two ahead. `None` when neither is.
pub fn side(
    next_sunrise: Option<DateTime<Utc>>,
    next_sunset: Option<DateTime<Utc>>,
) -> Option<Side> {
    match (next_sunrise, next_sunset) {
        (Some(rise), Some(set)) => Some(if rise < set { Side::Dark } else { Side::Light }),
        (None, Some(_)) => Some(Side::Light),
        (Some(_), None) => Some(Side::Dark),
        (None, None) => None,
    }
}

#[derive(Debug, Deserialize)]
struct GeocodingResponse {
    #[serde(default)]
    results: Vec<GeocodingResult>,
}

#[derive(Debug, Deserialize)]
struct GeocodingResult {
    name: String,
    latitude: f64,
    longitude: f64,
    admin1: Option<String>,
    country: Option<String>,
}

/// Places matching a name (« Lyon » → Lyon, Auvergne-Rhône-Alpes, France), best first.
pub async fn search(
    client: &reqwest::Client,
    query: &str,
    language: &str,
) -> Result<Vec<Place>, AppError> {
    let query = query.trim();
    if query.chars().count() < 2 {
        return Ok(Vec::new());
    }
    let response: GeocodingResponse = client
        .get(GEOCODING_API)
        .query(&[
            ("name", query),
            ("count", "6"),
            ("language", language),
            ("format", "json"),
        ])
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(response.results.into_iter().map(place_of).collect())
}

fn place_of(r: GeocodingResult) -> Place {
    let name = [Some(r.name), r.admin1, r.country]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ");
    Place {
        name,
        latitude: r.latitude,
        longitude: r.longitude,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    #[test]
    fn the_sun_says_dark_when_it_rises_next_and_light_when_it_sets_next() {
        let at = |h: u32| DateTime::<Utc>::from_timestamp(i64::from(h) * 3600, 0);
        assert_eq!(side(at(5), at(17)), Some(Side::Dark));
        assert_eq!(side(at(29), at(17)), Some(Side::Light));
        assert_eq!(side(None, at(2)), Some(Side::Light), "a polar day");
        assert_eq!(side(at(2), None), Some(Side::Dark), "a polar night");
        assert_eq!(side(None, None), None);
    }

    fn paris() -> Place {
        Place {
            name: "Paris".into(),
            latitude: 48.8566,
            longitude: 2.3522,
        }
    }

    #[test]
    fn paris_on_the_summer_solstice() {
        // timeanddate.com: sunrise 05:47, sunset 21:58 CEST (03:47 / 19:58 UTC), 21 June 2026
        let (rise, set) = sun_times(&paris(), NaiveDate::from_ymd_opt(2026, 6, 21).unwrap());
        let (rise, set) = (rise.unwrap(), set.unwrap());
        assert_eq!((rise.hour(), rise.minute() / 10), (3, 4), "{rise}");
        assert_eq!((set.hour(), set.minute() / 10), (19, 5), "{set}");
    }

    #[test]
    fn paris_on_the_winter_solstice() {
        // sunrise 08:42, sunset 16:56 CET (07:42 / 15:56 UTC), 21 December 2026
        let (rise, set) = sun_times(&paris(), NaiveDate::from_ymd_opt(2026, 12, 21).unwrap());
        let (rise, set) = (rise.unwrap(), set.unwrap());
        assert_eq!((rise.hour(), rise.minute() / 10), (7, 4), "{rise}");
        assert_eq!((set.hour(), set.minute() / 10), (15, 5), "{set}");
    }

    #[test]
    fn no_sunset_in_a_polar_day() {
        let tromso = Place {
            name: "Tromsø".into(),
            latitude: 69.65,
            longitude: 18.96,
        };
        let (rise, set) = sun_times(&tromso, NaiveDate::from_ymd_opt(2026, 6, 21).unwrap());
        assert!(rise.is_none() && set.is_none());
    }

    #[test]
    fn invalid_coordinates_give_no_times() {
        let nowhere = Place {
            name: "x".into(),
            latitude: 123.0,
            longitude: 0.0,
        };
        assert_eq!(
            sun_times(&nowhere, NaiveDate::from_ymd_opt(2026, 6, 21).unwrap()),
            (None, None)
        );
    }

    #[test]
    fn a_place_is_named_with_its_region_and_country() {
        let r = GeocodingResult {
            name: "Lyon".into(),
            latitude: 45.75,
            longitude: 4.85,
            admin1: Some("Auvergne-Rhône-Alpes".into()),
            country: Some("France".into()),
        };
        assert_eq!(place_of(r).name, "Lyon, Auvergne-Rhône-Alpes, France");
    }
}
