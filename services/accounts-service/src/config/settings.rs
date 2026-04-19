use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    pub database_url: String,
    pub port: u16,
    pub grpc_port: u16,
    pub ledger_grpc_url: String,
    pub users_grpc_url: String,
    #[allow(dead_code)]
    pub host: String,
    pub log_level: String,
    pub sentry_dsn: Option<String>,
    pub environment: String,
}

impl Settings {
    pub fn from_env() -> Result<Self, config::ConfigError> {
        dotenv::dotenv().ok();

        let database_url = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must be set");

        let port = std::env::var("PORT")
            .unwrap_or_else(|_| "8081".to_string())
            .parse()
            .unwrap_or(8081);

        let grpc_port = std::env::var("GRPC_PORT")
            .unwrap_or_else(|_| "9090".to_string())
            .parse()
            .unwrap_or(9090);

        let ledger_grpc_url = std::env::var("LEDGER_GRPC_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:50053".to_string());

        let users_grpc_url = std::env::var("USERS_GRPC_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:50051".to_string());

        let host = std::env::var("HOST")
            .unwrap_or_else(|_| "0.0.0.0".to_string());

        let log_level = std::env::var("RUST_LOG")
            .unwrap_or_else(|_| "info".to_string());

        let sentry_dsn = std::env::var("SENTRY_DSN").ok();
        let environment = std::env::var("ENVIRONMENT")
            .unwrap_or_else(|_| "development".to_string());

        Ok(Settings {
            database_url,
            port,
            grpc_port,
            ledger_grpc_url,
            users_grpc_url,
            host,
            log_level,
            sentry_dsn,
            environment,
        })
    }
}
