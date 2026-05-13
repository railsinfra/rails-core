use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
    middleware::{from_fn, Next},
    response::Response,
    routing::{get, post},
    Router,
};
use chrono::Utc;
use serde_json::json;
use sqlx::PgPool;
use std::sync::OnceLock;
use std::time::Duration;
use tonic::transport::Channel;
use uuid::Uuid;

use crate::grpc::audit_proto::audit_service_client::AuditServiceClient;

use crate::handlers::{
    accounts::*,
    health::{health_check, service_root},
    transactions::{get_transaction, list_account_transactions, list_transactions},
};

use crate::errors::AppError;
use crate::ledger_grpc::LedgerGrpc;
use crate::routes::rate_limit::{extract_client_key, RateLimitConfig, RateLimiter};
use crate::users_grpc::UsersGrpc;

fn log_request_boundary(
    phase: &str,
    method: &str,
    path: &str,
    correlation_id: &str,
    status: Option<u16>,
    duration_ms: Option<u64>,
) {
    let banner = format!(
        "-----------------------[{phase} - {} {}]----------------------------",
        method, path
    );
    tracing::info!(
        target = "accounts.request_boundary",
        marker = %banner,
        phase,
        method,
        path,
        correlation_id,
        status = status.unwrap_or(0),
        duration_ms = duration_ms.unwrap_or(0),
        "{banner}"
    );
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub ledger_grpc: LedgerGrpc,
    pub users_grpc: UsersGrpc,
    pub audit_client: Option<AuditServiceClient<Channel>>,
}

pub fn create_router(
    pool: PgPool,
    ledger_grpc: LedgerGrpc,
    users_grpc: UsersGrpc,
    audit_client: Option<AuditServiceClient<Channel>>,
) -> Router {
    let state = AppState {
        pool,
        ledger_grpc,
        users_grpc,
        audit_client,
    };
    Router::<AppState>::new()
        .route("/", get(service_root))
        .route("/health", get(health_check))
        .nest("/api/v1", create_api_routes())
        .layer(from_fn(correlation_id_middleware))
        .with_state(state)
}

fn create_api_routes() -> Router<AppState> {
    let standard_routes = Router::<AppState>::new()
        .route("/accounts", post(create_account).get(list_accounts))
        .route(
            "/accounts/:id",
            get(get_account)
                .patch(update_account_status)
                .delete(close_account),
        )
        .route(
            "/accounts/:account_id/transactions",
            get(list_account_transactions),
        )
        .route("/transactions", get(list_transactions))
        .route("/transactions/:id", get(get_transaction));

    let money_mutations = Router::<AppState>::new()
        .route("/accounts/:id/deposit", post(deposit))
        .route("/accounts/:id/withdraw", post(withdraw))
        .route("/accounts/:id/transfer", post(transfer))
        .layer(from_fn(money_rate_limit_middleware));

    standard_routes.merge(money_mutations)
}

