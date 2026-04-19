#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub server_addr: String,
    pub grpc_port: u16,
    pub accounts_grpc_url: String,
    pub sentry_dsn: Option<String>,
    pub environment: String,
    pub resend_api_key: Option<String>,
    pub resend_from_email: String,
    pub resend_from_name: String,
    pub resend_base_url: String,
    pub resend_beta_notification_email: String,
    pub frontend_base_url: String,
}

pub fn load() -> Result<Config, anyhow::Error> {
    let mut database_url = std::env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!(
            "DATABASE_URL environment variable is required. \
            Set it to your PostgreSQL connection string (e.g., Supabase, Neon, or local PostgreSQL). \
            Example: postgresql://user:password@host:5432/database"
        ))?;

    if let Some(stripped) = database_url.strip_prefix("jdbc:") {
        database_url = stripped.to_string();
    }

    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = std::env::var("PORT").ok().and_then(|p| p.parse::<u16>().ok());
    let server_addr = if let Some(port) = port {
        format!("{}:{}", host, port)
    } else {
        std::env::var("SERVER_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string())
    };

    let grpc_port = std::env::var("GRPC_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(50051);

    let accounts_grpc_url = std::env::var("ACCOUNTS_GRPC_URL")
        .unwrap_or_else(|_| "http://localhost:50052".to_string());
    
    let sentry_dsn = std::env::var("SENTRY_DSN").ok();
    let environment = std::env::var("ENVIRONMENT")
        .unwrap_or_else(|_| "development".to_string());
    
    let resend_api_key = std::env::var("RESEND_API_KEY").ok();
    let resend_from_email = std::env::var("RESEND_FROM_EMAIL")
        .unwrap_or_else(|_| "noreply@rails.co.za".to_string());
    let resend_from_name = std::env::var("RESEND_FROM_NAME")
        .unwrap_or_else(|_| "Rails Financial Infrastructure".to_string());
    let resend_base_url = std::env::var("RESEND_BASE_URL")
        .unwrap_or_else(|_| "https://api.resend.com".to_string());
    let resend_beta_notification_email = std::env::var("RESEND_BETA_NOTIFICATION_EMAIL")
        .unwrap_or_else(|_| resend_from_email.clone());
    let frontend_base_url = std::env::var("FRONTEND_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:5173".to_string());
    
    Ok(Config {
        database_url,
        server_addr,
        grpc_port,
        accounts_grpc_url,
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
