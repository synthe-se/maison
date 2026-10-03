//! The regulated Tempo prices (EDF's « tarif bleu », 6 kVA), from the CRE's open data on
//! data.gouv.fr: per colour, peak (06:00–22:00) and off-peak, tax included, €/kWh.
//!
//! The dataset writes its start dates year-day-month (`2026-01-08` is 1 August 2026: the
//! prices change on the first of a month), its end dates the usual way.

use chrono::NaiveDate;
use serde::{Deserialize, Deserializer, Serialize};

use crate::error::AppError;

/// One colour's prices: off-peak (« heures creuses », HC) and peak (« heures pleines », HP).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Price {
    pub off_peak: f64,
    pub peak: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tariffs {
    pub blue: Price,
    pub white: Price,
    pub red: Price,
    /// The yearly subscription, tax included (€).
    pub subscription: Option<f64>,
    /// When these prices came into force.
    pub starts_on: NaiveDate,
}

#[derive(Deserialize)]
struct Page {
    data: Vec<Row>,
}

/// A number, or a number in a string (the dataset has had both).
fn number<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Number {
        Float(f64),
        Text(String),
    }
    Ok(match Option::<Number>::deserialize(d)? {
        Some(Number::Float(value)) => Some(value),
        Some(Number::Text(text)) => text.trim().parse().ok(),
        None => None,
    })
}

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct Row {
    DATE_DEBUT: String,
    DATE_FIN: Option<String>,
    #[serde(default, deserialize_with = "number")]
    PART_FIXE_TTC: Option<f64>,
    #[serde(default, deserialize_with = "number")]
    PART_VARIABLE_HCBleu_TTC: Option<f64>,
    #[serde(default, deserialize_with = "number")]
    PART_VARIABLE_HPBleu_TTC: Option<f64>,
    #[serde(default, deserialize_with = "number")]
    PART_VARIABLE_HCBlanc_TTC: Option<f64>,
    #[serde(default, deserialize_with = "number")]
    PART_VARIABLE_HPBlanc_TTC: Option<f64>,
    #[serde(default, deserialize_with = "number")]
    PART_VARIABLE_HCRouge_TTC: Option<f64>,
    #[serde(default, deserialize_with = "number")]
    PART_VARIABLE_HPRouge_TTC: Option<f64>,
}

/// A start date: year-day-month when it reads as the first of a month that way, else as
/// written.
pub fn start_date(text: &str) -> Option<NaiveDate> {
    let mut parts = text.trim().splitn(3, '-').map(str::parse::<u32>);
    let (year, second, third) = (parts.next()?.ok()?, parts.next()?.ok()?, parts.next()?.ok()?);
    if second == 1 {
        if let Some(date) = NaiveDate::from_ymd_opt(year as i32, third, 1) {
            return Some(date);
        }
    }
    NaiveDate::from_ymd_opt(year as i32, second, third)
}

/// The prices in force on `today` from the dataset's latest row (`None` when it is over or
/// incomplete).
pub fn parse(body: &str, today: NaiveDate) -> Result<Option<Tariffs>, AppError> {
    let page: Page = serde_json::from_str(body)?;
    let Some(row) = page.data.first() else { return Ok(None) };
    let ended = row.DATE_FIN.as_deref().and_then(|d| d.parse::<NaiveDate>().ok()).is_some_and(|end| end < today);
    let price = |off_peak: Option<f64>, peak: Option<f64>| Some(Price { off_peak: off_peak?, peak: peak? });
    let tariffs = (|| {
        Some(Tariffs {
            blue: price(row.PART_VARIABLE_HCBleu_TTC, row.PART_VARIABLE_HPBleu_TTC)?,
            white: price(row.PART_VARIABLE_HCBlanc_TTC, row.PART_VARIABLE_HPBlanc_TTC)?,
            red: price(row.PART_VARIABLE_HCRouge_TTC, row.PART_VARIABLE_HPRouge_TTC)?,
            subscription: row.PART_FIXE_TTC,
            starts_on: start_date(&row.DATE_DEBUT)?,
        })
    })();
    Ok(tariffs.filter(|_| !ended))
}

pub async fn fetch(client: &reqwest::Client, url: &str, today: NaiveDate) -> Result<Option<Tariffs>, AppError> {
    let response = client.get(url).header("Accept", "application/json").send().await?;
    if !response.status().is_success() {
        return Err(AppError::service_unavailable(format!("data.gouv: {}", response.status())));
    }
    parse(&response.text().await?, today)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tempo::rules::tests::day;

    /// data.gouv's answer on 3 October 2026 (one row).
    const AUGUST_2026: &str = r#"{"data":[{"__id":127,"DATE_DEBUT":"2026-01-08","DATE_FIN":null,"P_SOUSCRITE":6,"PART_FIXE_HT":142.68,"PART_FIXE_TTC":189.9802924,"PART_VARIABLE_HCBleu_TTC":0.1356,"PART_VARIABLE_HPBleu_TTC":0.1654,"PART_VARIABLE_HCBlanc_TTC":0.1536,"PART_VARIABLE_HPBlanc_TTC":0.1921,"PART_VARIABLE_HCRouge_TTC":0.1615,"PART_VARIABLE_HPRouge_TTC":0.7295}],"links":{},"meta":{}}"#;

    #[test]
    fn start_dates_are_year_day_month() {
        assert_eq!(start_date("2026-01-08"), Some(day(2026, 8, 1)));
        assert_eq!(start_date("2026-01-02"), Some(day(2026, 2, 1)));
        assert_eq!(start_date("2019-01-06"), Some(day(2019, 6, 1)));
        assert_eq!(start_date("2026-08-01"), Some(day(2026, 8, 1)), "a plain date stays");
        assert_eq!(start_date("2026-01-01"), Some(day(2026, 1, 1)));
        assert_eq!(start_date("garbage"), None);
    }

    #[test]
    fn august_2026_prices() {
        let t = parse(AUGUST_2026, day(2026, 10, 3)).unwrap().unwrap();
        assert_eq!(t.starts_on, day(2026, 8, 1));
        assert_eq!((t.blue.off_peak, t.blue.peak, t.white.peak, t.red.peak), (0.1356, 0.1654, 0.1921, 0.7295));
        assert_eq!(t.subscription, Some(189.9802924));
    }

    #[test]
    fn prices_as_text_or_over() {
        let text = AUGUST_2026.replace("0.7295", "\"0.7295\"");
        assert_eq!(parse(&text, day(2026, 10, 3)).unwrap().unwrap().red.peak, 0.7295);
        let over = AUGUST_2026.replace("\"DATE_FIN\":null", "\"DATE_FIN\":\"2026-09-30\"");
        assert_eq!(parse(&over, day(2026, 10, 3)).unwrap(), None);
        let missing = AUGUST_2026.replace("\"PART_VARIABLE_HPRouge_TTC\":0.7295", "\"PART_VARIABLE_HPRouge_TTC\":null");
        assert_eq!(parse(&missing, day(2026, 10, 3)).unwrap(), None);
        assert_eq!(parse(r#"{"data":[]}"#, day(2026, 10, 3)).unwrap(), None);
    }
}
