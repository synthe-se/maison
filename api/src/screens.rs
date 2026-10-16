//! Wall screens (docs/passkeys.md « Wall screens »): pairing, as a TV signs in to an account.

use serde::{Deserialize, Serialize};

/// A pairing, as the screen shows it: its code, the secret it claims with (kept in memory, never
/// in an address), how long it waits and how often it asks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pairing {
    pub code: String,
    pub secret: String,
    pub expires_in_ms: u64,
    pub poll_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimRequest {
    pub secret: String,
}

/// What a claim found: `paired` with the screen's name (its cookie set), or not yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimAnswer {
    pub paired: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<ScreenName>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenName {
    pub name: String,
}

/// The screen, at each start of its page: who it is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Me {
    pub screen: ScreenName,
}

/// An admin pairs the screen showing `code`, as `name`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairRequest {
    pub code: String,
    pub name: String,
}

/// The screen just paired.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Paired {
    pub screen: ScreenName,
}

/// A screen as « Mon compte » lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Screen {
    pub id: String,
    pub name: String,
    pub created_ms: i64,
    pub last_used_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Screens {
    pub screens: Vec<Screen>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Answer;

    #[test]
    fn a_claim_reads_as_the_backend_writes_it() {
        let waiting: Answer<ClaimAnswer> =
            serde_json::from_str(r#"{"success":true,"paired":false}"#).unwrap();
        assert_eq!(
            waiting.body,
            ClaimAnswer {
                paired: false,
                screen: None
            }
        );
        let paired = Answer::new(ClaimAnswer {
            paired: true,
            screen: Some(ScreenName {
                name: "Salon".into(),
            }),
        });
        let json = serde_json::to_string(&paired).unwrap();
        assert_eq!(
            json,
            r#"{"success":true,"paired":true,"screen":{"name":"Salon"}}"#
        );
        assert_eq!(
            serde_json::from_str::<Answer<ClaimAnswer>>(&json).unwrap(),
            paired
        );
    }

    #[test]
    fn a_pairing_is_camel_case() {
        let p = Pairing {
            code: "ABCD-EFGH".into(),
            secret: "s".into(),
            expires_in_ms: 600_000,
            poll_ms: 3_000,
        };
        let json = serde_json::to_value(&p).unwrap();
        assert_eq!(json["expiresInMs"], 600_000);
        assert_eq!(json["pollMs"], 3_000);
    }
}
