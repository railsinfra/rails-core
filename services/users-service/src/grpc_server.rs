//! gRPC server for users service (ValidateApiKey for accounts service).

use crate::auth;
use sqlx::{PgPool, Row};
use tonic::{Request, Response, Status};
use uuid::Uuid;

pub mod proto {
    tonic::include_proto!("rails.users.v1");
}

use proto::users_service_server::{UsersService as UsersServiceTrait, UsersServiceServer};
use proto::{
    ResolveAccountUserForApiKeyRequest, ResolveAccountUserForApiKeyResponse, ValidateApiKeyRequest,
    ValidateApiKeyResponse,
};

pub(crate) fn validate_api_key_request_inputs(
    api_key: &str,
    environment: &str,
) -> Result<(String, String), Status> {
    let api_key_plain = api_key.trim();
    let environment = environment.trim().to_lowercase();

    if api_key_plain.is_empty() {
        return Err(Status::invalid_argument("api_key is required"));
    }
    if environment != "sandbox" && environment != "production" {
        return Err(Status::invalid_argument(
            "environment must be 'sandbox' or 'production'",
        ));
    }

    Ok((api_key_plain.to_string(), environment))
}

pub(crate) fn normalize_account_user_lookup_inputs(
    email: &str,
    first_name: &str,
    last_name: &str,
) -> Result<(String, String, String), Status> {
    let email = crate::routes::user::normalize_email(email);
    let first_name = first_name.trim().to_string();
    let last_name = last_name.trim().to_string();

    if email.is_empty() {
        return Err(Status::invalid_argument("email is required"));
    }
    if first_name.is_empty() {
        return Err(Status::invalid_argument("first_name is required"));
    }
    if last_name.is_empty() {
        return Err(Status::invalid_argument("last_name is required"));
    }

    Ok((email, first_name, last_name))
}

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

    async fn validate_api_key_scope(
        &self,
        api_key_plain: &str,
        environment: &str,
    ) -> Result<(Uuid, Uuid), Status> {
        let key_hash =
            auth::hash_api_key(api_key_plain).map_err(|e| Status::internal(e.to_string()))?;

        let rec = sqlx::query(
            "SELECT k.id, k.business_id, k.revoked_at, k.status FROM api_keys k WHERE k.key_hash = $1",
        )
        .bind(&key_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Status::internal(e.to_string()))?
        .ok_or_else(|| Status::unauthenticated("Invalid or revoked API key"))?;

        let business_id: Uuid = rec
            .try_get("business_id")
            .map_err(|_| Status::internal("business_id"))?;
        let status: String = rec
            .try_get("status")
            .map_err(|_| Status::internal("status"))?;
        let revoked_at: Option<chrono::DateTime<chrono::Utc>> = rec.try_get("revoked_at").ok();

        if status != "active" || revoked_at.is_some() {
            return Err(Status::unauthenticated("API key is revoked or inactive"));
        }

        let env_row = sqlx::query(
            "SELECT id FROM environments WHERE business_id = $1 AND type = $2 AND status = 'active'",
        )
        .bind(&business_id)
        .bind(environment)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Status::internal(e.to_string()))?
        .ok_or_else(|| Status::failed_precondition("No such environment for business"))?;

        let environment_id: Uuid = env_row.get("id");
        Ok((business_id, environment_id))
    }
}

#[tonic::async_trait]
impl UsersServiceTrait for UsersGrpcService {
    async fn validate_api_key(
        &self,
        request: Request<ValidateApiKeyRequest>,
    ) -> Result<Response<ValidateApiKeyResponse>, Status> {
        let req = request.into_inner();
        let (api_key_plain, environment) =
            validate_api_key_request_inputs(&req.api_key, &req.environment)?;
        let (business_id, environment_id) = self
            .validate_api_key_scope(&api_key_plain, environment.as_str())
            .await?;

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

    async fn resolve_account_user_for_api_key(
        &self,
        request: Request<ResolveAccountUserForApiKeyRequest>,
    ) -> Result<Response<ResolveAccountUserForApiKeyResponse>, Status> {
        let req = request.into_inner();
        let (api_key_plain, environment) =
            validate_api_key_request_inputs(&req.api_key, &req.environment)?;
        let (email, first_name, last_name) =
            normalize_account_user_lookup_inputs(&req.email, &req.first_name, &req.last_name)?;
        let (business_id, environment_id) = self
            .validate_api_key_scope(&api_key_plain, environment.as_str())
            .await?;

        let user_row = sqlx::query(
            "SELECT id FROM users WHERE business_id = $1 AND environment_id = $2 AND email = $3 AND first_name = $4 AND last_name = $5 AND status = 'active' LIMIT 1",
        )
        .bind(&business_id)
        .bind(&environment_id)
        .bind(&email)
        .bind(&first_name)
        .bind(&last_name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Status::internal(e.to_string()))?
        .ok_or_else(|| {
            Status::not_found("No active user matches the supplied account owner details")
        })?;

        let user_id: Uuid = user_row.get("id");
        Ok(Response::new(ResolveAccountUserForApiKeyResponse {
            business_id: business_id.to_string(),
            environment_id: environment_id.to_string(),
            user_id: user_id.to_string(),
        }))
    }
}

#[cfg(test)]
mod validate_api_key_input_tests {
    use super::{normalize_account_user_lookup_inputs, validate_api_key_request_inputs};
    use tonic::Code;

    #[test]
    fn accepts_trimmed_sandbox() {
        let (k, e) = validate_api_key_request_inputs("  abc  ", "  SANDBOX ").expect("ok");
        assert_eq!(k, "abc");
        assert_eq!(e, "sandbox");
    }

    #[test]
    fn rejects_empty_key() {
        let err = validate_api_key_request_inputs("  ", "sandbox").expect_err("err");
        assert_eq!(err.code(), Code::InvalidArgument);
    }

    #[test]
    fn rejects_bad_environment() {
        let err = validate_api_key_request_inputs("k", "staging").expect_err("err");
        assert_eq!(err.code(), Code::InvalidArgument);
    }

    #[test]
    fn normalizes_account_user_lookup_inputs() {
        let (email, first_name, last_name) =
            normalize_account_user_lookup_inputs("  USER@Example.COM ", " First ", " Last ")
                .expect("ok");
        assert_eq!(email, "user@example.com");
        assert_eq!(first_name, "First");
        assert_eq!(last_name, "Last");
    }

    #[test]
    fn rejects_empty_account_user_lookup_inputs() {
        assert_eq!(
            normalize_account_user_lookup_inputs(" ", "First", "Last")
                .expect_err("email")
                .code(),
            Code::InvalidArgument
        );
        assert_eq!(
            normalize_account_user_lookup_inputs("a@example.com", " ", "Last")
                .expect_err("first name")
                .code(),
            Code::InvalidArgument
        );
        assert_eq!(
            normalize_account_user_lookup_inputs("a@example.com", "First", " ")
                .expect_err("last name")
                .code(),
            Code::InvalidArgument
        );
    }
}
