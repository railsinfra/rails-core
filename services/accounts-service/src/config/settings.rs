use serde::Deserialize;
use tonic::transport::Endpoint;

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
    fn validate_grpc_url(
        env_key: &str,
        value: String,
    ) -> Result<String, config::ConfigError> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(config::ConfigError::Message(format!(
                "{} must be a non-empty URI",
                env_key
            )));
        }

        Endpoint::from_shared(trimmed.to_string()).map_err(|e| {
            config::ConfigError::Message(format!("invalid {}: {}", env_key, e))
        })?;
        Ok(trimmed.to_string())
    }

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

        let ledger_grpc_url = match std::env::var(LEDGER_GRPC_URL_ENV) {
            Ok(value) => Self::validate_grpc_url(LEDGER_GRPC_URL_ENV, value)?,
            Err(_) => "http://127.0.0.1:50053".to_string(),
        };

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

#[cfg(test)]
mod tests {
    use super::Settings;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    const DATABASE_URL_ENV: &str = "DATABASE_URL";
    const LEDGER_GRPC_URL_ENV: &str = "LEDGER_GRPC_URL";
    const USERS_GRPC_URL_ENV: &str = "USERS_GRPC_URL";
    const AUDIT_GRPC_URL_ENV: &str = "AUDIT_GRPC_URL";

    fn set_required_env() {
        std::env::set_var(
            DATABASE_URL_ENV,
            "postgres://postgres:postgres@localhost:5432/postgres",
        );
        std::env::set_var(USERS_GRPC_URL_ENV, "http://127.0.0.1:50051");
        std::env::set_var(AUDIT_GRPC_URL_ENV, "http://127.0.0.1:50054");
    }

    fn clear_required_env() {
        std::env::remove_var(DATABASE_URL_ENV);
        std::env::remove_var(LEDGER_GRPC_URL_ENV);
        std::env::remove_var(USERS_GRPC_URL_ENV);
        std::env::remove_var(AUDIT_GRPC_URL_ENV);
    }

    #[test]
    fn from_env_accepts_valid_ledger_grpc_url() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_required_env();
        std::env::set_var(LEDGER_GRPC_URL_ENV, "http://ledger-service:50053");

        let settings = Settings::from_env().expect("valid settings");
        assert_eq!(settings.ledger_grpc_url, "http://ledger-service:50053");

        clear_required_env();
    }

    #[test]
    fn from_env_rejects_empty_ledger_grpc_url() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_required_env();
        std::env::set_var(LEDGER_GRPC_URL_ENV, "   ");

        let err = Settings::from_env().expect_err("empty URL should fail");
        assert!(format!("{}", err).contains("LEDGER_GRPC_URL"));

        clear_required_env();
    }

    #[test]
    fn from_env_rejects_malformed_ledger_grpc_url() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_required_env();
        std::env::set_var(LEDGER_GRPC_URL_ENV, "http://[::1");

        let err = Settings::from_env().expect_err("malformed URL should fail");
        assert!(format!("{}", err).contains("invalid LEDGER_GRPC_URL"));

        clear_required_env();
    }
}
