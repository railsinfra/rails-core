use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountHolder {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub environment: String,
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}
