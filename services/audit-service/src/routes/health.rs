//! `GET /health` for gateway / CI (RAI-12 parity on JSON shape with Rust peers).

use axum::{http::StatusCode, Json};
use sentry::TransactionContext;
use serde_json::json;

pub async fn health_check() -> (StatusCode, Json<serde_json::Value>) {
    let txn = sentry::start_transaction(TransactionContext::new("GET /health", "http.server"));
    txn.finish();
    (
        StatusCode::OK,
        Json(json!({
            "status": "healthy",
            "service": "audit-service"
        })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn health_payload() {
        let (s, j) = health_check().await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(j["status"], "healthy");
        assert_eq!(j["service"], "audit-service");
    }
}
