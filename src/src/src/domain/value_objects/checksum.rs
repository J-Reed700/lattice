//! # Checksum Value Object
//!
//! Immutable checksum value object following DDD patterns.
//!
//! ## Pure Domain Model
//!
//! This value object contains only the checksum value and validation logic.
//! **The actual computation of checksums is handled by the application layer.**
//!
//! Domain Layer Principle: Zero file I/O, zero external cryptography dependencies.
//! For checksum computation, use `ChecksumFactory` from the application layer.

use crate::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};

/// File content checksum value object.
///
/// Provides content-based verification using checksums (typically SHA-256).
///
/// ## Invariants
///
/// - Checksum is a valid hex string
/// - Checksum is non-empty
/// - For SHA-256: exactly 64 hex characters
///
/// ## Example
///
/// ```rust,no_run
/// use lattice::domain::value_objects::checksum::Checksum;
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Create from existing hash
/// let checksum = Checksum::new("a".repeat(64))?;
/// println!("Checksum: {}", checksum.as_str());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Checksum(String);

impl Checksum {
    /// Create checksum from existing hash string (pure domain constructor).
    ///
    /// This is the primary pure domain constructor. For computing checksums from
    /// files or content, use `ChecksumFactory` from the application layer.
    ///
    /// # Errors
    ///
    /// Returns an error if the hash string is empty or invalid format.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use lattice::domain::value_objects::checksum::Checksum;
    ///
    /// let hash = "a".repeat(64); // Valid SHA-256 length
    /// let checksum = Checksum::new(hash)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn new(hash: String) -> Result<Self> {
        if hash.is_empty() {
            return Err(AppError::InvalidInput("Checksum cannot be empty".into()));
        }

        // Basic validation: SHA-256 is 64 hex characters
        if hash.len() != 64 {
            return Err(AppError::InvalidInput(format!(
                "Invalid SHA-256 checksum length: expected 64, got {}",
                hash.len()
            )));
        }

        if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(AppError::InvalidInput(
                "Checksum must contain only hex characters".into(),
            ));
        }

        Ok(Self(hash))
    }

    /// Get the checksum as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Check if this checksum matches another.
    ///
    /// This is a pure domain method comparing two checksums.
    pub fn matches(&self, other: &Checksum) -> bool {
        self.0 == other.0
    }
}

impl From<String> for Checksum {
    fn from(s: String) -> Self {
        // Note: This implementation doesn't validate. Use Checksum::new() for validation.
        Checksum(s)
    }
}

