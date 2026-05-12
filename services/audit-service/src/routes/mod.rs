//! HTTP routes for service health and SDK-public audit ingest.

mod audit_events;
mod health;

use axum::{routing::get, Router};
use sqlx::PgPool;

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
        .with_state(state)
}
