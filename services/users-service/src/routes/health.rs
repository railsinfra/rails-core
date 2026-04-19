use axum::{http::StatusCode, Json};
use serde_json::json;

pub async fn health_check() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::OK,
        Json(json!({
            "status": "healthy",
            "service": "users-service"
        })),
    )
}

#[cfg(test)]
mod tests {
    use super::health_check;

    #[tokio::test]
    async fn health_check_returns_ok_payload() {
        let (status, body) = health_check().await;
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(body["status"], "healthy");
        assert_eq!(body["service"], "users-service");
    }
}
