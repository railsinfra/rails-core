use axum::{
    body::Body,
    http::Request,
    middleware::{from_fn, Next},
    response::Response,
    routing::{get, post},
    Router,
};
use sqlx::PgPool;
use std::sync::OnceLock;
use std::time::Duration;
use tonic::transport::Channel;
use uuid::Uuid;

use crate::grpc::audit_proto::audit_service_client::AuditServiceClient;

use crate::handlers::{
    accounts::*,
    transactions::{get_transaction, list_account_transactions, list_transactions},
    health::{health_check, service_root},
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
        method,
        path
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
        .route("/accounts/:id", get(get_account).patch(update_account_status).delete(close_account))
        .route("/accounts/:account_id/transactions", get(list_account_transactions))
        .route("/transactions", get(list_transactions))
        .route("/transactions/:id", get(get_transaction));

    let money_mutations = Router::<AppState>::new()
        .route("/accounts/:id/deposit", post(deposit))
        .route("/accounts/:id/withdraw", post(withdraw))
        .route("/accounts/:id/transfer", post(transfer))
        .layer(from_fn(money_rate_limit_middleware));

    standard_routes.merge(money_mutations)
}

async fn correlation_id_middleware(
    req: Request<Body>,
    next: Next,
) -> Result<Response, AppError> {
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

    let mut res = next.run(req).await;
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

static MONEY_RATE_LIMITER: OnceLock<RateLimiter> = OnceLock::new();

fn money_rate_limit_config() -> RateLimitConfig {
    let window_seconds = std::env::var("ACCOUNTS_MONEY_RATE_LIMIT_WINDOW_SECONDS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(60);
    let max_requests = std::env::var("ACCOUNTS_MONEY_RATE_LIMIT_MAX")
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
    use axum::http::Request;
    use axum::extract::ConnectInfo;
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
        std::env::remove_var("ACCOUNTS_TRUSTED_PROXY_IPS");
        let mut req = Request::builder()
            .uri("/api/v1/accounts/1/deposit")
            .header("x-forwarded-for", "203.0.113.10")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 8080))));
        assert_eq!(extract_client_key(&req, "ACCOUNTS_TRUSTED_PROXY_IPS"), "127.0.0.1");
    }

    #[test]
    fn extract_client_key_uses_forwarded_ip_when_trusted() {
        let _lock = test_env_lock();
        std::env::set_var("ACCOUNTS_TRUSTED_PROXY_IPS", "127.0.0.1");
        let mut req = Request::builder()
            .uri("/api/v1/accounts/1/deposit")
            .header("x-forwarded-for", "203.0.113.10, 127.0.0.1")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 8080))));
        assert_eq!(extract_client_key(&req, "ACCOUNTS_TRUSTED_PROXY_IPS"), "203.0.113.10");
    }

    #[test]
    fn extract_client_key_ignores_forwarded_ip_when_last_hop_untrusted() {
        let _lock = test_env_lock();
        std::env::set_var("ACCOUNTS_TRUSTED_PROXY_IPS", "127.0.0.1");
        let mut req = Request::builder()
            .uri("/api/v1/accounts/1/deposit")
            .header("x-forwarded-for", "203.0.113.10, 198.51.100.5")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 8080))));
        assert_eq!(extract_client_key(&req, "ACCOUNTS_TRUSTED_PROXY_IPS"), "127.0.0.1");
    }
}
