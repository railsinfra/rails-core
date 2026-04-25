#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub server_addr: String,
    pub grpc_port: u16,
    pub accounts_grpc_url: String,
    pub audit_grpc_url: String,
    pub sentry_dsn: Option<String>,
    pub environment: String,
    pub resend_api_key: Option<String>,
    pub resend_from_email: String,
    pub resend_from_name: String,
    pub resend_base_url: String,
    pub resend_beta_notification_email: String,
    pub frontend_base_url: String,
}

pub(crate) fn strip_jdbc_database_url_prefix(url: &str) -> &str {
    url.strip_prefix("jdbc:").unwrap_or(url)
}

pub(crate) fn compose_server_addr(
    host: &str,
    port: Option<u16>,
    server_addr_env: Option<&str>,
) -> String {
    if let Some(port) = port {
        format!("{}:{}", host, port)
    } else {
        server_addr_env
            .map(String::from)
            .unwrap_or_else(|| "0.0.0.0:8080".to_string())
    }
}

pub fn load() -> Result<Config, anyhow::Error> {
    const DATABASE_URL_ENV: &str = "DATABASE_URL";
    const HOST_ENV: &str = "HOST";
    const PORT_ENV: &str = "PORT";
    const SERVER_ADDR_ENV: &str = "SERVER_ADDR";
    const GRPC_PORT_ENV: &str = "GRPC_PORT";
    const ACCOUNTS_GRPC_URL_ENV: &str = "ACCOUNTS_GRPC_URL";
    const AUDIT_GRPC_URL_ENV: &str = "AUDIT_GRPC_URL";
    const SENTRY_DSN_ENV: &str = "SENTRY_DSN";
    const ENVIRONMENT_ENV: &str = "ENVIRONMENT";
    const RESEND_API_KEY_ENV: &str = "RESEND_API_KEY";
    const RESEND_FROM_EMAIL_ENV: &str = "RESEND_FROM_EMAIL";
    const RESEND_FROM_NAME_ENV: &str = "RESEND_FROM_NAME";
    const RESEND_BASE_URL_ENV: &str = "RESEND_BASE_URL";
    const RESEND_BETA_NOTIFICATION_EMAIL_ENV: &str = "RESEND_BETA_NOTIFICATION_EMAIL";
    const FRONTEND_BASE_URL_ENV: &str = "FRONTEND_BASE_URL";

    let database_url_raw = std::env::var(DATABASE_URL_ENV)
        .map_err(|_| anyhow::anyhow!(
            "DATABASE_URL environment variable is required. \
            Set it to your PostgreSQL connection string (e.g., Supabase, Neon, or local PostgreSQL). \
            Example: postgresql://user:password@host:5432/database"
        ))?;
    let database_url = strip_jdbc_database_url_prefix(&database_url_raw).to_string();

    let host = std::env::var(HOST_ENV).unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = std::env::var(PORT_ENV)
        .ok()
        .and_then(|p| p.parse::<u16>().ok());
    let server_addr_env = std::env::var(SERVER_ADDR_ENV).ok();
    let server_addr = compose_server_addr(&host, port, server_addr_env.as_deref());

    let grpc_port = std::env::var(GRPC_PORT_ENV)
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(50051);

    let accounts_grpc_url = std::env::var(ACCOUNTS_GRPC_URL_ENV)
        .unwrap_or_else(|_| "http://localhost:50052".to_string());

    let audit_grpc_url = std::env::var(AUDIT_GRPC_URL_ENV)
        .unwrap_or_else(|_| "http://localhost:50054".to_string());

    let sentry_dsn = std::env::var(SENTRY_DSN_ENV).ok();
    let environment = std::env::var(ENVIRONMENT_ENV).unwrap_or_else(|_| "development".to_string());
    
    let resend_api_key = std::env::var(RESEND_API_KEY_ENV).ok();
    let resend_from_email = std::env::var(RESEND_FROM_EMAIL_ENV)
        .unwrap_or_else(|_| "noreply@rails.co.za".to_string());
    let resend_from_name = std::env::var(RESEND_FROM_NAME_ENV)
        .unwrap_or_else(|_| "Rails Financial Infrastructure".to_string());
    let resend_base_url = std::env::var(RESEND_BASE_URL_ENV)
        .unwrap_or_else(|_| "https://api.resend.com".to_string());
    let resend_beta_notification_email = std::env::var(RESEND_BETA_NOTIFICATION_EMAIL_ENV)
        .unwrap_or_else(|_| resend_from_email.clone());
    let frontend_base_url = std::env::var(FRONTEND_BASE_URL_ENV)
        .unwrap_or_else(|_| "http://localhost:5173".to_string());
    
    Ok(Config {
        database_url,
        server_addr,
        grpc_port,
        accounts_grpc_url,
        audit_grpc_url,
        sentry_dsn,
        environment,
        resend_api_key,
        resend_from_email,
        resend_from_name,
        resend_base_url,
        resend_beta_notification_email,
        frontend_base_url,
    })
}

