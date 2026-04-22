//! Shared test synchronization (env vars, etc.).
//!
//! Lives in the library (not under `#[cfg(test)]`) so integration tests in `tests/` link the same mutex as unit tests.

use std::net::SocketAddr;
use std::sync::{Mutex, OnceLock};

use axum::extract::ConnectInfo;

/// Stable loopback peer for handler unit tests that require [`ConnectInfo`].
pub fn test_connect_info() -> ConnectInfo<SocketAddr> {
    ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 42_042)))
}

pub fn global_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    match LOCK.get_or_init(|| Mutex::new(())).lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}
