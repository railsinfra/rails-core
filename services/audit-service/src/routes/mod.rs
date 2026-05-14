//! HTTP routes for service health and SDK-public audit ingest.

mod audit_events;
mod health;

use axum::{
    body::{to_bytes, Body},
    http::{header, HeaderValue, Request, StatusCode},
    middleware::{from_fn, Next},
    response::Response,
    routing::get,
    Router,
};
use chrono::Utc;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::users_grpc::UsersGrpc;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub users_grpc: UsersGrpc,
    pub internal_token: Option<String>,
}

pub fn router(pool: PgPool, users_grpc: UsersGrpc, internal_token: Option<String>) -> Router {
    let state = AppState {
        pool,
        users_grpc,
        internal_token,
    };

    Router::new()
        .route("/health", axum::routing::get(health::health_check))
        .route(
            "/api/v1/audit/events",
            get(audit_events::list_audit_events).post(audit_events::append_audit_event),
        )
        .layer(from_fn(correlation_id_middleware))
        .with_state(state)
}

async fn correlation_id_middleware(mut req: Request<Body>, next: Next) -> Response {
    let path = req.uri().path().to_string();
    if !path.starts_with("/api/") {
        return next.run(req).await;
    }

    let existing = req
        .headers()
        .get("x-correlation-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let correlation_id = existing.unwrap_or_else(|| Uuid::new_v4().to_string());

    if let Ok(header_value) = HeaderValue::from_str(&correlation_id) {
        req.headers_mut().insert("x-correlation-id", header_value);
    }

    let mut res = standardize_error_response(next.run(req).await, &correlation_id).await;
    if let Ok(header_value) = HeaderValue::from_str(&correlation_id) {
        res.headers_mut().insert("x-correlation-id", header_value);
    }
    res
}

async fn standardize_error_response(res: Response, correlation_id: &str) -> Response {
    let status = res.status();
    if status < StatusCode::BAD_REQUEST {
        return res;
    }

    let (mut parts, body) = res.into_parts();
    let message = match to_bytes(body, 1024 * 1024).await {
        Ok(bytes) => error_message_from_body(&bytes, status),
        Err(_) => status.canonical_reason().unwrap_or("error").to_string(),
    };
    let body = json!({
        "status": status.as_u16(),
        "message": message,
        "correlationId": correlation_id,
        "timestamp": Utc::now().to_rfc3339(),
    });

    parts.headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json"),
    );
    Response::from_parts(parts, Body::from(body.to_string()))
}

fn error_message_from_body(bytes: &[u8], status: StatusCode) -> String {
    serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()
        .and_then(|body| {
            body.get("message")
                .or_else(|| body.get("error"))
                .and_then(|value| value.as_str())
                .map(str::to_owned)
        })
        .filter(|message| !message.trim().is_empty())
        .unwrap_or_else(|| status.canonical_reason().unwrap_or("error").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_message_from_body_prefers_error_or_message_field() {
        assert_eq!(
            error_message_from_body(br#"{"error":"invalid"}"#, StatusCode::BAD_REQUEST),
            "invalid"
        );
        assert_eq!(
            error_message_from_body(br#"{"message":"missing"}"#, StatusCode::NOT_FOUND),
            "missing"
        );
        assert_eq!(
            error_message_from_body(b"not-json", StatusCode::FORBIDDEN),
            "Forbidden"
        );
    }

    #[tokio::test]
    async fn standardize_error_response_returns_common_shape() {
        let mut res = Response::new(Body::from(r#"{"message":"missing"}"#));
        *res.status_mut() = StatusCode::NOT_FOUND;

        let res = standardize_error_response(res, "cid-789").await;
        let status = res.status();
        let bytes = to_bytes(res.into_body(), 1024).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["status"], 404);
        assert_eq!(body["message"], "missing");
        assert_eq!(body["correlationId"], "cid-789");
        assert!(body["timestamp"].as_str().is_some());
    }
}
