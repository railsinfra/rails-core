//! accounts-api: HTTP + gRPC server and supporting modules.
//! The binary entrypoint (`src/main.rs`) delegates to [`run`].

pub mod config;
pub mod errors;
pub mod grpc;
pub mod handlers;
pub mod ledger_grpc;
pub mod models;
pub mod repositories;
pub mod routes;
pub mod services;
pub mod users_grpc;
pub mod utils;

use axum::serve;
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::prelude::*;

use config::Settings;
use routes::create_router;
use ledger_grpc::LedgerGrpc;

use grpc::accounts::AccountsGrpcService;
use grpc::proto::accounts_service_server::AccountsServiceServer;
use sqlx::PgPool;
use tonic::transport::Server;

pub(crate) fn resolve_migrate_run_result(
    result: Result<(), sqlx::migrate::MigrateError>,
) -> Result<(), Box<dyn std::error::Error>> {
    match result {
        Ok(()) => Ok(()),
        Err(e) => match e {
            sqlx::migrate::MigrateError::VersionMissing(_)
            | sqlx::migrate::MigrateError::VersionMismatch(_) => {
                tracing::warn!(
                    "Skipping SQLx migration failure (shared prod DB / hash drift): {}",
                    e
                );
                Ok(())
            }
            _ => Err(e.into()),
        },
    }
}

pub(crate) async fn run_accounts_migrations(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let mut migrator = sqlx::migrate!("./migrations_accounts");
    migrator.set_ignore_missing(true);
    resolve_migrate_run_result(migrator.run(pool).await)?;
    Ok(())
}

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Settings::from_env()?;

    let _guard = if let Some(dsn) = &settings.sentry_dsn {
        tracing::info!("Initializing Sentry error tracking");
        Some(sentry::init((
            dsn.clone(),
            sentry::ClientOptions {
                release: sentry::release_name!(),
                environment: Some(settings.environment.clone().into()),
                traces_sample_rate: 0.1,
                ..Default::default()
            },
        )))
    } else {
        tracing::info!("Sentry DSN not configured, skipping error tracking");
        None
    };

    if settings.sentry_dsn.is_some() {
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().with_filter(tracing_subscriber::EnvFilter::new(&settings.log_level)))
            .with(sentry_tracing::layer())
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(&settings.log_level)
            .init();
    }

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(std::time::Duration::from_secs(60))
        .idle_timeout(std::time::Duration::from_secs(600))
        .max_lifetime(std::time::Duration::from_secs(1800))
        .connect(&settings.database_url)
        .await?;

    info!("Connected to database");

    run_accounts_migrations(&pool).await?;

    info!("Database migrations completed");

    let ledger_grpc = LedgerGrpc::new(settings.ledger_grpc_url.clone());

    let users_grpc = users_grpc::UsersGrpc::connect_lazy(&settings.users_grpc_url)?;
    info!(
        "Users gRPC client configured at {} (connects on first use)",
        settings.users_grpc_url
    );

    let app = create_router(pool.clone(), ledger_grpc.clone(), users_grpc.clone());

    let grpc_addr = SocketAddr::from(([0, 0, 0, 0], settings.grpc_port));
    let grpc_service = AccountsGrpcService::new(pool.clone());

    let retry_pool = pool.clone();
    let retry_ledger = ledger_grpc.clone();
    tokio::spawn(async move {
        crate::services::transaction_retry::run(retry_pool, retry_ledger).await;
    });

    let addr = SocketAddr::from(([0, 0, 0, 0], settings.port));
    info!("Server starting on {}", addr);
    info!("gRPC server starting on {}", grpc_addr);

    let listener = TcpListener::bind(addr).await?;

    let http_task = async move {
        serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
            .await
            .map_err(|e| anyhow::anyhow!("HTTP server error: {}", e))
    };

    let grpc_task = async move {
        Server::builder()
            .add_service(AccountsServiceServer::new(grpc_service))
            .serve(grpc_addr)
            .await
            .map_err(|e| anyhow::anyhow!("gRPC server error: {}", e))
    };

    tokio::try_join!(http_task, grpc_task)?;

    Ok(())
}

#[cfg(test)]
mod migrate_resolution_tests {
    use super::resolve_migrate_run_result;
    use sqlx::migrate::MigrateError;

    #[test]
    fn resolve_migrate_ok() {
        assert!(resolve_migrate_run_result(Ok(())).is_ok());
    }

    #[test]
    fn resolve_migrate_version_missing_maps_to_ok() {
        assert!(resolve_migrate_run_result(Err(MigrateError::VersionMissing(1))).is_ok());
    }

    #[test]
    fn resolve_migrate_version_mismatch_maps_to_ok() {
        assert!(resolve_migrate_run_result(Err(MigrateError::VersionMismatch(2))).is_ok());
    }

    #[test]
    fn resolve_migrate_other_errors_propagate() {
        assert!(resolve_migrate_run_result(Err(MigrateError::VersionNotPresent(3))).is_err());
        let exec_err = MigrateError::Execute(sqlx::Error::RowNotFound);
        assert!(resolve_migrate_run_result(Err(exec_err)).is_err());
    }
}

#[cfg(test)]
mod run_accounts_migrations_smoke {
    use super::run_accounts_migrations;
    use sqlx::postgres::PgPoolOptions;
    use testcontainers::runners::AsyncRunner;
    use testcontainers_modules::postgres::Postgres;

    #[tokio::test]
    async fn run_accounts_migrations_applies_and_is_idempotent() {
        let container = Postgres::default()
            .start()
            .await
            .expect("start postgres testcontainer");
        let host = container.get_host().await.expect("container host");
        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("container port");
        let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");
        let pool = PgPoolOptions::new()
            .max_connections(3)
            .connect(&url)
            .await
            .expect("connect to test postgres");
        sqlx::query(r#"CREATE EXTENSION IF NOT EXISTS "pgcrypto""#)
            .execute(&pool)
            .await
            .expect("create pgcrypto extension for gen_random_uuid");
        run_accounts_migrations(&pool)
            .await
            .expect("first migration run");
        run_accounts_migrations(&pool)
            .await
            .expect("second migration run should succeed");
    }
}
