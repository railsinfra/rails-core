use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    pub database_url: String,
    pub port: u16,
    pub grpc_port: u16,
    pub ledger_grpc_url: String,
    pub users_grpc_url: String,
    pub audit_grpc_url: String,
    #[allow(dead_code)]
    pub host: String,
    pub log_level: String,
    pub sentry_dsn: Option<String>,
    pub environment: String,
}

impl Settings {
    pub fn from_env() -> Result<Self, config::ConfigError> {
        const DATABASE_URL_ENV: &str = "DATABASE_URL";
        const PORT_ENV: &str = "PORT";
        const GRPC_PORT_ENV: &str = "GRPC_PORT";
        const LEDGER_GRPC_URL_ENV: &str = "LEDGER_GRPC_URL";
        const USERS_GRPC_URL_ENV: &str = "USERS_GRPC_URL";
        const AUDIT_GRPC_URL_ENV: &str = "AUDIT_GRPC_URL";
        const HOST_ENV: &str = "HOST";
        const RUST_LOG_ENV: &str = "RUST_LOG";
        const SENTRY_DSN_ENV: &str = "SENTRY_DSN";
        const ENVIRONMENT_ENV: &str = "ENVIRONMENT";
        dotenv::dotenv().ok();

        let database_url = std::env::var(DATABASE_URL_ENV).expect("DATABASE_URL must be set");

        let port = std::env::var(PORT_ENV)
            .unwrap_or_else(|_| "8081".to_string())
            .parse()
            .unwrap_or(8081);

        let grpc_port = std::env::var(GRPC_PORT_ENV)
            .unwrap_or_else(|_| "9090".to_string())
            .parse()
            .unwrap_or(9090);

        let ledger_grpc_url = std::env::var(LEDGER_GRPC_URL_ENV)
            .unwrap_or_else(|_| "http://127.0.0.1:9090".to_string());

        let users_grpc_url = std::env::var(USERS_GRPC_URL_ENV)
            .unwrap_or_else(|_| "http://127.0.0.1:50051".to_string());

        let audit_grpc_url = std::env::var(AUDIT_GRPC_URL_ENV)
            .unwrap_or_else(|_| "http://127.0.0.1:50054".to_string());

        let host = std::env::var(HOST_ENV).unwrap_or_else(|_| "0.0.0.0".to_string());

        let log_level = std::env::var(RUST_LOG_ENV).unwrap_or_else(|_| "info".to_string());

        let sentry_dsn = std::env::var(SENTRY_DSN_ENV).ok();
        let environment =
            std::env::var(ENVIRONMENT_ENV).unwrap_or_else(|_| "development".to_string());

        Ok(Settings {
            database_url,
            port,
            grpc_port,
            ledger_grpc_url,
            users_grpc_url,
            audit_grpc_url,
            host,
            log_level,
            sentry_dsn,
            environment,
        })
    }
}
