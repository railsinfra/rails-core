pub mod business;
pub mod apikey;
pub mod user;
pub mod auth;
pub mod health;
pub mod password_reset;
pub mod beta;

mod rate_limit;

use axum::{Router, routing::{post, get}};
use axum::body::Body;
use axum::http::Request;
use axum::middleware::{from_fn, Next};
use axum::response::Response;
use crate::db::Db;
use crate::grpc::GrpcClients;
use crate::error::AppError;
use crate::email::EmailService;
use rate_limit::{extract_client_key, RateLimitConfig, RateLimiter};
use uuid::Uuid;
use std::sync::OnceLock;
use std::time::Duration;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub grpc: GrpcClients,
    pub email: Option<EmailService>,
}

pub fn register_routes(db: Db, grpc: GrpcClients, email: Option<EmailService>) -> Router {
    let state = AppState { db, grpc, email };
    let public = Router::new()
        .route("/health", get(health::health_check))
        .route("/api/v1/business/register", post(business::register_business))
        .route("/api/v1/auth/refresh", post(auth::refresh_token))
        .route("/api/v1/auth/revoke", post(auth::revoke_token));

    let auth_limited = Router::new()
        .route("/api/v1/auth/login", post(auth::login))
        .route("/api/v1/auth/password-reset/request", post(password_reset::request_password_reset))
        .route("/api/v1/auth/password-reset/reset", post(password_reset::reset_password))
        .route("/api/v1/beta/apply", post(beta::apply_for_beta))
        .layer(from_fn(auth_rate_limit_middleware));

    // Web/dashboard: JWT or API key (me, api-keys)
    let protected = Router::new()
        .route("/api/v1/api-keys", post(apikey::create_api_key))
        .route("/api/v1/api-keys", get(apikey::list_api_keys))
        .route("/api/v1/api-keys/:api_key_id/revoke", post(apikey::revoke_api_key))
        .route("/api/v1/me", get(user::me));

    public
        .merge(auth_limited)
        .merge(protected)
        .layer(from_fn(correlation_id_middleware))
        .layer(from_fn(internal_caller_middleware))
        .with_state(state)
}

async fn internal_caller_middleware(req: Request<Body>, next: Next) -> Result<Response, AppError> {
    let path = req.uri().path();

    let is_sensitive = path == "/api/v1/auth/login" || path == "/api/v1/business/register";
    if !is_sensitive {
        return Ok(next.run(req).await);
    }

    const INTERNAL_SERVICE_TOKEN_ALLOWLIST_ENV: &str = "INTERNAL_SERVICE_TOKEN_ALLOWLIST";
    let allowlist_raw = std::env::var(INTERNAL_SERVICE_TOKEN_ALLOWLIST_ENV).unwrap_or_default();
    if allowlist_raw.trim().is_empty() {
        return Ok(next.run(req).await);
    }

    let provided_token = req
        .headers()
        .get("x-internal-service-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .trim();

    let allowed_tokens: Vec<String> = allowlist_raw
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    if provided_token.is_empty() || !allowed_tokens.iter().any(|t| t == provided_token) {
        return Err(AppError::UnrecognizedSource);
    }

    Ok(next.run(req).await)
}

pub(crate) fn log_correlation_request_finished(
    correlation_id: &str,
    method: &str,
    path: &str,
    status: u16,
    duration_ms: u128,
) {
    let outcome = if status >= 400 { "failed" } else { "success" };
    if status >= 500 {
        tracing::error!(
            correlation_id = %correlation_id,
            %method,
            %path,
            status = status,
            duration_ms = duration_ms as u64,
            outcome = outcome,
            "finish"
        );
    } else if status >= 400 {
        tracing::warn!(
            correlation_id = %correlation_id,
            %method,
            %path,
            status = status,
            duration_ms = duration_ms as u64,
            outcome = outcome,
            "finish"
        );
    } else {
        tracing::info!(
            correlation_id = %correlation_id,
            %method,
            %path,
            status = status,
            duration_ms = duration_ms as u64,
            outcome = outcome,
            "finish"
        );
    }
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

    let should_set_header = existing.is_none();
    let correlation_id = existing.unwrap_or_else(|| Uuid::new_v4().to_string());

    let mut req = req;
    if should_set_header {
        req.headers_mut().insert(
            "x-correlation-id",
            correlation_id
                .parse()
                .map_err(|_| AppError::Internal)?,
        );
    }

    let start = std::time::Instant::now();
    tracing::info!(correlation_id = %correlation_id, %method, %path, "start");

    let mut res = next.run(req).await;
    res.headers_mut().insert(
        "x-correlation-id",
        correlation_id
            .parse()
            .map_err(|_| AppError::Internal)?,
    );
    let status = res.status().as_u16();
    let duration_ms = start.elapsed().as_millis();
    log_correlation_request_finished(&correlation_id, &method, &path, status, duration_ms);

    Ok(res)
}

static AUTH_RATE_LIMITER: OnceLock<RateLimiter> = OnceLock::new();

fn auth_rate_limit_config() -> RateLimitConfig {
    const USERS_AUTH_RATE_LIMIT_WINDOW_SECONDS_ENV: &str = "USERS_AUTH_RATE_LIMIT_WINDOW_SECONDS";
    const USERS_AUTH_RATE_LIMIT_MAX_ENV: &str = "USERS_AUTH_RATE_LIMIT_MAX";
    let window_seconds = std::env::var(USERS_AUTH_RATE_LIMIT_WINDOW_SECONDS_ENV)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(60);
    let max_requests = std::env::var(USERS_AUTH_RATE_LIMIT_MAX_ENV)
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(10);
    RateLimitConfig {
        window: Duration::from_secs(window_seconds),
        max: max_requests,
    }
}

fn auth_rate_limiter() -> &'static RateLimiter {
    AUTH_RATE_LIMITER.get_or_init(|| RateLimiter::new(auth_rate_limit_config()))
}

