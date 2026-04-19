use luhn3::decimal;
use rand::Rng;
use sqlx::PgPool;

pub(crate) fn clamp_account_number_length(length: usize) -> usize {
    length.max(10).min(16)
}

/// Build a random numeric account number string of total digit count `length` (including Luhn digit).
pub(crate) fn random_account_number_with_luhn(length: usize) -> Result<String, crate::errors::AppError> {
    let length = clamp_account_number_length(length);
    let base_length = length - 1;

    let mut rng = rand::thread_rng();
    let first_digit = rng.gen_range(1..=9);
    let mut base_number = first_digit.to_string();

    for _ in 1..base_length {
        base_number.push_str(&rng.gen_range(0..=9).to_string());
    }

    let checksum_byte = decimal::checksum(base_number.as_bytes()).ok_or_else(|| {
        crate::errors::AppError::Internal("Failed to compute Luhn checksum".to_string())
    })?;
    let checksum = (checksum_byte as char).to_string();
    Ok(format!("{}{}", base_number, checksum))
}

/// Generate a unique numeric account number with Luhn checksum
/// Format: 10-16 digits (configurable) with last digit as checksum
pub async fn generate_account_number(
    pool: &PgPool,
    length: usize,
) -> Result<String, crate::errors::AppError> {
    // Generate account number with retry logic for uniqueness
    const MAX_RETRIES: u32 = 10;
    for _ in 0..MAX_RETRIES {
        let account_number = random_account_number_with_luhn(length)?;

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

#[cfg(test)]
mod tests {
    use super::{clamp_account_number_length, random_account_number_with_luhn};

    #[test]
    fn clamp_length_respects_bounds() {
        assert_eq!(clamp_account_number_length(5), 10);
        assert_eq!(clamp_account_number_length(12), 12);
        assert_eq!(clamp_account_number_length(30), 16);
    }

    #[test]
    fn random_account_number_has_expected_len_and_luhn_digit() {
        for _ in 0..20 {
            let n = random_account_number_with_luhn(10).expect("ok");
            assert_eq!(n.len(), 10);
            assert!(n.chars().all(|c| c.is_ascii_digit()));
            let base = &n[..n.len() - 1];
            let checksum_byte = luhn3::decimal::checksum(base.as_bytes()).expect("checksum");
            assert_eq!(n.chars().last().expect("digit"), checksum_byte as char);
        }
    }
}
