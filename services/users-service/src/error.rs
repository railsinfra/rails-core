use thiserror::Error;
use axum::{http::StatusCode, response::{IntoResponse, Response}};

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
        let (status, code, details, should_report) = match &self {
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", None, false),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "forbidden", None, false),
            AppError::UnrecognizedSource => (StatusCode::FORBIDDEN, "unrecognized_source", None, true), // Security issue
            AppError::TooManyRequests => (StatusCode::TOO_MANY_REQUESTS, "rate_limited", None, false),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad_request", Some(msg.clone()), false),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, "conflict", Some(msg.clone()), false),
            AppError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal", None, true), // Always report internal errors
        };
        
        // Report critical errors to Sentry
        if should_report {
            sentry::capture_message(&self.to_string(), sentry::Level::Error);
        }
        
        // Return explicit technical error messages - transformation happens in client-server
        let mut body = serde_json::json!({
            "error": self.to_string(),
            "code": code
        });
        if let Some(details) = details {
            body["details"] = serde_json::Value::String(details);
        }
        (status, axum::Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::AppError;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    #[test]
    fn into_response_status_codes() {
        let cases: Vec<(AppError, StatusCode)> = vec![
            (AppError::Unauthorized, StatusCode::UNAUTHORIZED),
            (AppError::Forbidden, StatusCode::FORBIDDEN),
            (AppError::UnrecognizedSource, StatusCode::FORBIDDEN),
            (AppError::TooManyRequests, StatusCode::TOO_MANY_REQUESTS),
            (
                AppError::BadRequest("x".into()),
                StatusCode::BAD_REQUEST,
            ),
            (AppError::Conflict("dup".into()), StatusCode::CONFLICT),
            (AppError::Internal, StatusCode::INTERNAL_SERVER_ERROR),
        ];
        for (err, expected) in cases {
            assert_eq!(err.into_response().status(), expected);
        }
    }
}
