use crate::errors::AppError;
use crate::models::{Transaction, TransactionKind, TransactionStatus, PaginationMeta};
use chrono::{DateTime, Duration, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub struct TransactionRepository;

impl TransactionRepository {
    pub async fn create_or_get_by_idempotency(
        executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
        organization_id: Uuid,
        from_account_id: Uuid,
        to_account_id: Uuid,
        amount: i64,
        currency: &str,
        transaction_kind: TransactionKind,
        idempotency_key: &str,
        environment: &str,
        description: Option<&str>,
        external_recipient_id: Option<&str>,
        reference_id: Option<Uuid>,
    ) -> Result<Transaction, AppError> {
        let kind_str: &str = match transaction_kind {
            TransactionKind::Deposit => "deposit",
            TransactionKind::Withdraw => "withdraw",
            TransactionKind::Transfer => "transfer",
        };

        // Use a CTE-based approach to handle idempotency with the unique index
        // (organization_id, environment, idempotency_key). Environment is required (no NULL).
        let row = sqlx::query(
            r#"
            WITH existing AS (
                SELECT id, organization_id, from_account_id, to_account_id, amount, currency,
                       transaction_kind, status, failure_reason, idempotency_key, environment, description, external_recipient_id, reference_id, created_at, updated_at
                FROM transactions
                WHERE organization_id = $1
                  AND environment = $8
                  AND idempotency_key = $7
                LIMIT 1
            ),
            inserted AS (
                INSERT INTO transactions (
                    organization_id, from_account_id, to_account_id, amount, currency,
                    transaction_kind, status, failure_reason, idempotency_key, environment, description, external_recipient_id, reference_id
                )
                SELECT $1, $2, $3, $4, $5, $6, 'pending', NULL, $7, $8, $9, $10, $11
                WHERE NOT EXISTS (SELECT 1 FROM existing)
                RETURNING id, organization_id, from_account_id, to_account_id, amount, currency,
                          transaction_kind, status, failure_reason, idempotency_key, environment, description, external_recipient_id, reference_id, created_at, updated_at
            )
            SELECT * FROM inserted
            UNION ALL
            SELECT * FROM existing
            LIMIT 1
            "#,
        )
        .bind(organization_id)
        .bind(from_account_id)
        .bind(to_account_id)
        .bind(amount)
        .bind(currency)
        .bind(kind_str)
        .bind(idempotency_key)
        .bind(environment)
        .bind(description)
        .bind(external_recipient_id)
        .bind(reference_id)
        .fetch_one(executor)
        .await?;

        Ok(Self::row_to_transaction(&row)?)
    }

    pub async fn find_by_idempotency(
        pool: &PgPool,
        organization_id: Uuid,
        environment: &str,
        idempotency_key: &str,
    ) -> Result<Option<Transaction>, AppError> {
        let row = sqlx::query(
            r#"
            SELECT id, organization_id, from_account_id, to_account_id, amount, currency,
                   transaction_kind, status, failure_reason, idempotency_key, environment, description, external_recipient_id, reference_id, created_at, updated_at
            FROM transactions
            WHERE organization_id = $1
              AND environment = $2
              AND idempotency_key = $3
            LIMIT 1
            "#,
        )
        .bind(organization_id)
        .bind(environment)
        .bind(idempotency_key)
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|r| Self::row_to_transaction(&r)).transpose()?)
    }

    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Transaction, AppError> {
        let row = sqlx::query(
            r#"
            SELECT id, organization_id, from_account_id, to_account_id, amount, currency,
                   transaction_kind, status, failure_reason, idempotency_key, environment, description, external_recipient_id, reference_id, created_at, updated_at
            FROM transactions
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Transaction with id {} not found", id)))?;

        Ok(Self::row_to_transaction(&row)?)
    }

    pub async fn find_by_account_id(
        pool: &PgPool,
        account_id: Uuid,
        limit: Option<i64>,
        environment: Option<&str>,
    ) -> Result<Vec<Transaction>, AppError> {
        let limit = limit.unwrap_or(100);

        let rows = if let Some(env) = environment {
            sqlx::query(
                r#"
                SELECT id, organization_id, from_account_id, to_account_id, amount, currency,
                       transaction_kind, status, failure_reason, idempotency_key, environment, description, external_recipient_id, reference_id, created_at, updated_at
                FROM transactions
                WHERE (from_account_id = $1 OR to_account_id = $1)
                  AND environment = $3
                ORDER BY created_at DESC
                LIMIT $2
                "#,
            )
            .bind(account_id)
            .bind(limit)
            .bind(env)
            .fetch_all(pool)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT id, organization_id, from_account_id, to_account_id, amount, currency,
                       transaction_kind, status, failure_reason, idempotency_key, environment, description, external_recipient_id, reference_id, created_at, updated_at
                FROM transactions
                WHERE from_account_id = $1 OR to_account_id = $1
                ORDER BY created_at DESC
                LIMIT $2
                "#,
            )
            .bind(account_id)
            .bind(limit)
            .fetch_all(pool)
            .await?
        };

        let transactions = rows
            .iter()
            .map(|row| Self::row_to_transaction(row))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(transactions)
    }

    pub async fn find_by_organization_id_paginated(
        pool: &PgPool,
        organization_id: Uuid,
        environment: &str,
        page: u32,
        per_page: u32,
    ) -> Result<(Vec<Transaction>, PaginationMeta), AppError> {
        let offset = (page - 1) * per_page;

        let count_row = sqlx::query(
            r#"
            SELECT COUNT(*) as count 
            FROM transactions
            WHERE organization_id = $1 AND environment = $2
            "#
        )
        .bind(organization_id)
        .bind(environment)
        .fetch_one(pool)
        .await?;

        let total_count: i64 = count_row.get("count");
        let total_pages = ((total_count as f64) / (per_page as f64)).ceil() as u32;

        // Fetch paginated results with deterministic ordering
        let rows = sqlx::query(
            r#"
            SELECT id, organization_id, from_account_id, to_account_id, amount, currency,
                   transaction_kind, status, failure_reason, idempotency_key, environment, description, external_recipient_id, reference_id, created_at, updated_at
            FROM transactions
            WHERE organization_id = $1 AND environment = $2
            ORDER BY created_at DESC, id DESC
            LIMIT $3 OFFSET $4
            "#
        )
        .bind(organization_id)
        .bind(environment)
        .bind(per_page as i64)
        .bind(offset as i64)
        .fetch_all(pool)
        .await?;

        let transactions = rows
            .iter()
            .map(|row| Self::row_to_transaction(row))
            .collect::<Result<Vec<_>, _>>()?;

        Ok((
            transactions,
            PaginationMeta {
                page,
                per_page,
                total_count,
                total_pages,
            },
        ))
    }

    /// Find pending transactions across all organizations older than a cutoff.
    /// Used by the ledger retry worker (eventual consistency).
    pub async fn find_pending_older_than_any_org(
        pool: &PgPool,
        older_than: Duration,
        limit: Option<i64>,
        environment: Option<&str>,
    ) -> Result<Vec<Transaction>, AppError> {
        let cutoff: DateTime<Utc> = Utc::now() - older_than;
        let limit = limit.unwrap_or(100);

        let rows = if let Some(env) = environment {
            sqlx::query(
                r#"
                SELECT id, organization_id, from_account_id, to_account_id, amount, currency,
                       transaction_kind, status, failure_reason, idempotency_key, environment, description, external_recipient_id, reference_id, created_at, updated_at
                FROM transactions
                WHERE status = 'pending'
                  AND created_at < $1
                  AND environment = $3
                ORDER BY created_at ASC
                LIMIT $2
                "#,
            )
            .bind(cutoff)
            .bind(limit)
            .bind(env)
            .fetch_all(pool)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT id, organization_id, from_account_id, to_account_id, amount, currency,
                       transaction_kind, status, failure_reason, idempotency_key, environment, description, external_recipient_id, reference_id, created_at, updated_at
                FROM transactions
                WHERE status = 'pending' AND created_at < $1
                ORDER BY created_at ASC
                LIMIT $2
                "#,
            )
            .bind(cutoff)
            .bind(limit)
            .fetch_all(pool)
            .await?
        };

        Ok(rows
            .iter()
            .map(|row| Self::row_to_transaction(row))
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub async fn update_status(
        executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
        id: Uuid,
        status: TransactionStatus,
        failure_reason: Option<&str>,
    ) -> Result<Transaction, AppError> {
        let status_str: &str = match status {
            TransactionStatus::Pending => "pending",
            TransactionStatus::Posted => "posted",
            TransactionStatus::Failed => "failed",
        };

        let row = sqlx::query(
            r#"
            UPDATE transactions
            SET status = $2, failure_reason = $3, updated_at = NOW()
            WHERE id = $1
            RETURNING id, organization_id, from_account_id, to_account_id, amount, currency,
                      transaction_kind, status, failure_reason, idempotency_key, environment, description, external_recipient_id, reference_id, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(status_str)
        .bind(failure_reason)
        .fetch_one(executor)
        .await?;

        Ok(Self::row_to_transaction(&row)?)
    }

    pub fn row_to_transaction(row: &sqlx::postgres::PgRow) -> Result<Transaction, AppError> {
        let kind_str: String = row.get("transaction_kind");
        let transaction_kind = match kind_str.as_str() {
            "deposit" => TransactionKind::Deposit,
            "withdraw" => TransactionKind::Withdraw,
            "transfer" => TransactionKind::Transfer,
            _ => return Err(AppError::Internal("Invalid transaction kind".to_string())),
        };

        let status_str: String = row.get("status");
        let status = match status_str.as_str() {
            "pending" => TransactionStatus::Pending,
            "posted" => TransactionStatus::Posted,
            "failed" => TransactionStatus::Failed,
            _ => return Err(AppError::Internal("Invalid transaction status".to_string())),
        };

        Ok(Transaction {
            id: row.get("id"),
            organization_id: row.get("organization_id"),
            from_account_id: row.get("from_account_id"),
            to_account_id: row.get("to_account_id"),
            amount: row.get("amount"),
            currency: row.get("currency"),
            transaction_kind,
            status,
            failure_reason: row.get("failure_reason"),
            idempotency_key: row.get("idempotency_key"),
            environment: row.get("environment"),
            description: row.try_get("description").ok().flatten(),
            external_recipient_id: row.try_get("external_recipient_id").ok().flatten(),
            reference_id: row.try_get("reference_id").ok().flatten(),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
    }
}
