//! Environment configuration.

use anyhow::Context;

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub server_addr: String,
    pub grpc_port: u16,
    pub sentry_dsn: Option<String>,
    pub environment: String,
    pub log_level: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();
        let database_url = std::env::var("AUDIT_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .context("AUDIT_DATABASE_URL or DATABASE_URL must be set")?;
        let server_addr = std::env::var("SERVER_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
        let grpc_port: u16 = std::env::var("GRPC_PORT")
            .unwrap_or_else(|_| "50054".to_string())
            .parse()
            .context("GRPC_PORT must be a valid u16")?;
        let sentry_dsn = std::env::var("SENTRY_DSN").ok().filter(|s| !s.trim().is_empty());
        let environment = std::env::var("ENVIRONMENT").unwrap_or_else(|_| "development".to_string());
        let log_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
        Ok(Self {
            database_url,
            server_addr,
            grpc_port,
            sentry_dsn,
            environment,
            log_level,
        })
    }
}
