//! gRPC client for users-service API key validation.

use tonic::transport::{Channel, Endpoint};
use uuid::Uuid;

pub mod users_proto {
    tonic::include_proto!("rails.users.v1");
}

use users_proto::users_service_client::UsersServiceClient;

#[derive(Clone)]
pub struct UsersGrpc {
    client: UsersServiceClient<Channel>,
}

impl UsersGrpc {
    /// Create a lazy gRPC client so audit-service startup does not depend on users-service readiness.
    pub fn connect_lazy(url: &str) -> anyhow::Result<Self> {
        let channel = Endpoint::from_shared(url.to_string())
            .map_err(|e| anyhow::anyhow!("Invalid USERS_GRPC_URL: {}", e))?
            .connect_lazy();
        Ok(Self {
            client: UsersServiceClient::new(channel),
        })
    }

    pub async fn validate_api_key(
        &self,
        api_key: &str,
        environment: &str,
    ) -> Result<(Uuid, Uuid, Uuid), tonic::Status> {
        let req = users_proto::ValidateApiKeyRequest {
            api_key: api_key.to_string(),
            environment: environment.to_string(),
        };
        let res = self
            .client
            .clone()
            .validate_api_key(tonic::Request::new(req))
            .await?;
        let body = res.into_inner();

        let business_id = Uuid::parse_str(&body.business_id)
            .map_err(|_| tonic::Status::internal("invalid business_id from users service"))?;
        let environment_id = Uuid::parse_str(&body.environment_id)
            .map_err(|_| tonic::Status::internal("invalid environment_id from users service"))?;
        let admin_user_id = Uuid::parse_str(&body.admin_user_id)
            .map_err(|_| tonic::Status::internal("invalid admin_user_id from users service"))?;

        Ok((business_id, environment_id, admin_user_id))
    }
}
