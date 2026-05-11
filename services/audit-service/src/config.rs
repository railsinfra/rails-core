//! Environment configuration.

use anyhow::Context;

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub server_addr: String,
    pub grpc_port: u16,
    pub users_grpc_url: String,
    pub internal_token: Option<String>,
    pub sentry_dsn: Option<String>,
    pub environment: String,
    pub log_level: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        const AUDIT_DATABASE_URL_ENV: &str = "AUDIT_DATABASE_URL";
        const DATABASE_URL_ENV: &str = "DATABASE_URL";
        const SERVER_ADDR_ENV: &str = "SERVER_ADDR";
        const GRPC_PORT_ENV: &str = "GRPC_PORT";
        const USERS_GRPC_URL_ENV: &str = "USERS_GRPC_URL";
        const AUDIT_SERVICE_INTERNAL_TOKEN_ENV: &str = "AUDIT_SERVICE_INTERNAL_TOKEN";
        const SENTRY_DSN_ENV: &str = "SENTRY_DSN";
        const ENVIRONMENT_ENV: &str = "ENVIRONMENT";
        const RUST_LOG_ENV: &str = "RUST_LOG";
        dotenvy::dotenv().ok();
        let database_url = std::env::var(AUDIT_DATABASE_URL_ENV)
            .or_else(|_| std::env::var(DATABASE_URL_ENV))
            .context("AUDIT_DATABASE_URL or DATABASE_URL must be set")?;
        let server_addr =
            std::env::var(SERVER_ADDR_ENV).unwrap_or_else(|_| "0.0.0.0:8080".to_string());
        let grpc_port: u16 = std::env::var(GRPC_PORT_ENV)
            .unwrap_or_else(|_| "50054".to_string())
            .parse()
            .context("GRPC_PORT must be a valid u16")?;
        let users_grpc_url = std::env::var(USERS_GRPC_URL_ENV)
            .unwrap_or_else(|_| "http://127.0.0.1:50051".to_string());
        let internal_token = std::env::var(AUDIT_SERVICE_INTERNAL_TOKEN_ENV)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let sentry_dsn = std::env::var(SENTRY_DSN_ENV)
            .ok()
            .filter(|s| !s.trim().is_empty());
        let environment =
            std::env::var(ENVIRONMENT_ENV).unwrap_or_else(|_| "development".to_string());
        let log_level = std::env::var(RUST_LOG_ENV).unwrap_or_else(|_| "info".to_string());
        Ok(Self {
            database_url,
            server_addr,
            grpc_port,
            users_grpc_url,
            internal_token,
            sentry_dsn,
            environment,
            log_level,
        })
    }
}
