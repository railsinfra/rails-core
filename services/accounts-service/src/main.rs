#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    accounts_api::run().await
}
