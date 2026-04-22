//! Append-only audit trail ingest service (RAI-14).

pub mod bootstrap;
pub mod config;
pub mod db;
pub mod grpc_server;
pub mod proto;
pub mod routes;
pub mod validate;
