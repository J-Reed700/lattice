//! Small persistence helpers every Learning repository shares.
use crate::shared::error::{AppError, Result};
use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};

/// Map a SQLite error. A pool timeout means the database is busy rather than
/// broken, so it is reported as a retryable condition.
pub(crate) fn db(error: sqlx::Error) -> AppError {
    match error {
        sqlx::Error::PoolTimedOut => AppError::ServiceNotAvailable(
            "The learning database is temporarily busy. Saved work is retained.".into(),
        ),
        error => AppError::Database(error.to_string()),
    }
}

/// Wall-clock time in Unix milliseconds, the unit every Learning table stores.
pub(crate) fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// SHA-256 of a value's JSON encoding, as lowercase hex.
pub(crate) fn hash<T: Serialize + ?Sized>(value: &T) -> Result<String> {
    let bytes =
        serde_json::to_vec(value).map_err(|error| AppError::Serialization(error.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

/// SHA-256 of text, as lowercase hex.
pub(crate) fn hash_text(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

pub(crate) fn encode<T: Serialize + ?Sized>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|error| AppError::Serialization(error.to_string()))
}

pub(crate) fn decode<T: DeserializeOwned>(value: &str) -> Result<T> {
    serde_json::from_str(value).map_err(|error| AppError::Serialization(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_matches_the_hash_of_the_encoded_text() -> Result<()> {
        let value = serde_json::json!({"programId": "p", "revision": 3});
        assert_eq!(hash(&value)?, hash_text(&encode(&value)?));
        assert_eq!(hash_text("abc").len(), 64);
        Ok(())
    }

    #[test]
    fn pool_timeouts_are_reported_as_a_busy_database() {
        assert!(matches!(
            db(sqlx::Error::PoolTimedOut),
            AppError::ServiceNotAvailable(_)
        ));
        assert!(matches!(
            db(sqlx::Error::RowNotFound),
            AppError::Database(_)
        ));
    }
}
