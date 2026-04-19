use sqlx::{Pool, Postgres};
use sqlx::postgres::PgPoolOptions;

pub type Db = Pool<Postgres>;

pub async fn init(database_url: &str) -> Result<Db, sqlx::Error> {
    // Configure connection pool with reasonable defaults
    // For Supabase/Neon, use the pooler URL (ends with -pooler) for better performance
    PgPoolOptions::new()
        .max_connections(5) // Sufficient for single user
        .acquire_timeout(std::time::Duration::from_secs(60)) // Allow Neon cold start (~30s)
        .idle_timeout(std::time::Duration::from_secs(600)) // 10 minutes
        .max_lifetime(std::time::Duration::from_secs(1800)) // 30 minutes
        .test_before_acquire(false)
        .connect(database_url)
        .await
}