async fn auth_rate_limit_middleware(req: Request<Body>, next: Next) -> Result<Response, AppError> {
    let client_key = extract_client_key(&req, "USERS_TRUSTED_PROXY_IPS");
    if !auth_rate_limiter().allow(&client_key) {
        return Err(AppError::TooManyRequests);
    }
    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::global_test_lock;
    use axum::extract::ConnectInfo;
    use axum::http::Request;
    use std::net::SocketAddr;

    const USERS_TRUSTED_PROXY_IPS: &str = "USERS_TRUSTED_PROXY_IPS";

    fn reset_auth_rate_limiter() {
        if let Some(limiter) = AUTH_RATE_LIMITER.get() {
            limiter.reset();
        }
    }

    #[test]
    fn auth_rate_limit_blocks_after_max() {
        reset_auth_rate_limiter();
        let config = auth_rate_limit_config();
        let client = "127.0.0.1";
        for _ in 0..config.max {
            assert!(auth_rate_limiter().allow(client));
        }
        assert!(!auth_rate_limiter().allow(client));
    }

    #[test]
    fn extract_client_key_uses_peer_ip_when_untrusted() {
        let _lock = global_test_lock();
        std::env::remove_var(USERS_TRUSTED_PROXY_IPS);
        let mut req = Request::builder()
            .uri("/api/v1/auth/login")
            .header("x-forwarded-for", "203.0.113.10")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 8080))));
        assert_eq!(extract_client_key(&req, USERS_TRUSTED_PROXY_IPS), "127.0.0.1");
    }

    #[test]
    fn extract_client_key_uses_forwarded_ip_when_trusted() {
        let _lock = global_test_lock();
        std::env::set_var(USERS_TRUSTED_PROXY_IPS, "127.0.0.1");
        let mut req = Request::builder()
            .uri("/api/v1/auth/login")
            .header("x-forwarded-for", "203.0.113.10, 127.0.0.1")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 8080))));
        assert_eq!(extract_client_key(&req, USERS_TRUSTED_PROXY_IPS), "203.0.113.10");
    }
    #[test]
    fn log_correlation_request_finished_covers_status_branches() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_test_writer()
            .try_init();
        log_correlation_request_finished("cid", "GET", "/api/x", 200, 1);
        log_correlation_request_finished("cid", "GET", "/api/x", 404, 2);
        log_correlation_request_finished("cid", "GET", "/api/x", 500, 3);
    }

    #[test]
    fn extract_client_key_ignores_forwarded_ip_when_last_hop_untrusted() {
        let _lock = global_test_lock();
        std::env::set_var(USERS_TRUSTED_PROXY_IPS, "127.0.0.1");
        let mut req = Request::builder()
            .uri("/api/v1/auth/login")
            .header("x-forwarded-for", "203.0.113.10, 198.51.100.5")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 8080))));
        assert_eq!(extract_client_key(&req, USERS_TRUSTED_PROXY_IPS), "127.0.0.1");
    }
}
