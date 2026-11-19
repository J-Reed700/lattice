//! Small persistence helpers every Learning repository shares.
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::hash::sha256_hex;
use serde::{de::DeserializeOwned, Serialize};

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
}
