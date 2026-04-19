use crate::config::Config;
use tonic::transport::Channel;

pub mod proto {
    tonic::include_proto!("rails.accounts.v1");
}

use proto::accounts_service_client::AccountsServiceClient;

#[derive(Clone)]
pub struct GrpcClients {
    pub(crate) accounts_client: Option<AccountsServiceClient<Channel>>,
}

pub async fn init(config: &Config) -> Result<GrpcClients, tonic::transport::Error> {
    match AccountsServiceClient::connect(config.accounts_grpc_url.clone()).await {
        Ok(client) => {
            tracing::info!("Connected to Accounts gRPC service at {}", config.accounts_grpc_url);
            Ok(GrpcClients {
                accounts_client: Some(client),
            })
        }
        Err(e) => {
            tracing::warn!(
                "Failed to connect to Accounts gRPC service at {}: {}",
                config.accounts_grpc_url,
                e
            );
            tracing::warn!("Account creation will fail - ensure Accounts service is running");
            Ok(GrpcClients {
                accounts_client: None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::init;
    use crate::config::Config;

    #[tokio::test]
    async fn init_treats_connection_refused_as_optional_accounts_client() {
        let cfg = Config::test_stub_with_accounts_grpc("http://127.0.0.1:1".into());
        let clients = init(&cfg).await.expect("init returns Ok even when connect fails");
        assert!(clients.accounts_client.is_none());
    }
}
