//! Main entry point for the Users microservice

mod config;
mod db;
mod error;
mod routes;
mod auth;
mod grpc;
mod grpc_server;
mod email;

use tracing_subscriber::prelude::*;
use crate::routes::register_routes;
use crate::email::EmailService;
use axum::serve;
use tokio::net::TcpListener;
use tonic::transport::Server;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    eprintln!("[STARTUP] Users service starting...");
    
    dotenvy::dotenv().ok();
    
    eprintln!("[STARTUP] Loading configuration...");
    let config = match config::load() {
        Ok(c) => {
            eprintln!("[STARTUP] Configuration loaded successfully");
            c
        }
        Err(e) => {
            eprintln!("[FATAL] Failed to load configuration: {}", e);
            return Err(e);
        }
    };
    
    // Initialize Sentry before tracing to capture all errors
    let _guard = if let Some(dsn) = &config.sentry_dsn {
        tracing::info!("Initializing Sentry error tracking");
        Some(sentry::init((
            dsn.clone(),
            sentry::ClientOptions {
                release: sentry::release_name!(),
                environment: Some(config.environment.clone().into()),
                traces_sample_rate: 0.1, // Sample 10% of transactions for MVP
                ..Default::default()
            },
        )))
    } else {
        tracing::info!("Sentry DSN not configured, skipping error tracking");
        None
    };
    
    // Initialize tracing with Sentry integration
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    
    if config.sentry_dsn.is_some() {
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer())
            .with(sentry_tracing::layer())
            .with(filter)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .init();
    }
    tracing::info!("Loaded configuration");
    tracing::info!("  DATABASE_URL: configured (value omitted; see repository .env)");
    tracing::info!("  SERVER_ADDR: {}", config.server_addr);
    tracing::info!("  ACCOUNTS_GRPC_URL: {}", config.accounts_grpc_url);
    
    tracing::info!("Connecting to database...");
    eprintln!("[STARTUP] Connecting to database...");
    let db = match db::init(&config.database_url).await {
        Ok(pool) => {
            eprintln!("[STARTUP] Database connected successfully");
            tracing::info!("Database connection established");
            pool
        }
        Err(e) => {
            let msg = format!("Failed to connect to database: {}. Make sure PostgreSQL is running and DATABASE_URL is correct.", e);
            eprintln!("[FATAL] {}", msg);
            return Err(anyhow::anyhow!(msg));
        }
    };
    
    // Run migrations
    tracing::info!("Running database migrations...");
    let mut migrator = sqlx::migrate!("./migrations");
    migrator.set_ignore_missing(true);
    if let Err(e) = migrator.run(&db).await {
        match e {
            sqlx::migrate::MigrateError::VersionMissing(_)
            | sqlx::migrate::MigrateError::VersionMismatch(_) => {
                tracing::warn!(
                    "Skipping SQLx migration failure (shared prod DB / hash drift): {}",
                    e
                );
            }
            _ => {
                tracing::error!("Migration failed: {}", e);
                return Err(anyhow::anyhow!("Migration failed: {}", e));
            }
        }
    }
    tracing::info!("Database migrations completed");
    
    tracing::info!("Initializing gRPC clients...");
    let grpc = grpc::init(&config).await?;
    tracing::info!("gRPC clients initialized");
    
    // Initialize email service
    let email = if config.resend_api_key.is_some() {
        Some(EmailService::new(&config))
    } else {
        tracing::warn!("Resend API key not configured. Password reset emails will not be sent.");
        None
    };
    
    let app = register_routes(db.clone(), grpc.clone(), email);
    let addr: SocketAddr = config.server_addr.parse()
        .map_err(|e| anyhow::anyhow!("Failed to parse SERVER_ADDR '{}': {}", config.server_addr, e))?;
    let grpc_addr: SocketAddr = ([0, 0, 0, 0], config.grpc_port).into();

    tracing::info!("Binding HTTP to {}...", addr);
    let listener = TcpListener::bind(&addr).await
        .map_err(|e| anyhow::anyhow!("Failed to bind to {}: {}", addr, e))?;

    let grpc_svc = grpc_server::UsersGrpcService::new(db.clone()).into_server();

    let http_task = async move {
        serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
            .await
            .map_err(|e| anyhow::anyhow!("HTTP server error: {}", e))
    };

    let grpc_task = async move {
        Server::builder()
            .add_service(grpc_svc)
            .serve(grpc_addr)
            .await
            .map_err(|e| anyhow::anyhow!("gRPC server error: {}", e))
    };

    tracing::info!("🚀 Users service started successfully");
    tracing::info!("  HTTP: http://{}", addr);
    tracing::info!("  gRPC: {}", grpc_addr);
    tracing::info!("Available endpoints:");
    tracing::info!("  GET  /health");
    tracing::info!("  POST /api/v1/business/register");
    tracing::info!("  POST /api/v1/users (SDK-only: X-API-Key + X-Environment)");
    tracing::info!("  GET  /api/v1/users (SDK-only: X-API-Key + X-Environment)");
    tracing::info!("  POST /api/v1/auth/login");
    tracing::info!("  POST /api/v1/auth/refresh");
    tracing::info!("  POST /api/v1/auth/revoke");
    tracing::info!("  POST /api/v1/beta/apply");
    tracing::info!("  POST /api/v1/auth/password-reset/request");
    tracing::info!("  POST /api/v1/auth/password-reset/reset");

    tokio::try_join!(http_task, grpc_task)?;
    Ok(())
}
