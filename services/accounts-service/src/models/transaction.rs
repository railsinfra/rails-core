use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Transaction {
    pub id: Uuid,
    #[serde(rename = "organization_id")]
    pub organization_id: Uuid,
    #[serde(rename = "from_account_id")]
    pub from_account_id: Uuid,
    #[serde(rename = "to_account_id")]
    pub to_account_id: Uuid,
    pub amount: i64,
    pub currency: String,
    #[serde(rename = "transaction_kind")]
    pub transaction_kind: TransactionKind,
    pub status: TransactionStatus,
    #[serde(rename = "failure_reason")]
    pub failure_reason: Option<String>,
    #[serde(rename = "idempotency_key")]
    pub idempotency_key: String,
    pub environment: Option<String>,
    #[serde(rename = "description")]
    pub description: Option<String>,
    #[serde(rename = "external_recipient_id")]
    pub external_recipient_id: Option<String>,
    #[serde(rename = "reference_id")]
    pub reference_id: Option<Uuid>,
    #[serde(rename = "created_at")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "updated_at")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "varchar", rename_all = "lowercase")]
pub enum TransactionKind {
    Deposit,
    Withdraw,
    Transfer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "varchar", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum TransactionStatus {
    Pending,
    Posted,
    Failed,
}

#[derive(Debug, Serialize)]
pub struct TransactionResponse {
    pub id: Uuid,
    #[serde(rename = "organization_id")]
    pub organization_id: Uuid,
    #[serde(rename = "from_account_id")]
    pub from_account_id: Uuid,
    #[serde(rename = "to_account_id")]
    pub to_account_id: Uuid,
    pub amount: i64,
    pub currency: String,
    #[serde(rename = "transaction_kind")]
    pub transaction_kind: TransactionKind,
    pub status: TransactionStatus,
    #[serde(rename = "failure_reason")]
    pub failure_reason: Option<String>,
    #[serde(rename = "idempotency_key")]
    pub idempotency_key: String,
    pub environment: Option<String>,
    #[serde(rename = "created_at")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "updated_at")]
    pub updated_at: DateTime<Utc>,
}

impl From<Transaction> for TransactionResponse {
    fn from(transaction: Transaction) -> Self {
        Self {
            id: transaction.id,
            organization_id: transaction.organization_id,
            from_account_id: transaction.from_account_id,
            to_account_id: transaction.to_account_id,
            amount: transaction.amount,
            currency: transaction.currency,
            transaction_kind: transaction.transaction_kind,
            status: transaction.status,
            failure_reason: transaction.failure_reason,
            idempotency_key: transaction.idempotency_key,
            environment: transaction.environment,
            created_at: transaction.created_at,
            updated_at: transaction.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PaginatedTransactionsResponse {
    pub data: Vec<AccountTransactionResponse>,
    pub pagination: crate::models::account::PaginationMeta,
}

/// SDK-compatible transaction view when listing by account.
/// Maps our from/to model to account_id, transaction_type, balance_after, etc.
#[derive(Debug, Serialize)]
pub struct AccountTransactionResponse {
    pub id: Uuid,
    #[serde(rename = "account_id")]
    pub account_id: Uuid,
    #[serde(rename = "transaction_type")]
    pub transaction_type: String,
    /// Amount in minor units (cents).
    #[serde(rename = "amount")]
    pub amount: i64,
    /// Balance after transaction in minor units (cents).
    #[serde(rename = "balance_after")]
    pub balance_after: i64,
    pub currency: String,
    pub status: String,
    #[serde(rename = "created_at")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "updated_at")]
    pub updated_at: DateTime<Utc>,
    #[serde(rename = "description")]
    pub description: String,
    #[serde(rename = "external_recipient_id")]
    pub external_recipient_id: String,
    #[serde(rename = "recipient_account_id")]
    pub recipient_account_id: String,
    #[serde(rename = "reference_id")]
    pub reference_id: String,
}

impl AccountTransactionResponse {
    /// Build from Transaction when listing for a specific account.
    /// balance_after: display balance in cents (from Ledger). Use 0 if unavailable.
    pub fn from_transaction(
        transaction: &Transaction,
        for_account_id: Uuid,
        balance_after: i64,
    ) -> Self {
        let (transaction_type, recipient_account_id) = match transaction.transaction_kind {
            TransactionKind::Deposit => ("deposit".to_string(), String::new()),
            TransactionKind::Withdraw => ("withdrawal".to_string(), String::new()),
            TransactionKind::Transfer => {
                if transaction.from_account_id == for_account_id {
                    ("withdrawal".to_string(), transaction.to_account_id.to_string())
                } else {
                    ("deposit".to_string(), transaction.from_account_id.to_string())
                }
            }
        };
        let description = transaction.description.as_deref().unwrap_or("");
        let external_recipient_id = transaction.external_recipient_id.as_deref().unwrap_or("");
        let reference_id = transaction.reference_id.map(|u| u.to_string()).unwrap_or_default();
        Self::build(
            transaction,
            for_account_id,
            &transaction_type,
            &recipient_account_id,
            description,
            balance_after,
            external_recipient_id,
            &reference_id,
        )
    }

    /// Build for deposit/withdraw/transfer API responses.
    /// For transfer: use account_id=from_id, transaction_type="transfer", recipient_account_id=to_id.
    /// balance_after: account balance in cents after the transaction (from Ledger).
    pub fn for_mutation_response(
        transaction: &Transaction,
        balance_after: i64,
    ) -> Self {
        let (account_id, transaction_type, recipient_account_id) = match transaction.transaction_kind {
            TransactionKind::Deposit => (
                transaction.from_account_id,
                "deposit".to_string(),
                String::new(),
            ),
            TransactionKind::Withdraw => (
                transaction.from_account_id,
                "withdrawal".to_string(),
                String::new(),
            ),
            TransactionKind::Transfer => (
                transaction.from_account_id,
                "transfer".to_string(),
                transaction.to_account_id.to_string(),
            ),
        };
        let description = transaction.description.as_deref().unwrap_or("");
        let external_recipient_id = transaction.external_recipient_id.as_deref().unwrap_or("");
        let reference_id = transaction.reference_id.map(|u| u.to_string()).unwrap_or_default();
        Self::build(
            transaction,
            account_id,
            &transaction_type,
            &recipient_account_id,
            description,
            balance_after,
            external_recipient_id,
            &reference_id,
        )
    }

    fn build(
        transaction: &Transaction,
        account_id: Uuid,
        transaction_type: &str,
        recipient_account_id: &str,
        description: &str,
        balance_after: i64,
        external_recipient_id: &str,
        reference_id: &str,
    ) -> Self {
        let status = match transaction.status {
            TransactionStatus::Pending => "pending",
            TransactionStatus::Posted => "completed",
            TransactionStatus::Failed => "failed",
        };
        Self {
            id: transaction.id,
            account_id,
            transaction_type: transaction_type.to_string(),
            amount: transaction.amount,
            balance_after,
            currency: transaction.currency.clone(),
            status: status.to_string(),
            created_at: transaction.created_at,
            updated_at: transaction.updated_at,
            description: description.to_string(),
            external_recipient_id: external_recipient_id.to_string(),
            recipient_account_id: recipient_account_id.to_string(),
            reference_id: reference_id.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn sample_transaction(
        id: Uuid,
        from: Uuid,
        to: Uuid,
        kind: TransactionKind,
        status: TransactionStatus,
        amount: i64,
    ) -> Transaction {
        Transaction {
            id,
            organization_id: Uuid::nil(),
            from_account_id: from,
            to_account_id: to,
            amount,
            currency: "USD".to_string(),
            transaction_kind: kind,
            status,
            failure_reason: None,
            idempotency_key: "test".to_string(),
            environment: Some("sandbox".to_string()),
            description: None,
            external_recipient_id: None,
            reference_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn from_transaction_deposit() {
        let acc = Uuid::new_v4();
        let tx = sample_transaction(Uuid::new_v4(), acc, acc, TransactionKind::Deposit, TransactionStatus::Posted, 10000);
        let resp = AccountTransactionResponse::from_transaction(&tx, acc, 15000);
        assert_eq!(resp.account_id, acc);
        assert_eq!(resp.transaction_type, "deposit");
        assert_eq!(resp.amount, 10000);
        assert_eq!(resp.status, "completed");
        assert_eq!(resp.balance_after, 15000);
        assert!(resp.recipient_account_id.is_empty());
    }

    #[test]
    fn from_transaction_withdrawal() {
        let acc = Uuid::new_v4();
        let tx = sample_transaction(Uuid::new_v4(), acc, acc, TransactionKind::Withdraw, TransactionStatus::Posted, 5000);
        let resp = AccountTransactionResponse::from_transaction(&tx, acc, 5000);
        assert_eq!(resp.transaction_type, "withdrawal");
        assert_eq!(resp.status, "completed");
    }

    #[test]
    fn from_transaction_transfer_as_sender() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();
        let tx = sample_transaction(Uuid::new_v4(), from, to, TransactionKind::Transfer, TransactionStatus::Posted, 2000);
        let resp = AccountTransactionResponse::from_transaction(&tx, from, 8000);
        assert_eq!(resp.account_id, from);
        assert_eq!(resp.transaction_type, "withdrawal");
        assert_eq!(resp.recipient_account_id, to.to_string());
    }

    #[test]
    fn from_transaction_transfer_as_receiver() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();
        let tx = sample_transaction(Uuid::new_v4(), from, to, TransactionKind::Transfer, TransactionStatus::Posted, 2000);
        let resp = AccountTransactionResponse::from_transaction(&tx, to, 12000);
        assert_eq!(resp.account_id, to);
        assert_eq!(resp.transaction_type, "deposit");
        assert_eq!(resp.recipient_account_id, from.to_string());
    }

    #[test]
    fn from_transaction_balance_after_defaults_to_zero_when_empty() {
        let acc = Uuid::new_v4();
        let tx = sample_transaction(Uuid::new_v4(), acc, acc, TransactionKind::Deposit, TransactionStatus::Posted, 100);
        let resp = AccountTransactionResponse::from_transaction(&tx, acc, 0);
        assert_eq!(resp.balance_after, 0);
    }

    #[test]
    fn from_transaction_status_mapping() {
        let acc = Uuid::new_v4();
        let tx_pending = sample_transaction(Uuid::new_v4(), acc, acc, TransactionKind::Deposit, TransactionStatus::Pending, 100);
        let tx_failed = sample_transaction(Uuid::new_v4(), acc, acc, TransactionKind::Deposit, TransactionStatus::Failed, 100);
        assert_eq!(AccountTransactionResponse::from_transaction(&tx_pending, acc, 0).status, "pending");
        assert_eq!(AccountTransactionResponse::from_transaction(&tx_failed, acc, 0).status, "failed");
    }

    #[test]
    fn for_mutation_response_deposit() {
        let acc = Uuid::new_v4();
        let tx = sample_transaction(Uuid::new_v4(), acc, acc, TransactionKind::Deposit, TransactionStatus::Posted, 10000);
        let resp = AccountTransactionResponse::for_mutation_response(&tx, 0);
        assert_eq!(resp.account_id, acc);
        assert_eq!(resp.transaction_type, "deposit");
        assert!(resp.recipient_account_id.is_empty());
    }

    #[test]
    fn for_mutation_response_withdraw() {
        let acc = Uuid::new_v4();
        let tx = sample_transaction(Uuid::new_v4(), acc, acc, TransactionKind::Withdraw, TransactionStatus::Posted, 5000);
        let resp = AccountTransactionResponse::for_mutation_response(&tx, 0);
        assert_eq!(resp.account_id, acc);
        assert_eq!(resp.transaction_type, "withdrawal");
    }

    #[test]
    fn for_mutation_response_transfer() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();
        let tx = sample_transaction(Uuid::new_v4(), from, to, TransactionKind::Transfer, TransactionStatus::Posted, 2000);
        let resp = AccountTransactionResponse::for_mutation_response(&tx, 0);
        assert_eq!(resp.account_id, from);
        assert_eq!(resp.transaction_type, "transfer");
        assert_eq!(resp.recipient_account_id, to.to_string());
    }

    #[test]
    fn for_mutation_response_includes_description() {
        let mut tx = sample_transaction(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), TransactionKind::Withdraw, TransactionStatus::Posted, 1000);
        tx.description = Some("Optional note".to_string());
        let resp = AccountTransactionResponse::for_mutation_response(&tx, 0);
        assert_eq!(resp.description, "Optional note");
    }

    #[test]
    fn for_mutation_response_includes_balance_after() {
        let tx = sample_transaction(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), TransactionKind::Deposit, TransactionStatus::Posted, 1000);
        let resp = AccountTransactionResponse::for_mutation_response(&tx, 25000);
        assert_eq!(resp.balance_after, 25000);
    }

    #[test]
    fn for_mutation_response_balance_after_zero() {
        let tx = sample_transaction(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), TransactionKind::Deposit, TransactionStatus::Posted, 1000);
        let resp = AccountTransactionResponse::for_mutation_response(&tx, 0);
        assert_eq!(resp.balance_after, 0);
    }

    #[test]
    fn for_mutation_response_includes_external_recipient_and_reference() {
        let mut tx = sample_transaction(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), TransactionKind::Withdraw, TransactionStatus::Posted, 1000);
        tx.external_recipient_id = Some("ext_bank_123".to_string());
        tx.reference_id = Some(Uuid::new_v4());
        let ref_id = tx.reference_id.unwrap().to_string();
        let resp = AccountTransactionResponse::for_mutation_response(&tx, 0);
        assert_eq!(resp.external_recipient_id, "ext_bank_123");
        assert_eq!(resp.reference_id, ref_id);
    }
}
