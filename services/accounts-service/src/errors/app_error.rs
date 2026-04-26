use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Business logic error: {0}")]
    BusinessLogic(String),

    #[error("Account is not active")]
    AccountNotActive,

    #[error("Insufficient funds: cannot withdraw or transfer more than available balance")]
    InsufficientFunds,

    #[error("Invalid account type")]
    InvalidAccountType,

    #[error("Too many requests")]
    TooManyRequests,

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Internal server error: {0}")]
    Internal(String),
}

impl AppError {
    pub fn status_code(&self) -> u16 {
        match self {
            AppError::Database(_) => 500,
            AppError::NotFound(_) => 404,
            AppError::Validation(_) | AppError::BusinessLogic(_) | AppError::AccountNotActive
            | AppError::InsufficientFunds | AppError::InvalidAccountType => 400,
            AppError::TooManyRequests => 429,
            AppError::Unauthorized(_) => 401,
            AppError::Internal(_) => 500,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message, _should_report) = match &self {
            AppError::Database(ref e) => {
                tracing::error!("Database error: {}", e);
                // Always report database errors - they're critical
                sentry::capture_error(e);
                (StatusCode::INTERNAL_SERVER_ERROR, self.to_string(), true)
            }
            AppError::NotFound(_) => (StatusCode::NOT_FOUND, self.to_string(), false),
            AppError::Validation(_) => (StatusCode::BAD_REQUEST, self.to_string(), false),
            AppError::BusinessLogic(_) => (StatusCode::BAD_REQUEST, self.to_string(), false),
            AppError::AccountNotActive => (StatusCode::BAD_REQUEST, self.to_string(), false),
            AppError::InsufficientFunds => (StatusCode::BAD_REQUEST, self.to_string(), false),
            AppError::InvalidAccountType => (StatusCode::BAD_REQUEST, self.to_string(), false),
            AppError::TooManyRequests => (StatusCode::TOO_MANY_REQUESTS, self.to_string(), false),
            AppError::Unauthorized(_) => (StatusCode::UNAUTHORIZED, self.to_string(), false),
            AppError::Internal(ref e) => {
                tracing::error!("Internal error: {}", e);
                // Always report internal errors
                sentry::capture_message(e, sentry::Level::Error);
                (StatusCode::INTERNAL_SERVER_ERROR, self.to_string(), true)
            }
        };

        let body = Json(json!({
            "error": error_message,
            "status": status.as_u16()
        }));

        (status, body).into_response()
    }
}
