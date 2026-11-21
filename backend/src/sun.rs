//! The sun over the house: where the house is (found once by name through Open-Meteo's
//! geocoding, the same provider as Tempo's weather), and when the sun rises and sets there,
//! computed locally (the sunrise equation, [`sun_times`]), said once: the shutters'
//! schedule, the pages' theme and the place's answer all read [`next`]. The schedule never
//! needs the network at the moment it acts: a shutter still opens at dawn when the internet
//! is down.

use jiff::{Timestamp, civil::Date};
use serde::Deserialize;

use crate::{error::AppError, util::HOUSE_TZ};

/// A named place, the side the sun says, and when it next rises and sets: the API crate's,
/// shared with the frame.
pub use maison_api::sun::{NextSun, Place, Side};

/// Sunrise and sunset on a local calendar day, to the second; `None` in a polar day or night
/// (or at coordinates that are none). The sunrise equation as the `sunrise` crate (3.0.0)
/// computed it, which this replaced (it spoke chrono): the mean solar noon on `day`, the solar
/// mean anomaly, the equation of the centre, the ecliptic longitude, the transit, the
/// declination, then the hour angle where the sun's centre is 50′ below the horizon
/// (refraction and its radius), at sea level. Its outputs are kept in a test.
pub fn sun_times(place: &Place, day: Date) -> (Option<Timestamp>, Option<Timestamp>) {
    use std::f64::consts::PI;
    const SECONDS_IN_A_DAY: f64 = 86_400.0;
    const UNIX_EPOCH_JULIAN_DAY: f64 = 2_440_587.5;
    const J2000: f64 = 2_451_545.0;
    let (lat, lon) = (place.latitude, place.longitude);
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return (None, None);
    }
    let Ok(noon) = jiff::tz::TimeZone::UTC.to_timestamp(day.at(12, 0, 0, 0)) else {
        return (None, None);
    };
    // Julian days: the mean solar noon at this longitude
    let n = noon.as_second() as f64 / SECONDS_IN_A_DAY + UNIX_EPOCH_JULIAN_DAY - lon / 360.0;
    let anomaly = (357.5291 + 0.985_600_28 * (n - J2000))
        .to_radians()
        .rem_euclid(2.0 * PI);
    let center =
        (1.9148 * anomaly.sin() + 0.02 * (2.0 * anomaly).sin() + 0.0003 * (3.0 * anomaly).sin())
            .to_radians();
    let perihelion = (102.930_05 + 0.317_952_6 * (n - J2000) / 36525.0).to_radians();
    let longitude = (anomaly + center + perihelion % (2.0 * PI) + 3.0 * PI) % (2.0 * PI);
    let transit = n + (0.0053 * anomaly.sin() - 0.0069 * (2.0 * longitude).sin());
    let declination = (longitude.sin() * 0.39779).asin();
    let latitude = lat.to_radians();
    let below = 5.0_f64.to_radians() / 6.0;
    let cos_hour_angle =
        (-below.sin() - latitude.sin() * declination.sin()) / (latitude.cos() * declination.cos());
    let at = |sign: f64| {
        let hour_angle = sign * cos_hour_angle.acos();
        if hour_angle.is_nan() {
            return None;
        }
        let julian = transit + hour_angle / (2.0 * PI);
        // whole seconds, toward zero, as the crate did
        Timestamp::from_second(((julian - UNIX_EPOCH_JULIAN_DAY) * SECONDS_IN_A_DAY) as i64).ok()
    };
    (at(-1.0), at(1.0))
}

/// The next sunrise and sunset at a place, strictly after a moment; either is `None` in a
/// polar day or night (or with no place: the default).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SunTimes {
    pub sunrise: Option<Timestamp>,
    pub sunset: Option<Timestamp>,
}

impl SunTimes {
    /// As the API says it: the times (ms) and the side they make now.
    pub fn api(self) -> NextSun {
        NextSun {
            sunrise_ms: self.sunrise.map(|t| t.as_millisecond()),
            sunset_ms: self.sunset.map(|t| t.as_millisecond()),
            side: side(self.sunrise, self.sunset),
        }
    }
}

