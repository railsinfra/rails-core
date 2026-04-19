use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

async fn insert_pending_row(
    pool: &PgPool,
    org: Uuid,
    from_account_id: Uuid,
    to_account_id: Uuid,
    transaction_kind: &str,
    idempotency_key: &str,
    environment: Option<&str>,
    created_age: Duration,
) -> Uuid {
    let id = Uuid::new_v4();
    let age_secs: i64 = created_age.num_seconds().max(1);
    sqlx::query(
        r#"
        INSERT INTO transactions (
            id, organization_id, from_account_id, to_account_id, amount, currency,
            transaction_kind, status, failure_reason, idempotency_key, environment,
            description, external_recipient_id, reference_id, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, 100, 'USD', $8, 'pending', NULL, $5, $6,
                NULL, NULL, NULL,
                NOW() - ($7 * INTERVAL '1 second'),
                NOW() - ($7 * INTERVAL '1 second'))
        "#,
    )
    .bind(id)
    .bind(org)
    .bind(from_account_id)
    .bind(to_account_id)
    .bind(idempotency_key)
    .bind(environment)
    .bind(age_secs)
    .bind(transaction_kind)
    .execute(pool)
    .await
    .unwrap();
    id
}

pub async fn insert_pending_deposit(
    pool: &PgPool,
    org: Uuid,
    idem: &str,
    env: &str,
    created_age: Duration,
) -> Uuid {
    let acc = Uuid::new_v4();
    insert_pending_row(
        pool,
        org,
        acc,
        acc,
        "deposit",
        idem,
        Some(env),
        created_age,
    )
    .await
}

pub async fn insert_pending_tx_kind(
    pool: &PgPool,
    org: Uuid,
    transaction_kind: &str,
    from_account_id: Uuid,
    to_account_id: Uuid,
    idem: &str,
    env: &str,
    created_age: Duration,
) -> Uuid {
    insert_pending_row(
        pool,
        org,
        from_account_id,
        to_account_id,
        transaction_kind,
        idem,
        Some(env),
        created_age,
    )
    .await
}

pub async fn mark_posting_stale_30s(pool: &PgPool, id: Uuid) {
    sqlx::query(
        "UPDATE transactions SET status = 'posting', updated_at = NOW() - interval '30 seconds' WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
}

pub async fn allow_null_transaction_environment(pool: &PgPool) {
    sqlx::query("ALTER TABLE transactions ALTER COLUMN environment DROP NOT NULL")
        .execute(pool)
        .await
        .unwrap();
}

pub async fn insert_posting_deposit_null_environment(
    pool: &PgPool,
    id: Uuid,
    org: Uuid,
    account_id: Uuid,
    idempotency_key: &str,
    created_at: DateTime<Utc>,
) {
    sqlx::query(
        r#"
        INSERT INTO transactions (
            id, organization_id, from_account_id, to_account_id, amount, currency,
            transaction_kind, status, failure_reason, idempotency_key, environment,
            description, external_recipient_id, reference_id, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, 100, 'USD', 'deposit', 'posting', NULL, $5, NULL,
                NULL, NULL, NULL, $6, $6)
        "#,
    )
    .bind(id)
    .bind(org)
    .bind(account_id)
    .bind(account_id)
    .bind(idempotency_key)
    .bind(created_at)
    .execute(pool)
    .await
    .unwrap();
}

pub async fn drop_transaction_kind_check(pool: &PgPool) {
    sqlx::query(
        "ALTER TABLE transactions DROP CONSTRAINT IF EXISTS transactions_transaction_kind_check",
    )
    .execute(pool)
    .await
    .unwrap();
}

pub async fn drop_transaction_status_check(pool: &PgPool) {
    sqlx::query("ALTER TABLE transactions DROP CONSTRAINT IF EXISTS transactions_status_check")
        .execute(pool)
        .await
        .unwrap();
}

pub async fn insert_pending_row_bypassing_checks(
    pool: &PgPool,
    org: Uuid,
    account_id: Uuid,
    id: Uuid,
    idempotency_key: &str,
    transaction_kind: &str,
    status: &str,
    created_at: DateTime<Utc>,
) {
    sqlx::query(
        r#"
        INSERT INTO transactions (
            id, organization_id, from_account_id, to_account_id, amount, currency,
            transaction_kind, status, failure_reason, idempotency_key, environment,
            description, external_recipient_id, reference_id, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, 100, 'USD', $6, $7, NULL, $5, 'sandbox',
                NULL, NULL, NULL, $8, $8)
        "#,
    )
    .bind(id)
    .bind(org)
    .bind(account_id)
    .bind(account_id)
    .bind(idempotency_key)
    .bind(transaction_kind)
    .bind(status)
    .bind(created_at)
    .execute(pool)
    .await
    .unwrap();
}

pub fn hours_ago(hours: i64) -> DateTime<Utc> {
    Utc::now() - Duration::hours(hours)
}
