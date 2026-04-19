//! gRPC client for users service (ValidateApiKey for holder-based account creation).

use crate::errors::AppError;
use tonic::transport::Channel;
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
    /// Create a lazy gRPC client. Connection happens on first RPC, so startup is not blocked
    /// if the users service is temporarily unreachable (e.g. during Railway deployment).
    pub fn connect_lazy(url: &str) -> Result<Self, AppError> {
        use tonic::transport::Endpoint;
        let channel = Endpoint::from_shared(url.to_string())
            .map_err(|e| AppError::Internal(format!("Invalid USERS_GRPC_URL: {}", e)))?
            .connect_lazy();
        Ok(Self {
            client: UsersServiceClient::new(channel),
        })
    }

    /// Validate API key and return (business_id, environment_id, admin_user_id).
    pub async fn validate_api_key(
        &self,
        api_key: &str,
        environment: &str,
    ) -> Result<(Uuid, Uuid, Uuid), AppError> {
        use tonic::Request;
        let req = users_proto::ValidateApiKeyRequest {
            api_key: api_key.to_string(),
            environment: environment.to_string(),
        };
        let res = self
            .client
            .clone()
            .validate_api_key(Request::new(req))
            .await
            .map_err(|e| {
                let msg = e.message().to_string();
                if e.code() == tonic::Code::Unauthenticated || e.code() == tonic::Code::InvalidArgument {
                    AppError::Unauthorized(msg)
                } else {
                    AppError::Internal(format!("Users gRPC error: {}", msg))
                }
            })?;
        let r = res.into_inner();
        let business_id = Uuid::parse_str(&r.business_id)
            .map_err(|_| AppError::Internal("Invalid business_id from users service".to_string()))?;
        let environment_id = Uuid::parse_str(&r.environment_id)
            .map_err(|_| AppError::Internal("Invalid environment_id from users service".to_string()))?;
        let admin_user_id = Uuid::parse_str(&r.admin_user_id)
            .map_err(|_| AppError::Internal("Invalid admin_user_id from users service".to_string()))?;
        Ok((business_id, environment_id, admin_user_id))
    }
}

#[cfg(test)]
mod tests {
    use super::users_proto::users_service_server::{UsersService, UsersServiceServer};
    use super::users_proto::{ValidateApiKeyRequest, ValidateApiKeyResponse};
    use super::UsersGrpc;
    use std::net::SocketAddr;
    use tokio_stream::wrappers::TcpListenerStream;
    use tonic::transport::Server;
    use tonic::{Request, Response, Status};

    #[derive(Clone, Default)]
    struct MockOk;

    #[tonic::async_trait]
    impl UsersService for MockOk {
        async fn validate_api_key(
            &self,
            _req: Request<ValidateApiKeyRequest>,
        ) -> Result<Response<ValidateApiKeyResponse>, Status> {
            Ok(Response::new(ValidateApiKeyResponse {
                business_id: uuid::Uuid::nil().to_string(),
                environment_id: uuid::Uuid::nil().to_string(),
                admin_user_id: uuid::Uuid::nil().to_string(),
            }))
        }
    }

    #[derive(Clone, Default)]
    struct MockUnauth;

    #[tonic::async_trait]
    impl UsersService for MockUnauth {
        async fn validate_api_key(
            &self,
            _req: Request<ValidateApiKeyRequest>,
        ) -> Result<Response<ValidateApiKeyResponse>, Status> {
            Err(Status::unauthenticated("bad key"))
        }
    }

    #[derive(Clone, Default)]
    struct MockInvalidArg;

    #[tonic::async_trait]
    impl UsersService for MockInvalidArg {
        async fn validate_api_key(
            &self,
            _req: Request<ValidateApiKeyRequest>,
        ) -> Result<Response<ValidateApiKeyResponse>, Status> {
            Err(Status::invalid_argument("bad"))
        }
    }

    #[derive(Clone, Default)]
    struct MockBadIds;

    #[tonic::async_trait]
    impl UsersService for MockBadIds {
        async fn validate_api_key(
            &self,
            _req: Request<ValidateApiKeyRequest>,
        ) -> Result<Response<ValidateApiKeyResponse>, Status> {
            Ok(Response::new(ValidateApiKeyResponse {
                business_id: "not-a-uuid".into(),
                environment_id: uuid::Uuid::nil().to_string(),
                admin_user_id: uuid::Uuid::nil().to_string(),
            }))
        }
    }

    async fn users_base_url<S>(svc: S) -> String
    where
        S: UsersService + Send + Sync + 'static + Clone,
    {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        let incoming = TcpListenerStream::new(listener);
        tokio::spawn(async move {
            Server::builder()
                .add_service(UsersServiceServer::new(svc))
                .serve_with_incoming(incoming)
                .await
                .ok();
        });
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        format!("http://{}", addr)
    }

    #[tokio::test]
    async fn connect_lazy_rejects_invalid_uri() {
        let res = UsersGrpc::connect_lazy("%%%not-uri");
        assert!(res.is_err(), "expected invalid URL error");
        let msg = format!("{}", res.err().expect("err"));
        assert!(msg.contains("USERS_GRPC_URL") || msg.contains("Invalid"), "{msg}");
    }

    #[tokio::test]
    async fn validate_api_key_ok_round_trip() {
        let url = users_base_url(MockOk::default()).await;
        let grpc = UsersGrpc::connect_lazy(&url).expect("lazy");
        let (b, e, a) = grpc
            .validate_api_key("k", "sandbox")
            .await
            .expect("ok");
        assert_eq!(b, uuid::Uuid::nil());
        assert_eq!(e, uuid::Uuid::nil());
        assert_eq!(a, uuid::Uuid::nil());
    }

    #[tokio::test]
    async fn validate_api_key_maps_unauthenticated() {
        let url = users_base_url(MockUnauth::default()).await;
        let grpc = UsersGrpc::connect_lazy(&url).expect("lazy");
        let err = grpc
            .validate_api_key("k", "sandbox")
            .await
            .expect_err("unauth");
        assert!(matches!(err, crate::errors::AppError::Unauthorized(_)));
    }

    #[tokio::test]
    async fn validate_api_key_maps_invalid_argument_to_unauthorized() {
        let url = users_base_url(MockInvalidArg::default()).await;
        let grpc = UsersGrpc::connect_lazy(&url).expect("lazy");
        let err = grpc
            .validate_api_key("k", "sandbox")
            .await
            .expect_err("unauth");
        assert!(matches!(err, crate::errors::AppError::Unauthorized(_)));
    }

    #[tokio::test]
    async fn validate_api_key_invalid_uuid_is_internal() {
        let url = users_base_url(MockBadIds::default()).await;
        let grpc = UsersGrpc::connect_lazy(&url).expect("lazy");
        let err = grpc
            .validate_api_key("k", "sandbox")
            .await
            .expect_err("internal");
        assert!(matches!(err, crate::errors::AppError::Internal(_)));
    }
}
