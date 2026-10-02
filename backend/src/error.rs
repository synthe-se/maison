use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;
use thiserror::Error;
use tracing::{error, warn};

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{message}")]
    Http { status: StatusCode, message: String },
    /// A refusal the app explains in its own words: `code` names it (`passkey_rejected`,
    /// `last_passkey`…), the message is for the logs and curl.
    #[error("{code}: {message}")]
    Coded { status: StatusCode, code: &'static str, message: &'static str, retry_after_s: Option<u64> },
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),
    #[error("{0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("{0}")]
    Join(#[from] tokio::task::JoinError),
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    success: bool,
    error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'static str>,
}

impl AppError {
    pub fn http(status: StatusCode, message: impl Into<String>) -> Self {
        Self::Http {
            status,
            message: message.into(),
        }
    }

    pub fn coded(status: StatusCode, code: &'static str, message: &'static str) -> Self {
        Self::Coded { status, code, message, retry_after_s: None }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::http(StatusCode::UNAUTHORIZED, message)
    }

    pub fn service_unavailable(message: impl Into<String>) -> Self {
        Self::http(StatusCode::SERVICE_UNAVAILABLE, message)
    }

    fn status(&self) -> StatusCode {
        match self {
            Self::Http { status, .. } | Self::Coded { status, .. } => *status,
            Self::Io(_) | Self::Json(_) | Self::Jwt(_) | Self::Reqwest(_) | Self::Join(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    fn client_message(&self) -> String {
        match self {
            Self::Http { message, .. } => message.clone(),
            Self::Coded { message, .. } => (*message).to_string(),
            Self::Io(_) | Self::Json(_) | Self::Jwt(_) | Self::Join(_) => {
                "Internal server error".to_string()
            }
            Self::Reqwest(_) => "Upstream service unavailable".to_string(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let status = self.status();
        if status == StatusCode::SERVICE_UNAVAILABLE {
            warn!(error = %self, "upstream device unavailable");
        } else if status.is_server_error() {
            error!(error = %self, "request failed");
        }
        let (code, retry_after_s) = match &self {
            Self::Coded { code, retry_after_s, .. } => (Some(*code), *retry_after_s),
            _ => (None, None),
        };
        let body = ErrorBody {
            success: false,
            error: self.client_message(),
            code,
        };
        let mut response = (status, Json(body)).into_response();
        if let Some(seconds) = retry_after_s {
            response.headers_mut().insert(axum::http::header::RETRY_AFTER, seconds.into());
        }
        response
    }
}
