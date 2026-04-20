//! Users microservice: HTTP API, gRPC server, and shared modules.
//! The binary entry point delegates to [`bootstrap::run`].

pub mod auth;
pub mod bootstrap;
pub mod config;
pub mod db;
pub mod email;
pub mod error;
pub mod grpc;
pub mod grpc_server;
pub mod routes;

/// Test-only helpers (e.g. global env lock). Public so `tests/*.rs` integration binaries can use the same lock as `#[cfg(test)]` unit tests.
#[doc(hidden)]
pub mod test_support;