/// The next sunrise and sunset at `place` after `now` (looking up to two days ahead:
/// today's may be past).
pub fn next(place: &Place, now: Timestamp) -> SunTimes {
    let today = HOUSE_TZ.to_datetime(now).date();
    let mut next = SunTimes::default();
    for day in today.series(jiff::Span::new().days(1)).take(3) {
        let (rise, set) = sun_times(place, day);
        next.sunrise = next.sunrise.or(rise.filter(|t| *t > now));
        next.sunset = next.sunset.or(set.filter(|t| *t > now));
    }
    next
}

/// The side now, from the next sunrise and sunset: dark when the sun rises next, light when it
/// sets next; a polar day or night has only one of the two ahead. `None` when neither is.
pub fn side(next_sunrise: Option<Timestamp>, next_sunset: Option<Timestamp>) -> Option<Side> {
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

    pub(crate) fn paris() -> Place {
        Place {
            name: "Paris".into(),
            latitude: 48.8566,
            longitude: 2.3522,
        }
    }

    fn utc(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    /// The hour and the ten minutes of an instant, in UTC.
    fn utc_hour_tens(t: Timestamp) -> (i8, i8) {
        let t = jiff::tz::TimeZone::UTC.to_datetime(t);
        (t.hour(), t.minute() / 10)
    }

    #[test]
    fn the_sun_says_dark_when_it_rises_next_and_light_when_it_sets_next() {
        let at = |h: u32| Timestamp::from_second(i64::from(h) * 3600).ok();
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
        assert_eq!(api.sunset_ms, Some(set.as_millisecond()));
        assert_eq!(SunTimes::default().api().side, None, "no place, no side");
    }

    #[test]
    fn paris_on_the_summer_solstice() {
        // timeanddate.com: sunrise 05:47, sunset 21:58 CEST (03:47 / 19:58 UTC), 21 June 2026
        let (rise, set) = sun_times(&paris(), jiff::civil::date(2026, 6, 21));
        let (rise, set) = (rise.unwrap(), set.unwrap());
        assert_eq!(utc_hour_tens(rise), (3, 4), "{rise}");
        assert_eq!(utc_hour_tens(set), (19, 5), "{set}");
    }

    #[test]
    fn paris_on_the_winter_solstice() {
        // sunrise 08:42, sunset 16:56 CET (07:42 / 15:56 UTC), 21 December 2026
        let (rise, set) = sun_times(&paris(), jiff::civil::date(2026, 12, 21));
        let (rise, set) = (rise.unwrap(), set.unwrap());
        assert_eq!(utc_hour_tens(rise), (7, 4), "{rise}");
        assert_eq!(utc_hour_tens(set), (15, 5), "{set}");
    }

    /// Latitude, longitude, (month, day) of 2026, sunrise and sunset in Unix seconds.
    type Golden = (f64, f64, (i8, i8), Option<i64>, Option<i64>);

    /// What the `sunrise` crate (3.0.0) answered, before `sun_times` replaced it: both
    /// equinoxes, both solstices and both days the clocks change, at Paris, the equator, 45° S,
    /// 60° N, Tromsø (its midnight sun and polar night) and 40° N 120° W. Kept within a second
    /// (found equal to the second).
    const SUNRISE_CRATE: &[Golden] = &[
        (48.8566, 2.3522, (3, 20), Some(1773986036), Some(1774029738)), // paris
        (48.8566, 2.3522, (3, 29), Some(1774762491), Some(1774808143)), // paris
        (48.8566, 2.3522, (6, 21), Some(1782013613), Some(1782071864)), // paris
        (48.8566, 2.3522, (9, 23), Some(1790141890), Some(1790185677)), // paris
        (
            48.8566,
            2.3522,
            (10, 25),
            Some(1792909552),
            Some(1792946594),
        ), // paris
        (
            48.8566,
            2.3522,
            (12, 21),
            Some(1797838875),
            Some(1797868552),
        ), // paris
        (0.0, 0.0, (3, 20), Some(1773986651), Some(1774030251)),        // equator
        (0.0, 0.0, (3, 29), Some(1774764081), Some(1774807682)),        // equator
        (0.0, 0.0, (6, 21), Some(1782021485), Some(1782065121)),        // equator
        (0.0, 0.0, (9, 23), Some(1790142548), Some(1790186148)),        // equator
        (0.0, 0.0, (10, 25), Some(1792906833), Some(1792950442)),       // equator
        (0.0, 0.0, (12, 21), Some(1797832460), Some(1797876096)),       // equator
        (-45.0, 170.0, (3, 20), Some(1773945686), Some(1773989633)),    // south45
        (-45.0, 170.0, (3, 29), Some(1774723967), Some(1774766213)),    // south45
        (-45.0, 170.0, (6, 21), Some(1781986723), Some(1782018271)),    // south45
        (-45.0, 170.0, (9, 23), Some(1790101710), Some(1790145407)),    // south45
        (-45.0, 170.0, (10, 25), Some(1792863023), Some(1792912658)),   // south45
        (-45.0, 170.0, (12, 21), Some(1797785356), Some(1797841574)),   // south45
        (60.0, 10.0, (3, 20), Some(1773984135), Some(1774027968)),      // north60
        (60.0, 10.0, (3, 29), Some(1774760087), Some(1774806876)),      // north60
        (60.0, 10.0, (6, 21), Some(1782006942), Some(1782074864)),      // north60
        (60.0, 10.0, (9, 23), Some(1790139961), Some(1790183936)),      // north60
        (60.0, 10.0, (10, 25), Some(1792909412), Some(1792943063)),     // north60
        (60.0, 10.0, (12, 21), Some(1797841314), Some(1797862441)),     // north60
        (69.65, 18.96, (3, 20), Some(1773981863), Some(1774025940)),    // tromso
        (69.65, 18.96, (3, 29), Some(1774756982), Some(1774805682)),    // tromso
        (69.65, 18.96, (6, 21), None, None),                            // tromso
        (69.65, 18.96, (9, 23), Some(1790137637), Some(1790181961)),    // tromso
        (69.65, 18.96, (10, 25), Some(1792910213), Some(1792937961)),   // tromso
        (69.65, 18.96, (12, 21), None, None),                           // tromso
        (40.0, -120.0, (3, 20), Some(1774015396), Some(1774059094)),    // west
        (40.0, -120.0, (3, 29), Some(1774792111), Some(1774837239)),    // west
        (40.0, -120.0, (6, 21), Some(1782045080), Some(1782099134)),    // west
        (40.0, -120.0, (9, 23), Some(1790171314), Some(1790214967)),    // west
        (40.0, -120.0, (10, 25), Some(1792938069), Some(1792976802)),   // west
        (40.0, -120.0, (12, 21), Some(1797866304), Some(1797899872)),   // west
    ];

    #[test]
    fn the_sun_rises_and_sets_when_the_sunrise_crate_said() {
        let mut worst = 0;
        for &(latitude, longitude, (month, day), rise, set) in SUNRISE_CRATE {
            let place = Place {
                name: "x".into(),
                latitude,
                longitude,
            };
            let (r, s) = sun_times(&place, jiff::civil::date(2026, month, day));
            let seconds = |t: Option<Timestamp>| t.map(Timestamp::as_second);
            for (got, want) in [(seconds(r), rise), (seconds(s), set)] {
                assert_eq!(
                    got.is_some(),
                    want.is_some(),
                    "{latitude} {longitude} {month}-{day}"
                );
                if let (Some(got), Some(want)) = (got, want) {
                    worst = worst.max((got - want).abs());
                }
            }
        }
        assert!(worst <= 1, "{worst} s off");
    }

    #[test]
    fn no_sunset_in_a_polar_day() {
        let tromso = Place {
            name: "Tromsø".into(),
            latitude: 69.65,
            longitude: 18.96,
        };
        let (rise, set) = sun_times(&tromso, jiff::civil::date(2026, 6, 21));
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
            sun_times(&nowhere, jiff::civil::date(2026, 6, 21)),
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
