//! Startup: telemetry, database, HTTP + gRPC.

use axum::serve;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tonic::transport::Server;
use tracing_subscriber::prelude::*;

use crate::config::Config;
use crate::grpc_server::AuditGrpcService;
use crate::routes::router;

pub async fn run() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let config = Config::from_env()?;

    let _guard = if let Some(dsn) = &config.sentry_dsn {
        tracing::info!("Initializing Sentry error tracking");
        Some(sentry::init((
            dsn.clone(),
            sentry::ClientOptions {
                release: sentry::release_name!(),
                environment: Some(config.environment.clone().into()),
                traces_sample_rate: 0.1,
                ..Default::default()
            },
        )))
    } else {
        tracing::info!("Sentry DSN not configured, skipping error tracking");
        None
    };

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.log_level));

    if config.sentry_dsn.is_some() {
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer())
            .with(sentry_tracing::layer())
            .with(filter)
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }

    tracing::info!("Connecting audit database...");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await?;

    let mut migrator = sqlx::migrate!("./migrations");
    migrator.set_ignore_missing(true);
    if let Err(e) = migrator.run(&pool).await {
        match e {
            sqlx::migrate::MigrateError::VersionMissing(_)
            | sqlx::migrate::MigrateError::VersionMismatch(_) => {
                tracing::warn!("Skipping migration drift: {}", e);
            }
            _ => return Err(e.into()),
        }
    }

    let addr: SocketAddr = config
        .server_addr
        .parse()
        .map_err(|e| anyhow::anyhow!("SERVER_ADDR: {}", e))?;
    let grpc_addr: SocketAddr = ([0, 0, 0, 0], config.grpc_port).into();

    let listener = TcpListener::bind(addr).await?;
    let app = router();
    let grpc_svc = AuditGrpcService::new(pool.clone()).into_server();

    let http_task = async move {
        serve(listener, app.into_make_service())
            .await
            .map_err(|e| anyhow::anyhow!("HTTP: {}", e))
    };

    let grpc_task = async move {
        Server::builder()
            .add_service(grpc_svc)
            .serve(grpc_addr)
            .await
            .map_err(|e| anyhow::anyhow!("gRPC: {}", e))
    };

    tracing::info!("audit-service HTTP={} gRPC={}", addr, grpc_addr);
    tokio::try_join!(http_task, grpc_task)?;
    Ok(())
}
