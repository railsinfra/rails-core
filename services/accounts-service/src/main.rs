mod config;
mod errors;
mod grpc;
mod handlers;
mod ledger_grpc;
mod models;
mod repositories;
mod routes;
mod services;
mod users_grpc;
mod utils;

use axum::serve;
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::prelude::*;

use config::Settings;
use routes::create_router;
use crate::ledger_grpc::LedgerGrpc;

use grpc::accounts::AccountsGrpcService;
use grpc::proto::accounts_service_server::AccountsServiceServer;
use tonic::transport::Server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration
    let settings = Settings::from_env()?;

    // Initialize Sentry before tracing to capture all errors
    let _guard = if let Some(dsn) = &settings.sentry_dsn {
        tracing::info!("Initializing Sentry error tracking");
        Some(sentry::init((
            dsn.clone(),
            sentry::ClientOptions {
                release: sentry::release_name!(),
                environment: Some(settings.environment.clone().into()),
                traces_sample_rate: 0.1, // Sample 10% of transactions for MVP
                ..Default::default()
            },
        )))
    } else {
        tracing::info!("Sentry DSN not configured, skipping error tracking");
        None
    };

    // Initialize tracing with Sentry integration
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

    // Create database connection pool
    // For Neon pooler connections, use fewer max_connections (pooler has limits)
    // Increased pool size to handle concurrent gRPC requests
    let pool = PgPoolOptions::new()
        .max_connections(10) // Increased for concurrent gRPC requests
        .acquire_timeout(std::time::Duration::from_secs(60)) // Increased timeout for remote databases
        .idle_timeout(std::time::Duration::from_secs(600)) // 10 minutes
        .max_lifetime(std::time::Duration::from_secs(1800)) // 30 minutes
        .connect(&settings.database_url)
        .await?;

    info!("Connected to database");

    // Run migrations
    let mut migrator = sqlx::migrate!("./migrations_accounts");
    migrator.set_ignore_missing(true);
    if let Err(e) = migrator.run(&pool).await {
        match e {
            sqlx::migrate::MigrateError::VersionMissing(_)
            | sqlx::migrate::MigrateError::VersionMismatch(_) => {
                tracing::warn!(
                    "Skipping SQLx migration failure (shared prod DB / hash drift): {}",
                    e
                );
            }
            _ => return Err(e.into()),
        }
    }

    info!("Database migrations completed");

    // Ledger gRPC client wrapper (lazy connect per call)
    let ledger_grpc = LedgerGrpc::new(settings.ledger_grpc_url.clone());

    // Users gRPC client (ValidateApiKey for holder-based account creation)
    // Lazy connect so startup is not blocked if users service is temporarily unreachable
    let users_grpc = users_grpc::UsersGrpc::connect_lazy(&settings.users_grpc_url)?;
    info!("Users gRPC client configured at {} (connects on first use)", settings.users_grpc_url);

    // Create router with Ledger and Users gRPC
    let app = create_router(pool.clone(), ledger_grpc.clone(), users_grpc.clone());

    let grpc_addr = SocketAddr::from(([0, 0, 0, 0], settings.grpc_port));
    let grpc_service = AccountsGrpcService::new(pool.clone());

    // Background retry loop: post pending transactions to Ledger via gRPC
    let retry_pool = pool.clone();
    let retry_ledger = ledger_grpc.clone();
    tokio::spawn(async move {
        crate::services::transaction_retry::run(retry_pool, retry_ledger).await;
    });

    // Start server
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
