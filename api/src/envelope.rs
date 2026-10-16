//! The shape of every answer that says more than « done »: `{success: true, ..body}`, the body's
//! fields flattened in (the body is a struct or a JSON object).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Answer<T> {
    pub success: bool,
    #[serde(flatten)]
    pub body: T,
}

impl<T> Answer<T> {
    /// A successful answer saying `body`.
    pub fn new(body: T) -> Self {
        Self {
            success: true,
            body,
        }
    }
}

/// A plain answer's words (`{success, message}`), for logs and curl: the apps say their own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub message: String,
}

impl Answer<Message> {
    /// `{success: true, message}`.
    pub fn said(message: impl Into<String>) -> Self {
        Self::new(Message {
            message: message.into(),
        })
    }
}

/// An answer about one device: `{success, device: {id, name}, message, ..body}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfDevice<T> {
    pub device: DeviceRef,
    pub message: String,
    #[serde(flatten)]
    pub body: T,
}

/// A device as the device routes name it in their answers (`device: {id, name}`), Tuya and
/// Meross alike.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRef {
    pub id: String,
    pub name: String,
}
