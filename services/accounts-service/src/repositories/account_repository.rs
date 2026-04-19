use crate::errors::AppError;
use crate::models::{Account, AccountStatus, AccountType, PaginatedAccountsResponse, PaginationMeta, AccountResponse};
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub struct AccountRepository;

impl AccountRepository {
    pub async fn create(
        pool: &PgPool,
        account_number: &str,
        account_type: AccountType,
        organization_id: Option<Uuid>,
        environment: &str,
        user_id: Uuid,
        currency: &str,
    ) -> Result<Account, AppError> {
        let account_type_str: &str = match account_type {
            AccountType::Checking => "checking",
            AccountType::Saving => "saving",
        };

        let row = sqlx::query(
            r#"
            INSERT INTO accounts (account_number, account_type, organization_id, environment, user_id, currency)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, account_number, account_type, organization_id, environment, holder_id, user_id, admin_user_id, user_role, currency, status, created_at, updated_at
            "#,
        )
        .bind(account_number)
        .bind(account_type_str)
        .bind(organization_id)
        .bind(environment)
        .bind(user_id)
        .bind(currency)
        .fetch_one(pool)
        .await?;

        Ok(Self::row_to_account(&row)?)
    }

    pub async fn create_with_hierarchy(
        executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
        account_number: &str,
        account_type: AccountType,
        organization_id: Option<Uuid>,
        environment: &str,
        user_id: Uuid,
        admin_user_id: Option<Uuid>,
        user_role: Option<String>,
        currency: &str,
    ) -> Result<Account, sqlx::Error> {
        let account_type_str: &str = match account_type {
            AccountType::Checking => "checking",
            AccountType::Saving => "saving",
        };

        let row = sqlx::query(
            r#"
            INSERT INTO accounts (account_number, account_type, organization_id, environment, user_id, admin_user_id, user_role, currency)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id, account_number, account_type, organization_id, environment, holder_id, user_id, admin_user_id, user_role, currency, status, created_at, updated_at
            "#,
        )
        .bind(account_number)
        .bind(account_type_str)
        .bind(organization_id)
        .bind(environment)
        .bind(user_id)
        .bind(admin_user_id)
        .bind(user_role)
        .bind(currency)
        .fetch_one(executor)
        .await?;

        Ok(Self::row_to_account(&row).map_err(|e| sqlx::Error::Protocol(e.to_string().into()))?)
    }

    pub async fn find_by_id(pool: &PgPool, id: Uuid, environment: &str) -> Result<Account, AppError> {
        let row = sqlx::query(
            r#"
            SELECT id, account_number, account_type, organization_id, environment, holder_id, user_id, admin_user_id, user_role, currency, status, created_at, updated_at
            FROM accounts
            WHERE id = $1 AND environment = $2
            "#,
        )
        .bind(id)
        .bind(environment)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Account with id {} not found in environment {}", id, environment)))?;

        Ok(Self::row_to_account(&row)?)
    }

    pub async fn find_by_user_id(pool: &PgPool, user_id: Uuid, environment: &str) -> Result<Vec<Account>, AppError> {
        let rows = sqlx::query(
            r#"
            SELECT id, account_number, account_type, organization_id, environment, holder_id, user_id, admin_user_id, user_role, currency, status, created_at, updated_at
            FROM accounts
            WHERE user_id = $1 AND environment = $2
            ORDER BY created_at DESC, id DESC
            "#,
        )
        .bind(user_id)
        .bind(environment)
        .fetch_all(pool)
        .await?;

        let accounts = rows
            .iter()
            .map(|row| Self::row_to_account(row))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(accounts)
    }

    pub async fn find_by_organization_id(pool: &PgPool, organization_id: Uuid, environment: &str) -> Result<Vec<Account>, AppError> {
        let rows = sqlx::query(
            r#"
            SELECT id, account_number, account_type, organization_id, environment, holder_id, user_id, admin_user_id, user_role, currency, status, created_at, updated_at
            FROM accounts
            WHERE organization_id = $1 AND environment = $2
            ORDER BY created_at DESC, id DESC
            "#,
        )
        .bind(organization_id)
        .bind(environment)
        .fetch_all(pool)
        .await?;

        let accounts = rows
            .iter()
            .map(|row| Self::row_to_account(row))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(accounts)
    }

