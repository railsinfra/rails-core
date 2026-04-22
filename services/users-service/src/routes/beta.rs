//! Private beta application endpoint

use std::collections::HashMap;
use std::net::SocketAddr;

use axum::extract::ConnectInfo;
use axum::http::HeaderMap;
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::audit_emit;
use crate::error::{AppError, DUPLICATE_BETA_EMAIL_MESSAGE};
use crate::grpc::audit_proto::ActorType;
use crate::routes::{user, AppState};

#[derive(Deserialize)]
pub struct BetaApplicationRequest {
    pub name: String,
    pub email: String,
    pub company: String,
    #[serde(rename = "use_case")]
    pub use_case: String,
}

#[derive(Serialize)]
pub struct BetaApplicationResponse {
    pub message: String,
}

struct BetaApplicationInput {
    name: String,
    email: String,
    company: String,
    use_case: String,
}

fn normalize_payload(payload: BetaApplicationRequest) -> Result<BetaApplicationInput, AppError> {
    let name = payload.name.trim();
    let email = payload.email.trim();
    let company = payload.company.trim();
    let use_case = payload.use_case.trim();

    if name.is_empty() || email.is_empty() || company.is_empty() || use_case.is_empty() {
        return Err(AppError::BadRequest(
            "All fields are required.".to_string(),
        ));
    }

    Ok(BetaApplicationInput {
        name: name.to_string(),
        email: email.to_string(),
        company: company.to_string(),
        use_case: use_case.to_string(),
    })
}

async fn apply_for_beta_inner(
    state: AppState,
    Json(payload): Json<BetaApplicationRequest>,
) -> Result<(BetaApplicationResponse, Uuid), AppError> {
    let input = normalize_payload(payload)?;
    let email_normalized = user::normalize_email(&input.email);
    if email_normalized.is_empty() {
        return Err(AppError::BadRequest("Email is required.".to_string()));
    }

    // Application-level duplicate check (defense in depth with DB constraint)
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM beta_applications WHERE LOWER(TRIM(email)) = $1)"
    )
    .bind(&email_normalized)
    .fetch_one(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;
    if exists {
        return Err(AppError::Conflict(DUPLICATE_BETA_EMAIL_MESSAGE.to_string()));
    }

    let application_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO beta_applications (id, name, email, company, use_case) VALUES ($1, $2, $3, $4, $5)"
    )
    .bind(application_id)
    .bind(&input.name)
    .bind(&email_normalized)
    .bind(&input.company)
    .bind(&input.use_case)
    .execute(&state.db)
    .await
    .map_err(|error| {
        if let Some(db_err) = error.as_database_error() {
            let msg = db_err.message();
            if msg.contains("duplicate key") || msg.contains("idx_beta_applications_email_normalized") || msg.contains("unique constraint") {
                return AppError::Conflict(DUPLICATE_BETA_EMAIL_MESSAGE.to_string());
            }
        }
        tracing::error!("Failed to insert beta application: {}", error);
        AppError::Internal
    })?;

    if let Some(email_service) = &state.email {
        if let Err(error) = email_service
            .send_beta_application(&input.name, &email_normalized, &input.company, &input.use_case)
            .await
        {
            tracing::error!("Failed to send beta application email: {}", error);
        }
    } else {
        tracing::warn!("Email service not configured, beta application not sent");
    }

    Ok((
        BetaApplicationResponse {
            message: "Application received. We'll be in touch shortly.".to_string(),
        },
        application_id,
    ))
}

