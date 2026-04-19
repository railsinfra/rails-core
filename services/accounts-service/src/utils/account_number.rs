use luhn3::decimal;
use rand::Rng;
use sqlx::PgPool;

/// Generate a unique numeric account number with Luhn checksum
/// Format: 10-16 digits (configurable) with last digit as checksum
pub async fn generate_account_number(
    pool: &PgPool,
    length: usize,
) -> Result<String, crate::errors::AppError> {
    // Ensure minimum length of 10 digits for banking standards
    let length = length.max(10).min(16); // Max 16 digits
    
    // Generate random base digits (length - 1, since last digit is checksum)
    let base_length = length - 1;
    
    // Generate account number with retry logic for uniqueness
    const MAX_RETRIES: u32 = 10;
    for _ in 0..MAX_RETRIES {
        let account_number = {
            // Generate random base number (avoid leading zeros)
            let mut rng = rand::thread_rng();
            let first_digit = rng.gen_range(1..=9);
            let mut base_number = first_digit.to_string();

            for _ in 1..base_length {
                base_number.push_str(&rng.gen_range(0..=9).to_string());
            }

            // Calculate and append Luhn checksum digit
            let checksum_byte = decimal::checksum(base_number.as_bytes()).ok_or_else(|| {
                crate::errors::AppError::Internal("Failed to compute Luhn checksum".to_string())
            })?;
            let checksum = (checksum_byte as char).to_string();
            format!("{}{}", base_number, checksum)
        };
        
        // Verify it's unique in database
        if !account_number_exists(pool, &account_number).await? {
            return Ok(account_number);
        }
    }
    
    Err(crate::errors::AppError::Internal(
        "Failed to generate unique account number after multiple attempts".to_string(),
    ))
}

/// Check if account number already exists in database
async fn account_number_exists(
    pool: &PgPool,
    account_number: &str,
) -> Result<bool, crate::errors::AppError> {
    let result = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM accounts WHERE account_number = $1)",
    )
    .bind(account_number)
    .fetch_one(pool)
    .await?;

    Ok(result)
}
