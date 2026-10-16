//! The sun over the house, as pages read it: where the house is, when the sun next rises and
//! sets there, and the side that says for a page's theme (the backend's `sun::side` decides it).

use serde::{Deserialize, Serialize};

/// A named place: what the person picked, and its coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
}

/// The side the sun says for a page's theme: light from sunrise to sunset, dark the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Light,
    Dark,
}

/// When the sun next rises and sets at the house (ms), and the side it says now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NextSun {
    pub sunrise_ms: Option<i64>,
    pub sunset_ms: Option<i64>,
    pub side: Option<Side>,
}

/// The house's place (`GET /api/matter/place`): it, and the sun there; both null when unset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Where {
    pub place: Option<Place>,
    pub sun: Option<NextSun>,
}

/// Signed out (a screen waiting to be paired), the side alone (`GET /api/sun`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SunNow {
    pub side: Option<Side>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Answer;

    #[test]
    fn the_sun_reads_as_the_backend_writes_it() {
        let json = r#"{"success":true,"place":{"name":"Paris","latitude":48.85,"longitude":2.35},"sun":{"sunriseMs":1,"sunsetMs":2,"side":"dark"}}"#;
        let w: Answer<Where> = serde_json::from_str(json).unwrap();
        assert_eq!(w.body.sun.unwrap().side, Some(Side::Dark));
        let none: Answer<SunNow> = serde_json::from_str(r#"{"success":true,"side":null}"#).unwrap();
        assert_eq!(none.body.side, None);
    }
}