impl std::fmt::Display for Checksum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checksum_new_valid() {
        let hash = "a".repeat(64);
        let checksum = Checksum::new(hash.clone()).unwrap();
        assert_eq!(checksum.as_str(), hash);
    }

    #[test]
    fn test_checksum_new_empty() {
        let result = Checksum::new("".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_checksum_new_wrong_length() {
        let result = Checksum::new("abc123".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_checksum_new_invalid_chars() {
        let invalid = "g".repeat(64); // 'g' is not a hex digit
        let result = Checksum::new(invalid);
        assert!(result.is_err());
    }

    #[test]
    fn test_checksum_display() {
        let hash = "abc".repeat(21) + "d"; // 64 chars
        let checksum = Checksum::new(hash.clone()).unwrap();
        assert_eq!(checksum.to_string(), hash);
    }

    #[test]
    fn test_checksum_equality() {
        let hash = "a".repeat(64);
        let checksum1 = Checksum::new(hash.clone()).unwrap();
        let checksum2 = Checksum::new(hash).unwrap();

        assert_eq!(checksum1, checksum2);
        assert!(checksum1.matches(&checksum2));
    }

    #[test]
    fn test_checksum_matches_rejects_different_value() {
        let checksum1 = Checksum::new("a".repeat(64)).unwrap();
        let checksum2 = Checksum::new("b".repeat(64)).unwrap();

        assert_ne!(checksum1, checksum2);
        assert!(!checksum1.matches(&checksum2));
    }

    #[test]
    fn test_checksum_serialization() {
        let hash = "b".repeat(64);
        let checksum = Checksum::new(hash).unwrap();

        // Serialize to JSON
        let json = serde_json::to_string(&checksum).unwrap();

        // Deserialize from JSON
        let deserialized: Checksum = serde_json::from_str(&json).unwrap();

        assert_eq!(checksum, deserialized);
    }

    #[cfg(test)]
    mod property_tests {
        use super::*;
        use proptest::prelude::*;

        fn valid_checksum_hex() -> impl Strategy<Value = String> {
            prop::string::string_regex("[a-f0-9]{64}").expect("valid regex")
        }

        fn valid_checksum_hex_mixed_case() -> impl Strategy<Value = String> {
            prop::string::string_regex("[a-fA-F0-9]{64}").expect("valid regex")
        }

        fn invalid_checksum_length() -> impl Strategy<Value = String> {
            prop_oneof![
                prop::string::string_regex("[a-f0-9]{0,63}").expect("valid regex"), // Too short
                prop::string::string_regex("[a-f0-9]{65,128}").expect("valid regex"), // Too long
            ]
        }

        fn invalid_checksum_chars() -> impl Strategy<Value = String> {
            prop::string::string_regex("[g-z]{64}").expect("valid regex") // Invalid hex chars
        }

        proptest! {
            #[test]
            fn prop_checksum_from_hex_valid(hex in valid_checksum_hex()) {
                // GIVEN: Valid 64-char hex string
                // WHEN: Creating checksum from hex
                let result = Checksum::new(hex.clone());

                // THEN: Should succeed
                prop_assert!(result.is_ok());
                if let Ok(checksum) = result {
                    prop_assert_eq!(checksum.as_str(), hex);
                }
            }

            #[test]
            fn prop_checksum_hex_only(hex in valid_checksum_hex()) {
                // GIVEN: Checksum built from a valid hex string
                let checksum = Checksum::new(hex).expect("valid hex checksum");

                // THEN: All characters should be hex
                prop_assert!(checksum.as_str().chars().all(|c| c.is_ascii_hexdigit()));
            }

            #[test]
            fn prop_checksum_length_64(hex in valid_checksum_hex()) {
                // GIVEN: Checksum built from a valid hex string
                let checksum = Checksum::new(hex).expect("valid hex checksum");

                // THEN: Length must always be 64 (SHA-256)
                prop_assert_eq!(checksum.as_str().len(), 64);
            }
        }

        proptest! {
            #[test]
            fn prop_checksum_empty_rejected(
                _dummy in 0..10u32
            ) {
                // GIVEN: Empty string
                let empty = "";

                // WHEN: Attempting to create checksum
                let result = Checksum::new(empty.to_string());

                // THEN: Should be rejected
                prop_assert!(result.is_err());
            }

            #[test]
            fn prop_checksum_wrong_length_rejected(wrong_len in invalid_checksum_length()) {
                // GIVEN: Hex string with wrong length
                // WHEN: Attempting to create checksum
                let result = Checksum::new(wrong_len);

                // THEN: Should be rejected
                prop_assert!(result.is_err());
            }

            #[test]
            fn prop_checksum_invalid_chars_rejected(invalid in invalid_checksum_chars()) {
                // GIVEN: String with invalid hex characters
                // WHEN: Attempting to create checksum
                let result = Checksum::new(invalid);

                // THEN: Should be rejected
                prop_assert!(result.is_err());
            }

            #[test]
            fn prop_checksum_case_preserved(hex in valid_checksum_hex_mixed_case()) {
                // GIVEN: Mixed-case hex string
                // WHEN: Creating checksum
                let result = Checksum::new(hex.clone());

                // THEN: If valid, case should be preserved as-is
                if hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
                    if let Ok(checksum) = result {
                        // Checksum stores the string as-is without normalization
                        prop_assert_eq!(checksum.as_str(), hex);
                    }
                }
            }
        }

        proptest! {
            #[test]
            fn prop_checksum_equality_by_value(hex in valid_checksum_hex()) {
                // GIVEN: Same hex string used twice
                let checksum1 = Checksum::new(hex.clone()).expect("valid hex checksum");
                let checksum2 = Checksum::new(hex).expect("valid hex checksum");

                // THEN: Checksums should be equal (value equality)
                prop_assert_eq!(checksum1, checksum2);
            }

            #[test]
            fn prop_checksum_matches_reflexive(hex in valid_checksum_hex()) {
                // GIVEN: Two checksums built from the same hex string
                let checksum1 = Checksum::new(hex.clone()).expect("valid hex checksum");
                let checksum2 = Checksum::new(hex).expect("valid hex checksum");

                // THEN: Checksum should match the checksum from the same value
                prop_assert!(checksum1.matches(&checksum2));
            }

            #[test]
            fn prop_checksum_serde_roundtrip(hex in valid_checksum_hex()) {
                // GIVEN: Checksum instance
                let checksum = Checksum::new(hex).expect("valid hex checksum");

                // WHEN: Serializing and deserializing
                let json = serde_json::to_string(&checksum).unwrap();
                let deserialized: Checksum = serde_json::from_str(&json).unwrap();

                // THEN: Should roundtrip successfully
                prop_assert_eq!(checksum, deserialized);
            }
        }
    }
}
