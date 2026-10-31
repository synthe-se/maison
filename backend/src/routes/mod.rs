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
pub mod photos;
pub mod root;
pub mod scenes;
pub mod screens;
pub mod tempo;
pub mod tv;
pub mod zigbee;

/// The answer of a command that only says it was done: `{success: true, message}`.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimpleResponse {
    pub success: bool,
    pub message: String,
}

impl SimpleResponse {
    pub fn ok(message: impl Into<String>) -> axum::Json<Self> {
        axum::Json(Self {
            success: true,
            message: message.into(),
        })
    }
}

/// Any other answer: `{success: true, ..body}`, the body's fields flattened in (the body
/// is a struct or a JSON object). Its shape is the API crate's, shared with the frame.
pub use maison_api::Answer;

/// An answer as a route returns it (`Answer::ok(body)`): the API crate knows no web framework.
pub trait Reply<T> {
    fn ok(body: T) -> axum::Json<Answer<T>>;
}

impl<T: serde::Serialize> Reply<T> for Answer<T> {
    fn ok(body: T) -> axum::Json<Answer<T>> {
        axum::Json(Answer::new(body))
    }
}

/// A device as the device routes name it in their answers (`device: {id, name}`).
pub use maison_api::DeviceRef;
