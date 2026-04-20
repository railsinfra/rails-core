//! Binary entry point for the Users microservice.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    users_service::bootstrap::run().await
}
