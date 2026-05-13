use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use thiserror::Error;
use uuid::Uuid;

/// User-facing message when duplicate email registration is attempted.
pub const DUPLICATE_EMAIL_MESSAGE: &str =
    "An account with this email already exists. Try signing in or resetting your password.";

/// User-facing message when duplicate beta application is attempted.
pub const DUPLICATE_BETA_EMAIL_MESSAGE: &str =
    "An application with this email has already been submitted. We'll be in touch shortly.";

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Unauthorized")]
    Unauthorized,
    #[error("Forbidden")]
    Forbidden,
    #[error("Request rejected: API called from an unrecognized source.")]
    UnrecognizedSource,
    #[error("Too many requests")]
    TooManyRequests,
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Conflict: {0}")]
    Conflict(String),
    #[error("Internal server error")]
    Internal,
}

impl AppError {
    /// HTTP status for this error (keeps parity with [`IntoResponse`]).
    pub fn status_code(&self) -> u16 {
        match self {
            AppError::Unauthorized => StatusCode::UNAUTHORIZED.as_u16(),
            AppError::Forbidden | AppError::UnrecognizedSource => StatusCode::FORBIDDEN.as_u16(),
            AppError::TooManyRequests => StatusCode::TOO_MANY_REQUESTS.as_u16(),
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST.as_u16(),
            AppError::Conflict(_) => StatusCode::CONFLICT.as_u16(),
            AppError::Internal => StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, should_report) = match &self {
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, false),
            AppError::Forbidden => (StatusCode::FORBIDDEN, false),
            AppError::UnrecognizedSource => (StatusCode::FORBIDDEN, true), // Security issue
            AppError::TooManyRequests => (StatusCode::TOO_MANY_REQUESTS, false),
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, false),
            AppError::Conflict(_) => (StatusCode::CONFLICT, false),
            AppError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, true), // Always report internal errors
        };

        // Report critical errors to Sentry
        if should_report {
            sentry::capture_message(&self.to_string(), sentry::Level::Error);
        }

        let body = serde_json::json!({
            "status": status.as_u16(),
            "message": self.to_string(),
            "correlationId": Uuid::new_v4().to_string(),
            "timestamp": Utc::now().to_rfc3339(),
        });
        (status, axum::Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::AppError;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    #[test]
    fn status_code_matches_http_mapping() {
        assert_eq!(AppError::Unauthorized.status_code(), 401);
        assert_eq!(AppError::Forbidden.status_code(), 403);
        assert_eq!(AppError::UnrecognizedSource.status_code(), 403);
        assert_eq!(AppError::TooManyRequests.status_code(), 429);
        assert_eq!(AppError::BadRequest("bad".into()).status_code(), 400);
        assert_eq!(AppError::Conflict("dup".into()).status_code(), 409);
        assert_eq!(AppError::Internal.status_code(), 500);
    }

    #[test]
    fn into_response_status_codes() {
        let cases: Vec<(AppError, StatusCode)> = vec![
            (AppError::Unauthorized, StatusCode::UNAUTHORIZED),
            (AppError::Forbidden, StatusCode::FORBIDDEN),
            (AppError::UnrecognizedSource, StatusCode::FORBIDDEN),
            (AppError::TooManyRequests, StatusCode::TOO_MANY_REQUESTS),
            (AppError::BadRequest("x".into()), StatusCode::BAD_REQUEST),
            (AppError::Conflict("dup".into()), StatusCode::CONFLICT),
            (AppError::Internal, StatusCode::INTERNAL_SERVER_ERROR),
        ];
        for (err, expected) in cases {
            assert_eq!(err.into_response().status(), expected);
        }
    }
}
