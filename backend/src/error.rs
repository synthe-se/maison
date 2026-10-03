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
    Coded {
        status: StatusCode,
        code: &'static str,
        message: &'static str,
        retry_after_s: Option<u64>,
        /// What the app needs to act on the refusal (e.g. who already has a name), sent as
        /// `detail`.
        detail: Option<Box<serde_json::Value>>,
    },
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
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<Box<serde_json::Value>>,
}

impl AppError {
    pub fn http(status: StatusCode, message: impl Into<String>) -> Self {
        Self::Http {
            status,
            message: message.into(),
        }
    }

    pub fn coded(status: StatusCode, code: &'static str, message: &'static str) -> Self {
        Self::Coded { status, code, message, retry_after_s: None, detail: None }
    }

    /// A coded refusal with what the app needs to act on it.
    pub fn with_detail(self, value: serde_json::Value) -> Self {
        match self {
            Self::Coded { status, code, message, retry_after_s, .. } => {
                Self::Coded { status, code, message, retry_after_s, detail: Some(Box::new(value)) }
            }
            other => other,
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::http(StatusCode::BAD_REQUEST, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::http(StatusCode::NOT_FOUND, message)
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::http(StatusCode::FORBIDDEN, message)
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
            // a service we asked did not answer (503), or answered nonsense (502): not our bug
            Self::Reqwest(e) if e.is_connect() || e.is_timeout() => StatusCode::SERVICE_UNAVAILABLE,
            Self::Reqwest(_) => StatusCode::BAD_GATEWAY,
            Self::Io(_) | Self::Json(_) | Self::Jwt(_) | Self::Join(_) => StatusCode::INTERNAL_SERVER_ERROR,
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
        match status {
            // a device or a service out there: their trouble, said as a warning
            StatusCode::BAD_GATEWAY | StatusCode::SERVICE_UNAVAILABLE | StatusCode::GATEWAY_TIMEOUT => {
                warn!(error = %self, "upstream unavailable");
            }
            s if s.is_server_error() => error!(error = %self, "request failed"),
            _ => {}
        }
        let (code, retry_after_s, detail) = match &self {
            Self::Coded { code, retry_after_s, detail, .. } => (Some(*code), *retry_after_s, detail.clone()),
            _ => (None, None, None),
        };
        let body = ErrorBody {
            success: false,
            error: self.client_message(),
            code,
            detail,
        };
        let mut response = (status, Json(body)).into_response();
        if let Some(seconds) = retry_after_s {
            response.headers_mut().insert(axum::http::header::RETRY_AFTER, seconds.into());
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reqwest error of each kind, from a real (local) request.
    async fn reqwest_error(url: &str) -> reqwest::Error {
        reqwest::Client::new().get(url).send().await.and_then(|r| r.error_for_status()).expect_err("fails")
    }

    #[tokio::test]
    async fn an_upstream_failure_is_a_gateway_error_not_ours() {
        // nothing listens there: the service is unavailable
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let closed = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let down = AppError::from(reqwest_error(&closed).await);
        assert_eq!(down.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(down.client_message(), "Upstream service unavailable");
        // an answer we cannot use (a 500 of theirs): a bad gateway
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let broken = format!("http://{}", listener.local_addr().unwrap());
        let app = axum::Router::new().route("/", axum::routing::get(|| async { StatusCode::INTERNAL_SERVER_ERROR }));
        tokio::spawn(async move { axum::serve(listener, app).await });
        let bad = AppError::from(reqwest_error(&broken).await);
        assert_eq!(bad.status(), StatusCode::BAD_GATEWAY);
    }
}