#[cfg(test)]
impl Config {
    /// Minimal config for unit tests that only exercise a subset of fields (e.g. gRPC client init).
    pub fn test_stub_with_accounts_grpc(accounts_grpc_url: String) -> Self {
        Self {
            database_url: "postgresql://stub:stub@127.0.0.1:1/stub".into(),
            server_addr: "0.0.0.0:0".into(),
            grpc_port: 50051,
            accounts_grpc_url,
            audit_grpc_url: "http://127.0.0.1:1".into(),
            sentry_dsn: None,
            environment: "test".into(),
            resend_api_key: None,
            resend_from_email: "noreply@example.com".into(),
            resend_from_name: "Test".into(),
            resend_base_url: "https://api.example.test".into(),
            resend_beta_notification_email: "beta@example.com".into(),
            frontend_base_url: "http://localhost:5173".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{compose_server_addr, load, strip_jdbc_database_url_prefix};
    use std::sync::{Mutex, OnceLock};

    fn test_env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        match LOCK.get_or_init(|| Mutex::new(())).lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        }
    }

    fn restore_env(key: &str, saved: Option<String>) {
        match saved {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }

    #[test]
    fn strip_jdbc_prefix() {
        assert_eq!(
            strip_jdbc_database_url_prefix("jdbc:postgresql://localhost/db"),
            "postgresql://localhost/db"
        );
        assert_eq!(
            strip_jdbc_database_url_prefix("postgresql://localhost/db"),
            "postgresql://localhost/db"
        );
    }

    #[test]
    fn compose_server_addr_prefers_port_env() {
        assert_eq!(
            compose_server_addr("127.0.0.1", Some(3000), Some("ignored:1")),
            "127.0.0.1:3000"
        );
    }

    #[test]
    fn compose_server_addr_falls_back() {
        assert_eq!(
            compose_server_addr("0.0.0.0", None, Some("0.0.0.0:9090")),
            "0.0.0.0:9090"
        );
        assert_eq!(
            compose_server_addr("0.0.0.0", None, None),
            "0.0.0.0:8080"
        );
    }

    #[test]
    fn load_errors_when_database_url_missing() {
        let _l = test_env_lock();
        const DATABASE_URL_ENV: &str = "DATABASE_URL";
        let saved_db = std::env::var(DATABASE_URL_ENV).ok();
        std::env::remove_var(DATABASE_URL_ENV);
        assert!(load().is_err());
        restore_env(DATABASE_URL_ENV, saved_db);
    }

    #[test]
    fn load_strips_jdbc_prefix_and_reads_optional_env() {
        let _l = test_env_lock();
        const DATABASE_URL_ENV: &str = "DATABASE_URL";
        const GRPC_PORT_ENV: &str = "GRPC_PORT";
        const ACCOUNTS_GRPC_URL_ENV: &str = "ACCOUNTS_GRPC_URL";
        const RESEND_FROM_EMAIL_ENV: &str = "RESEND_FROM_EMAIL";
        let saved_db = std::env::var(DATABASE_URL_ENV).ok();
        let saved_grpc = std::env::var(GRPC_PORT_ENV).ok();
        let saved_accounts = std::env::var(ACCOUNTS_GRPC_URL_ENV).ok();
        let saved_from = std::env::var(RESEND_FROM_EMAIL_ENV).ok();

        std::env::set_var(DATABASE_URL_ENV, "jdbc:postgresql://db.example:5432/app");
        std::env::set_var(GRPC_PORT_ENV, "60001");
        std::env::set_var(ACCOUNTS_GRPC_URL_ENV, "http://accounts.test:999");
        std::env::set_var(RESEND_FROM_EMAIL_ENV, "custom-from@example.com");

        let c = load().expect("load");
        assert_eq!(c.database_url, "postgresql://db.example:5432/app");
        assert_eq!(c.grpc_port, 60001);
        assert_eq!(c.accounts_grpc_url, "http://accounts.test:999");
        assert_eq!(c.resend_from_email, "custom-from@example.com");

        restore_env(DATABASE_URL_ENV, saved_db);
        restore_env(GRPC_PORT_ENV, saved_grpc);
        restore_env(ACCOUNTS_GRPC_URL_ENV, saved_accounts);
        restore_env(RESEND_FROM_EMAIL_ENV, saved_from);
    }
}
