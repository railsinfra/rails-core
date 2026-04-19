//! gRPC server for users service (ValidateApiKey for accounts service).

use crate::auth;
use sqlx::{PgPool, Row};
use tonic::{Request, Response, Status};
use uuid::Uuid;

pub mod proto {
    tonic::include_proto!("rails.users.v1");
}

use proto::users_service_server::{UsersService as UsersServiceTrait, UsersServiceServer};
use proto::{ValidateApiKeyRequest, ValidateApiKeyResponse};

#[derive(Clone)]
pub struct UsersGrpcService {
    pool: PgPool,
}

impl UsersGrpcService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn into_server(self) -> UsersServiceServer<Self> {
        UsersServiceServer::new(self)
    }
}

#[tonic::async_trait]
impl UsersServiceTrait for UsersGrpcService {
    async fn validate_api_key(
        &self,
        request: Request<ValidateApiKeyRequest>,
    ) -> Result<Response<ValidateApiKeyResponse>, Status> {
        let req = request.into_inner();
        let api_key_plain = req.api_key.trim();
        let environment = req.environment.trim().to_lowercase();

        if api_key_plain.is_empty() {
            return Err(Status::invalid_argument("api_key is required"));
        }
        if environment != "sandbox" && environment != "production" {
            return Err(Status::invalid_argument(
                "environment must be 'sandbox' or 'production'",
            ));
        }

        let key_hash = auth::hash_api_key(api_key_plain).map_err(|e| Status::internal(e.to_string()))?;

        let rec = sqlx::query(
            "SELECT k.id, k.business_id, k.revoked_at, k.status FROM api_keys k WHERE k.key_hash = $1",
        )
        .bind(&key_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Status::internal(e.to_string()))?
        .ok_or_else(|| Status::unauthenticated("Invalid or revoked API key"))?;

        let business_id: Uuid = rec.try_get("business_id").map_err(|_| Status::internal("business_id"))?;
        let status: String = rec.try_get("status").map_err(|_| Status::internal("status"))?;
        let revoked_at: Option<chrono::DateTime<chrono::Utc>> = rec.try_get("revoked_at").ok();

        if status != "active" || revoked_at.is_some() {
            return Err(Status::unauthenticated("API key is revoked or inactive"));
        }

        let env_row = sqlx::query(
            "SELECT id FROM environments WHERE business_id = $1 AND type = $2 AND status = 'active'",
        )
        .bind(&business_id)
        .bind(&environment)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Status::internal(e.to_string()))?
        .ok_or_else(|| Status::failed_precondition("No such environment for business"))?;

        let environment_id: Uuid = env_row.get("id");

        let admin_row = sqlx::query(
            "SELECT id FROM users WHERE business_id = $1 AND environment_id = $2 AND role = 'admin' AND status = 'active' LIMIT 1",
        )
        .bind(&business_id)
        .bind(&environment_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Status::internal(e.to_string()))?
        .ok_or_else(|| Status::failed_precondition("No admin user in this environment"))?;

        let admin_user_id: Uuid = admin_row.get("id");

        Ok(Response::new(ValidateApiKeyResponse {
            business_id: business_id.to_string(),
            environment_id: environment_id.to_string(),
            admin_user_id: admin_user_id.to_string(),
        }))
    }
}
