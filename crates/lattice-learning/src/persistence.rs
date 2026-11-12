//! Small persistence helpers every Learning repository shares.
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::hash::sha256_hex;
use lattice_core::shared::persistence::{PersistenceError, PersistenceErrorKind};
use serde::{de::DeserializeOwned, Serialize};

/// Map a SQLite error. A pool timeout means the database is busy rather than
/// broken, so it is reported as a retryable condition.
pub fn db(error: impl Into<PersistenceError>) -> AppError {
    let error = error.into();
    match error.kind() {
        PersistenceErrorKind::Busy => AppError::DatabaseBusy(
            "The learning database is temporarily busy. Saved work is retained.".into(),
        ),
        _ => AppError::Database(error.to_string()),
    }
}

/// Wall-clock time in Unix milliseconds, the unit every Learning table stores.
pub fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// SHA-256 of a value's JSON encoding, as lowercase hex.
pub(crate) fn hash<T: Serialize + ?Sized>(value: &T) -> Result<String> {
    let bytes =
        serde_json::to_vec(value).map_err(|error| AppError::Serialization(error.to_string()))?;
    Ok(sha256_hex(bytes))
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
        assert_eq!(hash(&value)?, sha256_hex(&encode(&value)?));
        assert_eq!(sha256_hex("abc").len(), 64);
        Ok(())
    }

    #[test]
    fn pool_timeouts_are_reported_as_a_busy_database() {
        assert!(matches!(
            db(sqlx::Error::PoolTimedOut),
            AppError::DatabaseBusy(_)
        ));
        assert!(matches!(
            db(PersistenceError::from(sqlx::Error::PoolTimedOut)),
            AppError::DatabaseBusy(message) if message.contains("temporarily busy")
        ));
    }

    #[test]
    fn other_errors_keep_their_message() {
        let expected = sqlx::Error::RowNotFound.to_string();
        assert!(matches!(
            db(sqlx::Error::RowNotFound),
            AppError::Database(message) if message == expected
        ));
        assert!(matches!(
            db(PersistenceError::from(sqlx::Error::PoolClosed)),
            AppError::Database(message) if message == sqlx::Error::PoolClosed.to_string()
        ));
    }
}