async fn correlation_id_middleware(req: Request<Body>, next: Next) -> Result<Response, AppError> {
    let path = req.uri().path().to_string();
    let method = req.method().to_string();

    if !path.starts_with("/api/") {
        return Ok(next.run(req).await);
    }

    let existing = req
        .headers()
        .get("x-correlation-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let correlation_id = existing
        .as_deref()
        .map(ToString::to_string)
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let mut req = req;
    if existing.is_none() {
        req.headers_mut().insert(
            "x-correlation-id",
            correlation_id
                .parse()
                .map_err(|_| AppError::Internal("Failed to set correlation id".to_string()))?,
        );
    }

    let start = std::time::Instant::now();
    log_request_boundary("START", &method, &path, &correlation_id, None, None);
    tracing::info!(correlation_id = %correlation_id, %method, %path, "start");

    let mut res = standardize_error_response(next.run(req).await, &correlation_id).await?;
    res.headers_mut().insert(
        "x-correlation-id",
        correlation_id
            .parse()
            .map_err(|_| AppError::Internal("Failed to set correlation id".to_string()))?,
    );
    let status = res.status().as_u16();
    let duration_ms = start.elapsed().as_millis();
    let outcome = if status >= 400 { "failed" } else { "success" };

    if status >= 500 {
        tracing::error!(correlation_id = %correlation_id, %method, %path, status = status, duration_ms = duration_ms as u64, outcome = outcome, "finish");
    } else if status >= 400 {
        tracing::warn!(correlation_id = %correlation_id, %method, %path, status = status, duration_ms = duration_ms as u64, outcome = outcome, "finish");
    } else {
        tracing::info!(correlation_id = %correlation_id, %method, %path, status = status, duration_ms = duration_ms as u64, outcome = outcome, "finish");
    }
    log_request_boundary(
        "END",
        &method,
        &path,
        &correlation_id,
        Some(status),
        Some(duration_ms as u64),
    );

    Ok(res)
}

async fn standardize_error_response(
    res: Response,
    correlation_id: &str,
) -> Result<Response, AppError> {
    let status = res.status();
    if status < StatusCode::BAD_REQUEST {
        return Ok(res);
    }

    let (mut parts, body) = res.into_parts();
    let bytes = to_bytes(body, 1024 * 1024)
        .await
        .map_err(|_| AppError::Internal("Failed to read error response body".to_string()))?;
    let message = error_message_from_body(&bytes, status);
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
    Ok(Response::from_parts(parts, Body::from(body.to_string())))
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

static MONEY_RATE_LIMITER: OnceLock<RateLimiter> = OnceLock::new();

fn money_rate_limit_config() -> RateLimitConfig {
    const ACCOUNTS_MONEY_RATE_LIMIT_WINDOW_SECONDS_ENV: &str =
        "ACCOUNTS_MONEY_RATE_LIMIT_WINDOW_SECONDS";
    const ACCOUNTS_MONEY_RATE_LIMIT_MAX_ENV: &str = "ACCOUNTS_MONEY_RATE_LIMIT_MAX";
    let window_seconds = std::env::var(ACCOUNTS_MONEY_RATE_LIMIT_WINDOW_SECONDS_ENV)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(60);
    let max_requests = std::env::var(ACCOUNTS_MONEY_RATE_LIMIT_MAX_ENV)
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(20);
    RateLimitConfig {
        window: Duration::from_secs(window_seconds),
        max: max_requests,
    }
}

fn money_rate_limiter() -> &'static RateLimiter {
    MONEY_RATE_LIMITER.get_or_init(|| RateLimiter::new(money_rate_limit_config()))
}

async fn money_rate_limit_middleware(req: Request<Body>, next: Next) -> Result<Response, AppError> {
    let client_key = extract_client_key(&req, "ACCOUNTS_TRUSTED_PROXY_IPS");
    if !money_rate_limiter().allow(&client_key) {
        return Err(AppError::TooManyRequests);
    }
    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::ConnectInfo;
    use axum::http::Request;
    use std::net::SocketAddr;
    use std::sync::{Mutex, OnceLock};

    fn reset_money_rate_limiter() {
        if let Some(limiter) = MONEY_RATE_LIMITER.get() {
            limiter.reset();
        }
    }

    fn test_env_lock() -> std::sync::MutexGuard<'static, ()> {
        static TEST_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        TEST_ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    const ACCOUNTS_TRUSTED_PROXY_IPS: &str = "ACCOUNTS_TRUSTED_PROXY_IPS";

    #[test]
    fn money_rate_limit_blocks_after_max() {
        reset_money_rate_limiter();
        let config = money_rate_limit_config();
        let client = "10.0.0.1";
        for _ in 0..config.max {
            assert!(money_rate_limiter().allow(client));
        }
        assert!(!money_rate_limiter().allow(client));
    }

    #[test]
    fn extract_client_key_uses_peer_ip_when_untrusted() {
        let _lock = test_env_lock();
        std::env::remove_var(ACCOUNTS_TRUSTED_PROXY_IPS);
        let mut req = Request::builder()
            .uri("/api/v1/accounts/1/deposit")
            .header("x-forwarded-for", "203.0.113.10")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 8080))));
        assert_eq!(
            extract_client_key(&req, ACCOUNTS_TRUSTED_PROXY_IPS),
            "127.0.0.1"
        );
    }

    #[test]
    fn extract_client_key_uses_forwarded_ip_when_trusted() {
        let _lock = test_env_lock();
        std::env::set_var(ACCOUNTS_TRUSTED_PROXY_IPS, "127.0.0.1");
        let mut req = Request::builder()
            .uri("/api/v1/accounts/1/deposit")
            .header("x-forwarded-for", "203.0.113.10, 127.0.0.1")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 8080))));
        assert_eq!(
            extract_client_key(&req, ACCOUNTS_TRUSTED_PROXY_IPS),
            "203.0.113.10"
        );
    }

    #[test]
    fn extract_client_key_ignores_forwarded_ip_when_last_hop_untrusted() {
        let _lock = test_env_lock();
        std::env::set_var(ACCOUNTS_TRUSTED_PROXY_IPS, "127.0.0.1");
        let mut req = Request::builder()
            .uri("/api/v1/accounts/1/deposit")
            .header("x-forwarded-for", "203.0.113.10, 198.51.100.5")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 8080))));
        assert_eq!(
            extract_client_key(&req, ACCOUNTS_TRUSTED_PROXY_IPS),
            "127.0.0.1"
        );
    }

    #[test]
    fn error_message_from_body_prefers_error_or_message_field() {
        assert_eq!(
            error_message_from_body(br#"{"error":"validation failed"}"#, StatusCode::BAD_REQUEST),
            "validation failed"
        );
        assert_eq!(
            error_message_from_body(br#"{"message":"not found"}"#, StatusCode::NOT_FOUND),
            "not found"
        );
        assert_eq!(
            error_message_from_body(b"not-json", StatusCode::TOO_MANY_REQUESTS),
            "Too Many Requests"
        );
    }

    #[tokio::test]
    async fn standardize_error_response_returns_common_shape() {
        let mut res = Response::new(Body::from(r#"{"error":"rate limited"}"#));
        *res.status_mut() = StatusCode::TOO_MANY_REQUESTS;

        let res = standardize_error_response(res, "cid-456").await.unwrap();
        let status = res.status();
        let bytes = to_bytes(res.into_body(), 1024).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(body["status"], 429);
        assert_eq!(body["message"], "rate limited");
        assert_eq!(body["correlationId"], "cid-456");
        assert!(body["timestamp"].as_str().is_some());
    }
}
