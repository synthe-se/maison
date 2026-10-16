//! Where the colours come from, the first that answers:
//!
//! 1. RTE's official API, with a client id (a free account on data.rte-france.com,
//!    `RTE_CLIENT_ID` / `RTE_CLIENT_SECRET`): OAuth2 client credentials;
//! 2. RTE's public open data, what its Tempo calendar page reads (no key, undocumented);
//! 3. api-couleur-tempo.fr, a community cache of RTE (no key).
//!
//! EDF's own season answer gives the quotas (and is checked against the colours counted).

use std::{collections::BTreeMap, time::Instant};

use chrono::{Duration, NaiveDate};
use maison_api::tempo::Color;
use serde::Deserialize;
use tokio::sync::Mutex;

use super::rules::{season_bounds, season_name, History, Quotas};
use crate::{
    config::Config,
    error::{AppError, Refusal},
    net::fetch_json,
    util::{house_time, HOUSE_TZ},
};

/// RTE's answers and the season files: `{"values": {"2026-01-29": "RED", …}}`. Other keys
/// (RTE's `"2026-01-29-fallback": "false"`) and unknown colours are dropped.
#[derive(Debug, Default, Deserialize, serde::Serialize)]
pub struct Values {
    pub values: BTreeMap<String, String>,
}

impl Values {
    pub fn history(&self) -> History {
        self.values
            .iter()
            .filter_map(|(day, color)| Some((day.parse().ok()?, Color::parse(color)?)))
            .collect()
    }

    pub fn from_history(history: &History) -> Self {
        Self {
            values: history
                .iter()
                .map(|(day, color)| (day.to_string(), color.as_str().to_string()))
                .collect(),
        }
    }
}

pub struct Sources {
    official: Option<Official>,
    public: String,
    fallback: String,
}

struct Official {
    url: String,
    id: String,
    secret: String,
    /// The bearer token and when it expires (RTE's last 2 h).
    token: Mutex<Option<(String, Instant)>>,
}

impl Sources {
    pub fn from_config(config: &Config) -> Self {
        let official = match (&config.rte_client_id, &config.rte_client_secret) {
            (Some(id), Some(secret)) => Some(Official {
                url: config.tempo_rte_api_url.clone(),
                id: id.clone(),
                secret: secret.clone(),
                token: Mutex::new(None),
            }),
            _ => None,
        };
        Self {
            official,
            public: config.tempo_rte_url.clone(),
            fallback: config.tempo_fallback_url.clone(),
        }
    }

    /// A season's colours, tomorrow's included once published, from the first source with
    /// an answer. `current`: the season `today` is in (it has a today and a tomorrow).
    pub async fn season(
        &self,
        client: &reqwest::Client,
        start_year: i32,
        today: NaiveDate,
    ) -> Result<History, AppError> {
        let current = season_bounds(start_year).0 <= today && today <= season_bounds(start_year).1;
        let mut failure = AppError::from(Refusal::ServiceUnreachable);
        if let Some(official) = &self.official {
            match official.season(client, start_year, today).await {
                Ok(history) if !history.is_empty() => return Ok(history),
                Ok(_) => tracing::warn!(season = start_year, "RTE's API answered no day"),
                Err(error) => {
                    tracing::warn!(%error, "RTE's API failed: trying its open data");
                    failure = error;
                }
            }
        }
        match public_season(client, &self.public, start_year, current).await {
            Ok(history) if !history.is_empty() => return Ok(history),
            Ok(_) => tracing::warn!(season = start_year, "RTE's open data answered no day"),
            Err(error) => {
                tracing::warn!(%error, "RTE's open data failed: trying api-couleur-tempo");
                failure = error;
            }
        }
        match fallback_season(client, &self.fallback, start_year, current).await {
            Ok(history) if !history.is_empty() => Ok(history),
            Ok(_) => Err(failure),
            Err(error) => {
                tracing::warn!(%error, "api-couleur-tempo failed too");
                Err(failure)
            }
        }
    }
}

/// `/tempo?season=` (up to today), and `/tempoLight` (today and tomorrow) for the current one.
async fn public_season(
    client: &reqwest::Client,
    url: &str,
    start_year: i32,
    current: bool,
) -> Result<History, AppError> {
    let season = season_name(start_year);
    let request = client
        .get(format!("{url}/tempo"))
        .query(&[("season", season.as_str())]);
    let mut history = fetch_json::<Values>(request, "RTE open data")
        .await?
        .history();
    if current {
        history.extend(
            fetch_json::<Values>(client.get(format!("{url}/tempoLight")), "RTE open data")
                .await?
                .history(),
        );
    }
    Ok(history)
}

