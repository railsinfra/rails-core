//! Binary entry point for audit-service.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    audit_service::bootstrap::run().await
}
