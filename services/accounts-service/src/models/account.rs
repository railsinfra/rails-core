use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: Uuid,
    #[serde(rename = "account_number")]
    pub account_number: String,
    #[serde(rename = "account_type")]
    pub account_type: AccountType,
    #[serde(rename = "organization_id")]
    pub organization_id: Option<Uuid>,
    pub environment: Option<String>,
    #[serde(rename = "holder_id")]
    pub holder_id: Option<Uuid>,
    #[serde(rename = "user_id")]
    pub user_id: Option<Uuid>,
    #[serde(rename = "admin_user_id")]
    pub admin_user_id: Option<Uuid>,
    pub user_role: Option<String>,
    pub currency: Option<String>,
    pub status: Option<AccountStatus>,
    #[serde(rename = "created_at")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(rename = "updated_at")]
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "varchar", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum AccountType {
    Checking,
    Saving,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "varchar", rename_all = "lowercase")]
pub enum AccountStatus {
    Active,
    Suspended,
    Closed,
}

#[derive(Debug, Deserialize)]
pub struct CreateAccountRequest {
    // account_number is auto-generated, not provided by user
    pub account_type: AccountType,
    #[serde(default)]
    pub organization_id: Option<Uuid>,
    #[serde(default = "default_environment")]
    pub environment: Option<String>,
    /// Required when creating account for a platform user (legacy). Omit when using holder metadata (email, first_name, last_name).
    #[serde(default)]
    pub user_id: Option<Uuid>,
    #[serde(default = "default_currency")]
    pub currency: String,
    #[serde(default)]
    pub admin_user_id: Option<Uuid>,  // Required for customer accounts (holder-based or legacy)
    /// Holder-based creation: email (unique per org+env). With first_name, last_name.
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub first_name: Option<String>,
    #[serde(default)]
    pub last_name: Option<String>,
}

fn default_currency() -> String {
    "USD".to_string()
}

fn default_environment() -> Option<String> {
    Some("sandbox".to_string())
}

#[derive(Debug, Deserialize)]
pub struct UpdateAccountRequest {
    pub status: Option<AccountStatus>,
}

#[derive(Debug, Serialize)]
pub struct AccountResponse {
    pub id: Uuid,
    #[serde(rename = "account_number")]
    pub account_number: String,
    #[serde(rename = "account_type")]
    pub account_type: AccountType,
    #[serde(rename = "organization_id")]
    pub organization_id: Option<Uuid>,
    pub environment: String,
    #[serde(rename = "holder_id")]
    pub holder_id: Option<Uuid>,
    #[serde(rename = "user_id")]
    pub user_id: Option<Uuid>,
    #[serde(rename = "admin_user_id")]
    pub admin_user_id: Option<Uuid>,
    pub user_role: Option<String>,
    pub currency: String,
    pub status: AccountStatus,
    #[serde(rename = "created_at")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(rename = "updated_at")]
    pub updated_at: Option<DateTime<Utc>>,
    /// Balance in minor units (cents). 0 if Ledger unavailable.
    pub balance: i64,
}

impl From<Account> for AccountResponse {
    fn from(account: Account) -> Self {
        Self {
            id: account.id,
            account_number: account.account_number,
            account_type: account.account_type,
            organization_id: account.organization_id,
            environment: account.environment.clone().unwrap_or_default(),
            holder_id: account.holder_id,
            user_id: account.user_id,
            admin_user_id: account.admin_user_id,
            user_role: account.user_role,
            currency: account.currency.clone().unwrap_or_else(|| "USD".to_string()),
            status: account.status.unwrap_or(AccountStatus::Active),
            created_at: account.created_at,
            updated_at: account.updated_at,
            balance: 0,
        }
    }
}

impl AccountResponse {
    pub fn with_balance(mut self, balance: i64) -> Self {
        self.balance = balance;
        self
    }
}

#[derive(Debug, Serialize)]
pub struct PaginationMeta {
    pub page: u32,
    pub per_page: u32,
    pub total_count: i64,
    pub total_pages: u32,
}

#[derive(Debug, Serialize)]
pub struct PaginatedAccountsResponse {
    pub data: Vec<AccountResponse>,
    pub pagination: PaginationMeta,
}