pub async fn apply_for_beta(
    State(state): State<AppState>,
    headers: HeaderMap,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(payload): Json<BetaApplicationRequest>,
) -> Result<Json<BetaApplicationResponse>, AppError> {
    let path = "/api/v1/beta/apply";
    let mut meta = HashMap::new();
    match apply_for_beta_inner(state.clone(), Json(payload)).await {
        Ok((body, app_id)) => {
            audit_emit::emit_users_mutation(
                &state.grpc,
                &headers,
                &peer,
                "POST",
                path,
                "users.beta.apply",
                Uuid::nil(),
                ActorType::Anonymous,
                "",
                vec![],
                "beta_application",
                app_id,
                200,
                None,
                meta,
            )
            .await;
            Ok(Json(body))
        }
        Err(e) => {
            meta.insert(
                "http_status".into(),
                audit_emit::http_status_for_error(&e).to_string(),
            );
            audit_emit::emit_users_mutation(
                &state.grpc,
                &headers,
                &peer,
                "POST",
                path,
                "users.beta.apply",
                Uuid::nil(),
                ActorType::Anonymous,
                "",
                vec![],
                "beta_application",
                Uuid::nil(),
                audit_emit::http_status_for_error(&e),
                Some(audit_emit::truncate_reason(&e.to_string())),
                meta,
            )
            .await;
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::email::EmailService;
    use crate::grpc::GrpcClients;
    use crate::test_support::test_connect_info;
    use axum::http::HeaderMap;
    use httpmock::Method::POST;
    use httpmock::MockServer;
    use sqlx::postgres::PgPoolOptions;

    #[test]
    fn normalize_payload_trims_and_accepts_valid_input() {
        let payload = BetaApplicationRequest {
            name: " Jane Doe ".to_string(),
            email: " jane@acme.com ".to_string(),
            company: " Acme ".to_string(),
            use_case: " Treasury ".to_string(),
        };

        let result = normalize_payload(payload).expect("should be valid");
        assert_eq!(result.name, "Jane Doe");
        assert_eq!(result.email, "jane@acme.com");
        assert_eq!(result.company, "Acme");
        assert_eq!(result.use_case, "Treasury");
    }

    #[test]
    fn normalize_payload_rejects_empty_fields() {
        let payload = BetaApplicationRequest {
            name: " ".to_string(),
            email: "test@example.com".to_string(),
            company: "Acme".to_string(),
            use_case: "Use case".to_string(),
        };

        let result = normalize_payload(payload);
        assert!(matches!(result, Err(AppError::BadRequest(_))));
    }

    #[tokio::test]
    async fn apply_for_beta_persists_and_sends_email() {
        let database_url = match std::env::var("DATABASE_URL") {
            Ok(value) => value,
            Err(_) => {
                eprintln!("DATABASE_URL not set; skipping beta application integration test.");
                return;
            }
        };

        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&database_url)
            .await
            .expect("Failed to connect to database");

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        let server = MockServer::start_async().await;
        let resend_mock = server
            .mock_async(|when, then| {
                when.method(POST).path("/emails");
                then.status(200)
                    .header("content-type", "application/json")
                    .body("{\"id\":\"email-1\"}");
            })
            .await;

        let config = Config {
            database_url: database_url.clone(),
            server_addr: "127.0.0.1:0".to_string(),
            grpc_port: 50051,
            accounts_grpc_url: "http://localhost:50052".to_string(),
            audit_grpc_url: "http://127.0.0.1:1".to_string(),
            sentry_dsn: None,
            environment: "test".to_string(),
            resend_api_key: Some("test-key".to_string()),
            resend_from_email: "noreply@rails.co.za".to_string(),
            resend_from_name: "Rails Financial Infrastructure".to_string(),
            resend_base_url: server.base_url(),
            resend_beta_notification_email: "beta@rails.co.za".to_string(),
            frontend_base_url: "http://localhost:5173".to_string(),
        };

        let email_service = EmailService::new(&config);
        let state = AppState {
            db: pool.clone(),
            grpc: GrpcClients::none(),
            email: Some(email_service),
        };

        let applicant_email = format!("beta+{}@example.com", Uuid::new_v4());
        let payload = BetaApplicationRequest {
            name: "Jane Doe".to_string(),
            email: applicant_email.clone(),
            company: "Acme".to_string(),
            use_case: "Treasury automation".to_string(),
        };

        let response = apply_for_beta(
            State(state),
            HeaderMap::new(),
            test_connect_info(),
            Json(payload),
        )
        .await
        .expect("apply_for_beta should succeed");

        assert_eq!(
            response.0.message,
            "Application received. We'll be in touch shortly."
        );

        let count: i64 = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM beta_applications WHERE email = $1"
        )
        .bind(&applicant_email)
        .fetch_one(&pool)
        .await
        .expect("Failed to query beta applications");

        assert_eq!(count, 1);
        resend_mock.assert_async().await;
    }

    #[tokio::test]
    async fn apply_for_beta_succeeds_without_email_service() {
        let database_url = match std::env::var("DATABASE_URL") {
            Ok(value) => value,
            Err(_) => {
                eprintln!("DATABASE_URL not set; skipping beta application integration test.");
                return;
            }
        };

        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&database_url)
            .await
            .expect("Failed to connect to database");

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        let state = AppState {
            db: pool.clone(),
            grpc: GrpcClients::none(),
            email: None,
        };

        let applicant_email = format!("beta+{}@example.com", Uuid::new_v4());
        let payload = BetaApplicationRequest {
            name: "Jane Doe".to_string(),
            email: applicant_email.clone(),
            company: "Acme".to_string(),
            use_case: "Treasury automation".to_string(),
        };

        let response = apply_for_beta(
            State(state),
            HeaderMap::new(),
            test_connect_info(),
            Json(payload),
        )
        .await
        .expect("apply_for_beta should succeed without email");

        assert_eq!(
            response.0.message,
            "Application received. We'll be in touch shortly."
        );

        let count: i64 = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM beta_applications WHERE email = $1"
        )
        .bind(&applicant_email)
        .fetch_one(&pool)
        .await
        .expect("Failed to query beta applications");

        assert_eq!(count, 1);
    }
}
