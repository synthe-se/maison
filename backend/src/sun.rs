//! The sun over the house: where the house is (found once by name through Open-Meteo's
//! geocoding, the same provider as Tempo's weather), and when the sun rises and sets there,
//! computed locally (NOAA solar equations, `sunrise` crate), said once: the shutters'
//! schedule, the pages' theme and the place's answer all read [`next`]. The schedule never
//! needs the network at the moment it acts: a shutter still opens at dawn when the internet
//! is down.

use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;
use sunrise::{Coordinates, SolarDay, SolarEvent};

use crate::{error::AppError, util::HOUSE_TZ};

/// A named place, the side the sun says, and when it next rises and sets: the API crate's,
/// shared with the frame.
pub use maison_api::sun::{NextSun, Place, Side};

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

/// The next sunrise and sunset at a place, strictly after a moment; either is `None` in a
/// polar day or night (or with no place: the default).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SunTimes {
    pub sunrise: Option<DateTime<Utc>>,
    pub sunset: Option<DateTime<Utc>>,
}

impl SunTimes {
    /// As the API says it: the times (ms) and the side they make now.
    pub fn api(self) -> NextSun {
        NextSun {
            sunrise_ms: self.sunrise.map(|t| t.timestamp_millis()),
            sunset_ms: self.sunset.map(|t| t.timestamp_millis()),
            side: side(self.sunrise, self.sunset),
        }
    }
}

/// The next sunrise and sunset at `place` after `now` (looking up to two days ahead:
/// today's may be past).
pub fn next(place: &Place, now: DateTime<Utc>) -> SunTimes {
    let today = now.with_timezone(&HOUSE_TZ).date_naive();
    let mut next = SunTimes::default();
    for day in today.iter_days().take(3) {
        let (rise, set) = sun_times(place, day);
        next.sunrise = next.sunrise.or(rise.filter(|t| *t > now));
        next.sunset = next.sunset.or(set.filter(|t| *t > now));
    }
    next
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
#[serde(rename_all = "camelCase")]
struct GeocodingResponse {
    #[serde(default)]
    results: Vec<GeocodingResult>,
}

/// Open-Meteo's words (`admin1`: the region).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeocodingResult {
    name: String,
    latitude: f64,
    longitude: f64,
    admin1: Option<String>,
    country: Option<String>,
}

/// Places matching a name (« Lyon » → Lyon, Auvergne-Rhône-Alpes, France), best first, from
/// the geocoding service at `url` (`Config::geocoding_url`); a service down is the coded
/// `service_unreachable`.
pub async fn search(
    client: &reqwest::Client,
    url: &str,
    query: &str,
    language: &str,
) -> Result<Vec<Place>, AppError> {
    let query = query.trim();
    if query.chars().count() < 2 {
        return Ok(Vec::new());
    }
    let request = client.get(format!("{url}/v1/search")).query(&[
        ("name", query),
        ("count", "6"),
        ("language", language),
        ("format", "json"),
    ]);
    let response: GeocodingResponse =
        crate::net::fetch_json(request, "Open-Meteo geocoding").await?;
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
pub(crate) mod tests {
    use super::*;
    use chrono::Timelike;

    pub(crate) fn paris() -> Place {
        Place {
            name: "Paris".into(),
            latitude: 48.8566,
            longitude: 2.3522,
        }
    }

    fn utc(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    #[test]
    fn the_sun_says_dark_when_it_rises_next_and_light_when_it_sets_next() {
        let at = |h: u32| DateTime::<Utc>::from_timestamp(i64::from(h) * 3600, 0);
        assert_eq!(side(at(5), at(17)), Some(Side::Dark));
        assert_eq!(side(at(29), at(17)), Some(Side::Light));
        assert_eq!(side(None, at(2)), Some(Side::Light), "a polar day");
        assert_eq!(side(at(2), None), Some(Side::Dark), "a polar night");
        assert_eq!(side(None, None), None);
    }

    #[test]
    fn the_next_events_are_after_now_and_say_the_side() {
        let now = utc("2026-10-02T12:00:00Z");
        let next = next(&paris(), now);
        let (rise, set) = (next.sunrise.unwrap(), next.sunset.unwrap());
        assert!(
            set > now && set < utc("2026-10-02T18:00:00Z"),
            "today's sunset: {set}"
        );
        assert!(rise > set, "tomorrow's sunrise: {rise}");
        let api = next.api();
        assert_eq!(api.side, Some(Side::Light), "noon");
        assert_eq!(api.sunset_ms, Some(set.timestamp_millis()));
        assert_eq!(SunTimes::default().api().side, None, "no place, no side");
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
        let bare = GeocodingResult {
            name: "Nowhere".into(),
            latitude: 0.0,
            longitude: 0.0,
            admin1: None,
            country: None,
        };
        assert_eq!(place_of(bare).name, "Nowhere");
    }

    /// The search asks the geocoding service (a local stub here) for the name, in the
    /// reader's language, and names what it finds; a short query asks nothing.
    #[tokio::test]
    async fn a_place_is_searched_by_name() {
        use std::sync::{Arc, Mutex};
        let asked: Arc<Mutex<Vec<String>>> = Arc::default();
        let log = asked.clone();
        let app = axum::Router::new()
            .route(
                "/v1/search",
                axum::routing::get(
                    move |axum::extract::RawQuery(query): axum::extract::RawQuery| {
                        let log = log.clone();
                        async move {
                            log.lock().unwrap().push(query.unwrap_or_default());
                            axum::Json(serde_json::json!({ "results": [
                                { "name": "Lyon", "latitude": 45.75, "longitude": 4.85,
                                  "admin1": "Auvergne-Rhône-Alpes", "country": "France", "id": 1 }
                            ] }))
                        }
                    },
                ),
            )
            .route(
                "/down/v1/search",
                axum::routing::get(|| async { axum::http::StatusCode::BAD_GATEWAY }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await });
        let client =
            crate::net::web_client(crate::net::USER_AGENT, std::time::Duration::from_secs(2))
                .unwrap();

        let places = search(&client, &base, " Lyon ", "en").await.unwrap();
        assert_eq!(places.len(), 1);
        assert_eq!(places[0].name, "Lyon, Auvergne-Rhône-Alpes, France");
        let query = asked.lock().unwrap()[0].clone();
        assert!(
            query.contains("name=Lyon") && query.contains("language=en"),
            "{query}"
        );

        assert!(search(&client, &base, "L", "fr").await.unwrap().is_empty());
        assert_eq!(asked.lock().unwrap().len(), 1, "a letter asks nothing");

        let error = search(&client, &format!("{base}/down"), "Lyon", "fr")
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            AppError::Coded {
                code: "service_unreachable",
                ..
            }
        ));
    }
}