/// One day as api-couleur-tempo says it (its own French words).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CouleurDay {
    date_jour: NaiveDate,
    /// 1 blue, 2 white, 3 red, 0 not known yet.
    code_jour: u8,
}

impl CouleurDay {
    fn color(&self) -> Option<(NaiveDate, Color)> {
        let color = match self.code_jour {
            1 => Color::Blue,
            2 => Color::White,
            3 => Color::Red,
            _ => return None,
        };
        Some((self.date_jour, color))
    }
}

/// `/api/joursTempo?periode=` (up to today), and `/api/jourTempo/tomorrow` for the current one.
async fn fallback_season(
    client: &reqwest::Client,
    url: &str,
    start_year: i32,
    current: bool,
) -> Result<History, AppError> {
    let request = client
        .get(format!("{url}/api/joursTempo"))
        .query(&[("periode", season_name(start_year))]);
    let mut history: History = fetch_json::<Vec<CouleurDay>>(request, "api-couleur-tempo")
        .await?
        .iter()
        .filter_map(CouleurDay::color)
        .collect();
    if current {
        let tomorrow: CouleurDay = fetch_json(
            client.get(format!("{url}/api/jourTempo/tomorrow")),
            "api-couleur-tempo",
        )
        .await?;
        history.extend(tomorrow.color());
    }
    Ok(history)
}

#[derive(Deserialize)]
struct Token {
    access_token: String,
    expires_in: u64,
}

/// RTE's API answer (its own snake_case words).
#[derive(Deserialize)]
struct Calendars {
    tempo_like_calendars: Vec<RteCalendar>,
}

#[derive(Deserialize)]
struct RteCalendar {
    values: Vec<RteDay>,
}

#[derive(Deserialize)]
struct RteDay {
    /// `2026-10-03T00:00:00+02:00`
    start_date: String,
    value: String,
}

/// Midnight in Paris, as RTE's API wants its bounds.
fn paris_midnight(day: NaiveDate) -> String {
    house_time(day, 0, 0)
        .with_timezone(&HOUSE_TZ)
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string()
}

impl Official {
    async fn token(&self, client: &reqwest::Client) -> Result<String, AppError> {
        let mut token = self.token.lock().await;
        if let Some((value, expires)) = token.as_ref() {
            if Instant::now() < *expires {
                return Ok(value.clone());
            }
        }
        let request = client
            .post(format!("{}/token/oauth/", self.url))
            .basic_auth(&self.id, Some(&self.secret))
            .form(&[("grant_type", "client_credentials")]);
        let fresh: Token = fetch_json(request, "RTE OAuth").await?;
        // a minute early: never a token that dies on the way
        let lasts = std::time::Duration::from_secs(fresh.expires_in.saturating_sub(60));
        *token = Some((fresh.access_token.clone(), Instant::now() + lasts));
        Ok(fresh.access_token)
    }

    /// The season up to tomorrow (RTE serves at most D+1, in spans up to 366 days).
    async fn season(
        &self,
        client: &reqwest::Client,
        start_year: i32,
        today: NaiveDate,
    ) -> Result<History, AppError> {
        let (first, last) = season_bounds(start_year);
        let end = (last + Duration::days(1)).min(today + Duration::days(2));
        let request = client
            .get(format!(
                "{}/open_api/tempo_like_supply_contract/v1/tempo_like_calendars",
                self.url
            ))
            .bearer_auth(self.token(client).await?)
            .query(&[
                ("start_date", paris_midnight(first)),
                ("end_date", paris_midnight(end)),
            ]);
        let calendars: Calendars = fetch_json(request, "RTE API").await?;
        Ok(calendars
            .tempo_like_calendars
            .iter()
            .flat_map(|c| &c.values)
            .filter_map(|d| {
                Some((
                    d.start_date.get(..10)?.parse().ok()?,
                    Color::parse(&d.value)?,
                ))
            })
            .collect())
    }
}