    pub async fn find_by_admin_user_id(pool: &PgPool, admin_user_id: Uuid, environment: &str) -> Result<Vec<Account>, AppError> {
        let rows = sqlx::query(
            r#"
            SELECT id, account_number, account_type, organization_id, environment, holder_id, user_id, admin_user_id, user_role, currency, status, created_at, updated_at
            FROM accounts
            WHERE admin_user_id = $1 AND environment = $2
            ORDER BY created_at DESC, id DESC
            "#,
        )
        .bind(admin_user_id)
        .bind(environment)
        .fetch_all(pool)
        .await?;

        let accounts = rows
            .iter()
            .map(|row| Self::row_to_account(row))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(accounts)
    }

    pub async fn find_by_user_id_paginated(
        pool: &PgPool,
        user_id: Uuid,
        environment: &str,
        page: u32,
        per_page: u32,
    ) -> Result<PaginatedAccountsResponse, AppError> {
        let offset = (page - 1) * per_page;

        // Get total count (filtered by environment)
        let count_row = sqlx::query(
            "SELECT COUNT(*) as count FROM accounts WHERE user_id = $1 AND environment = $2"
        )
        .bind(user_id)
        .bind(environment)
        .fetch_one(pool)
        .await?;

        let total_count: i64 = count_row.get("count");
        let total_pages = ((total_count as f64) / (per_page as f64)).ceil() as u32;

        // Fetch paginated results (filtered by environment)
        let rows = sqlx::query(
            r#"
            SELECT id, account_number, account_type, organization_id, environment, holder_id, user_id, admin_user_id, user_role, currency, status, created_at, updated_at
            FROM accounts
            WHERE user_id = $1 AND environment = $2
            ORDER BY created_at DESC, id DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(user_id)
        .bind(environment)
        .bind(per_page as i64)
        .bind(offset as i64)
        .fetch_all(pool)
        .await?;

        let accounts: Vec<Account> = rows
            .iter()
            .map(|row| Self::row_to_account(row))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(PaginatedAccountsResponse {
            data: accounts.into_iter().map(AccountResponse::from).collect(),
            pagination: PaginationMeta {
                page,
                per_page,
                total_count,
                total_pages,
            },
        })
    }

    pub async fn find_by_organization_id_paginated(
        pool: &PgPool,
        organization_id: Uuid,
        environment: &str,
        page: u32,
        per_page: u32,
    ) -> Result<PaginatedAccountsResponse, AppError> {
        let offset = (page - 1) * per_page;

        // Get total count (filtered by environment)
        let count_row = sqlx::query(
            "SELECT COUNT(*) as count FROM accounts WHERE organization_id = $1 AND environment = $2"
        )
        .bind(organization_id)
        .bind(environment)
        .fetch_one(pool)
        .await?;

        let total_count: i64 = count_row.get("count");
        let total_pages = ((total_count as f64) / (per_page as f64)).ceil() as u32;

        // Fetch paginated results (filtered by environment)
        let rows = sqlx::query(
            r#"
            SELECT id, account_number, account_type, organization_id, environment, holder_id, user_id, admin_user_id, user_role, currency, status, created_at, updated_at
            FROM accounts
            WHERE organization_id = $1 AND environment = $2
            ORDER BY created_at DESC, id DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(organization_id)
        .bind(environment)
        .bind(per_page as i64)
        .bind(offset as i64)
        .fetch_all(pool)
        .await?;

        let accounts: Vec<Account> = rows
            .iter()
            .map(|row| Self::row_to_account(row))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(PaginatedAccountsResponse {
            data: accounts.into_iter().map(AccountResponse::from).collect(),
            pagination: PaginationMeta {
                page,
                per_page,
                total_count,
                total_pages,
            },
        })
    }

    pub async fn find_by_admin_user_id_paginated(
        pool: &PgPool,
        admin_user_id: Uuid,
        environment: &str,
        page: u32,
        per_page: u32,
    ) -> Result<PaginatedAccountsResponse, AppError> {
        let offset = (page - 1) * per_page;

        // Get total count (filtered by environment)
        let count_row = sqlx::query(
            "SELECT COUNT(*) as count FROM accounts WHERE admin_user_id = $1 AND environment = $2"
        )
        .bind(admin_user_id)
        .bind(environment)
        .fetch_one(pool)
        .await?;

        let total_count: i64 = count_row.get("count");
        let total_pages = ((total_count as f64) / (per_page as f64)).ceil() as u32;

        // Fetch paginated results (filtered by environment)
        let rows = sqlx::query(
            r#"
            SELECT id, account_number, account_type, organization_id, environment, holder_id, user_id, admin_user_id, user_role, currency, status, created_at, updated_at
            FROM accounts
            WHERE admin_user_id = $1 AND environment = $2
            ORDER BY created_at DESC, id DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(admin_user_id)
        .bind(environment)
        .bind(per_page as i64)
        .bind(offset as i64)
        .fetch_all(pool)
        .await?;

        let accounts: Vec<Account> = rows
            .iter()
            .map(|row| Self::row_to_account(row))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(PaginatedAccountsResponse {
            data: accounts.into_iter().map(AccountResponse::from).collect(),
            pagination: PaginationMeta {
                page,
                per_page,
                total_count,
                total_pages,
            },
        })
    }

    pub async fn update_status(
        pool: &PgPool,
        id: Uuid,
        environment: &str,
        status: AccountStatus,
    ) -> Result<Account, AppError> {
        let status_str: &str = match status {
            AccountStatus::Active => "active",
            AccountStatus::Suspended => "suspended",
            AccountStatus::Closed => "closed",
        };

        let row = sqlx::query(
            r#"
            UPDATE accounts
            SET status = $3, updated_at = NOW()
            WHERE id = $1 AND environment = $2
            RETURNING id, account_number, account_type, organization_id, environment, holder_id, user_id, admin_user_id, user_role, currency, status, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(environment)
        .bind(status_str)
        .fetch_one(pool)
        .await?;

        Ok(Self::row_to_account(&row)?)
    }

    /// Create account for a holder (SDK flow). No user_id.
    pub async fn create_with_holder(
        pool: &PgPool,
        account_number: &str,
        account_type: AccountType,
        organization_id: Uuid,
        environment: &str,
        holder_id: Uuid,
        admin_user_id: Option<Uuid>,
        currency: &str,
    ) -> Result<Account, AppError> {
        let account_type_str: &str = match account_type {
            AccountType::Checking => "checking",
            AccountType::Saving => "saving",
        };

        let row = sqlx::query(
            r#"
            INSERT INTO accounts (account_number, account_type, organization_id, environment, holder_id, admin_user_id, user_role, currency)
            VALUES ($1, $2, $3, $4, $5, $6, 'CUSTOMER', $7)
            RETURNING id, account_number, account_type, organization_id, environment, holder_id, user_id, admin_user_id, user_role, currency, status, created_at, updated_at
            "#,
        )
        .bind(account_number)
        .bind(account_type_str)
        .bind(organization_id)
        .bind(environment)
        .bind(holder_id)
        .bind(admin_user_id)
        .bind(currency)
        .fetch_one(pool)
        .await?;

        Ok(Self::row_to_account(&row)?)
    }

    /// Count accounts by holder and account_type (for enforcing max 1 checking, 1 saving per holder).
    pub async fn count_by_holder_and_type(
        pool: &PgPool,
        holder_id: Uuid,
        account_type: AccountType,
        environment: &str,
    ) -> Result<i64, AppError> {
        let account_type_str: &str = match account_type {
            AccountType::Checking => "checking",
            AccountType::Saving => "saving",
        };
        let row = sqlx::query(
            "SELECT COUNT(*) as count FROM accounts WHERE holder_id = $1 AND account_type = $2 AND environment = $3",
        )
        .bind(holder_id)
        .bind(account_type_str)
        .bind(environment)
        .fetch_one(pool)
        .await?;
        let count: i64 = row.get("count");
        Ok(count)
    }

    fn row_to_account(row: &sqlx::postgres::PgRow) -> Result<Account, AppError> {
        let account_type_str: String = row.get("account_type");
        let account_type = match account_type_str.as_str() {
            "checking" => AccountType::Checking,
            "saving" => AccountType::Saving,
            _ => return Err(AppError::InvalidAccountType),
        };

        let status_str: String = row.get("status");
        let status = match status_str.as_str() {
            "active" => AccountStatus::Active,
            "suspended" => AccountStatus::Suspended,
            "closed" => AccountStatus::Closed,
            _ => return Err(AppError::Internal("Invalid account status".to_string())),
        };

        Ok(Account {
            id: row.get("id"),
            account_number: row.get("account_number"),
            account_type,
            organization_id: row.get("organization_id"),
            environment: row.get("environment"),
            holder_id: row.try_get("holder_id").ok(),
            user_id: row.try_get("user_id").ok(),
            admin_user_id: row.try_get("admin_user_id").ok(),
            user_role: row.try_get("user_role").ok(),
            currency: row.try_get("currency").ok(),
            status: Some(status),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
    }
}
