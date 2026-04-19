use crate::errors::AppError;
use crate::models::AccountHolder;
use sqlx::PgPool;
use uuid::Uuid;

pub struct AccountHolderRepository;

impl AccountHolderRepository {
    /// Find holder by (organization_id, environment, email). Email is normalized to lowercase.
    pub async fn find_by_org_env_email(
        pool: &PgPool,
        organization_id: Uuid,
        environment: &str,
        email: &str,
    ) -> Result<Option<AccountHolder>, AppError> {
        let email_normalized = email.trim().to_lowercase();
        if email_normalized.is_empty() {
            return Err(AppError::Validation("Email is required".to_string()));
        }

        let row = sqlx::query(
            r#"
            SELECT id, organization_id, environment, email, first_name, last_name, created_at, updated_at
            FROM account_holders
            WHERE organization_id = $1 AND environment = $2 AND email = $3
            "#,
        )
        .bind(organization_id)
        .bind(environment)
        .bind(&email_normalized)
        .fetch_optional(pool)
        .await?;

        Ok(row.map(|r| Self::row_to_holder(&r)).transpose()?)
    }

    /// Create a new holder. Returns error if (organization_id, environment, email) already exists.
    pub async fn create(
        pool: &PgPool,
        organization_id: Uuid,
        environment: &str,
        email: &str,
        first_name: &str,
        last_name: &str,
    ) -> Result<AccountHolder, AppError> {
        let email_normalized = email.trim().to_lowercase();
        if email_normalized.is_empty() {
            return Err(AppError::Validation("Email is required".to_string()));
        }

        let row = sqlx::query(
            r#"
            INSERT INTO account_holders (organization_id, environment, email, first_name, last_name)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, organization_id, environment, email, first_name, last_name, created_at, updated_at
            "#,
        )
        .bind(organization_id)
        .bind(environment)
        .bind(&email_normalized)
        .bind(first_name.trim())
        .bind(last_name.trim())
        .fetch_one(pool)
        .await?;

        Self::row_to_holder(&row)
    }

    /// Get or create holder: find by (org, env, email) or create.
    pub async fn get_or_create(
        pool: &PgPool,
        organization_id: Uuid,
        environment: &str,
        email: &str,
        first_name: &str,
        last_name: &str,
    ) -> Result<AccountHolder, AppError> {
        if let Some(holder) = Self::find_by_org_env_email(pool, organization_id, environment, email).await? {
            return Ok(holder);
        }
        Self::create(pool, organization_id, environment, email, first_name, last_name).await
    }

    fn row_to_holder(row: &sqlx::postgres::PgRow) -> Result<AccountHolder, AppError> {
        use sqlx::Row;
        Ok(AccountHolder {
            id: row.get("id"),
            organization_id: row.get("organization_id"),
            environment: row.get("environment"),
            email: row.get("email"),
            first_name: row.get("first_name"),
            last_name: row.get("last_name"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
    }
}