#[derive(Deserialize)]
struct EdfSeasons {
    content: Vec<EdfSeason>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EdfSeason {
    type_jour_eff: String,
    nombre_jours: u32,
    nombre_jours_tires: u32,
}

/// EDF's season on `day`: the quotas and the days it counts as used `[blue, white, red]`.
pub async fn edf_season(
    client: &reqwest::Client,
    url: &str,
    day: NaiveDate,
) -> Result<(Quotas, [u32; 3]), AppError> {
    let request = client.get(format!("{url}/saisons/search")).query(&[
        ("option", "TEMPO".to_string()),
        ("dateReference", day.to_string()),
    ]);
    let seasons: EdfSeasons = fetch_json(request, "EDF").await?;
    edf_quotas(&seasons, super::rules::season_start_year(day))
}

/// EDF's answer read: its three colours, quotas a season can hold ([`Quotas::checked`]);
/// anything else is the coded `service_unreachable` (EDF answered nonsense).
fn edf_quotas(seasons: &EdfSeasons, start_year: i32) -> Result<(Quotas, [u32; 3]), AppError> {
    let find = |kind: &str| seasons.content.iter().find(|s| s.type_jour_eff == kind);
    let (Some(blue), Some(white), Some(red)) =
        (find("TEMPO_BLEU"), find("TEMPO_BLANC"), find("TEMPO_ROUGE"))
    else {
        tracing::warn!("EDF answered without the three colours");
        return Err(Refusal::ServiceUnreachable.into());
    };
    let Some(quotas) = Quotas::checked(red.nombre_jours, white.nombre_jours, start_year) else {
        tracing::warn!(
            red = red.nombre_jours,
            white = white.nombre_jours,
            "EDF's quotas cannot be a season's"
        );
        return Err(Refusal::ServiceUnreachable.into());
    };
    Ok((
        quotas,
        [
            blue.nombre_jours_tires,
            white.nombre_jours_tires,
            red.nombre_jours_tires,
        ],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tempo::rules::tests::day;

    #[test]
    fn rte_s_extra_keys_are_dropped() {
        let values: Values = serde_json::from_str(
            r#"{"values":{"2026-10-03-fallback":"false","2026-10-03":"BLUE","2026-10-04":"ROUGE","2026-10-05":"?"}}"#,
        )
        .unwrap();
        let history = values.history();
        assert_eq!(history.len(), 2);
        assert_eq!(history[&day(2026, 10, 4)], Color::Red);
        assert_eq!(Values::from_history(&history).values["2026-10-04"], "RED");
    }

    #[test]
    fn rte_s_bounds_are_paris_midnights() {
        assert_eq!(paris_midnight(day(2026, 9, 1)), "2026-09-01T00:00:00+02:00");
        assert_eq!(
            paris_midnight(day(2027, 1, 15)),
            "2027-01-15T00:00:00+01:00"
        );
    }

    #[test]
    fn unknown_days_are_not_colours() {
        let unknown: CouleurDay = serde_json::from_str(r#"{"dateJour":"2026-10-04","codeJour":0,"periode":"2026-2027","libCouleur":"Inconnu"}"#).unwrap();
        assert!(unknown.color().is_none());
        let red: CouleurDay =
            serde_json::from_str(r#"{"dateJour":"2026-01-29","codeJour":3}"#).unwrap();
        assert_eq!(red.color(), Some((day(2026, 1, 29), Color::Red)));
    }

    #[test]
    fn edf_s_quotas_are_checked_before_they_count() {
        let answer = |red: u32, white: u32| -> EdfSeasons {
            serde_json::from_value(serde_json::json!({ "content": [
                { "typeJourEff": "TEMPO_ROUGE", "nombreJours": red, "nombreJoursTires": 1 },
                { "typeJourEff": "TEMPO_BLANC", "nombreJours": white, "nombreJoursTires": 2 },
                { "typeJourEff": "TEMPO_BLEU", "nombreJours": 300, "nombreJoursTires": 3 },
            ] }))
            .unwrap()
        };
        let (quotas, used) = edf_quotas(&answer(20, 45), 2026).unwrap();
        assert_eq!((quotas.red, quotas.white, used), (20, 45, [3, 2, 1]));
        for (red, white) in [(400, 43), (22, 4_000_000_000), (u32::MAX, u32::MAX)] {
            assert!(
                matches!(
                    edf_quotas(&answer(red, white), 2026),
                    Err(AppError::Coded {
                        code: "service_unreachable",
                        ..
                    })
                ),
                "{red}/{white}"
            );
        }
        let partial = EdfSeasons { content: vec![] };
        assert!(edf_quotas(&partial, 2026).is_err());
    }
}
