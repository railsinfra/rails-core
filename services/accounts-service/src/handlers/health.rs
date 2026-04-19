use axum::{extract::State, http::StatusCode, Json};
use serde_json::json;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::routes::api::AppState;

fn try_connect_ledger_grpc(endpoint: &str, timeout: Duration) -> bool {
    // Expect an HTTP/HTTPS-style endpoint (tonic Endpoint::from_shared),
    // e.g. "http://127.0.0.1:50053"
    let without_scheme = endpoint
        .strip_prefix("http://")
        .or_else(|| endpoint.strip_prefix("https://"))
        .unwrap_or(endpoint);

    let host_port = without_scheme
        .split('/')
        .next()
        .unwrap_or(without_scheme);

    // Resolve and attempt a TCP connect with timeout (best-effort).
    if let Ok(mut addrs) = host_port.to_socket_addrs() {
        if let Some(addr) = addrs.next() {
            return TcpStream::connect_timeout(&addr, timeout).is_ok();
        }
    }

    false
}

pub async fn health_check(
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let endpoint = state.ledger_grpc.endpoint().to_string();
    let timeout = state.ledger_grpc.timeout();
    let grpc_ok = try_connect_ledger_grpc(&endpoint, timeout);

    (
        StatusCode::OK,
        Json(json!({
            "status": "healthy",
            "service": "accounts-api",
            "ledger_grpc": {
                "endpoint": endpoint,
                "timeout_ms": timeout.as_millis(),
                "status": if grpc_ok { "ok" } else { "down" }
            }
        })),
    )
}
