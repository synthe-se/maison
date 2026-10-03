pub mod androidtv;
pub mod auth;
pub mod broadlink;
pub mod devices;
pub mod hue;
pub mod ir;
pub mod lamps;
pub mod matter;
pub mod meross;
pub mod nabaztag;
pub mod passkeys;
pub mod root;
pub mod tempo;
pub mod tv;
pub mod zigbee;

/// The answer of a command that only says it was done: `{success: true, message}`.
#[derive(Debug, serde::Serialize)]
pub struct SimpleResponse {
    pub success: bool,
    pub message: String,
}

impl SimpleResponse {
    pub fn ok(message: impl Into<String>) -> axum::Json<Self> {
        axum::Json(Self { success: true, message: message.into() })
    }
}
