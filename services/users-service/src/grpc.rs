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
