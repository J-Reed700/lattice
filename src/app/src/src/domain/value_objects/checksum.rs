//! # Checksum Value Object
//!
//! Immutable checksum value object following DDD patterns.
//!
//! ## Pure Domain Model
//!
//! This value object contains only the checksum value and validation logic.
//! **The actual computation of checksums is handled by the infrastructure layer.**
//!
//! Domain Layer Principle: Zero file I/O, zero external cryptography dependencies.
//! For checksum computation, use `ChecksumFactory` from the infrastructure layer.

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
/// use vault_desktop::domain::value_objects::checksum::Checksum;
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Create from existing hash
/// let checksum = Checksum::from_string("a".repeat(64))?;
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
    /// files or content, use `ChecksumFactory` from the infrastructure layer.
    ///
    /// # Errors
    ///
    /// Returns an error if the hash string is empty or invalid format.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use vault_desktop::domain::value_objects::checksum::Checksum;
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

        // Validate hex characters
        if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(AppError::InvalidInput(
                "Checksum must contain only hex characters".into(),
            ));
        }

        Ok(Self(hash))
    }

    /// Create checksum from existing hash string.
    ///
    /// # Deprecated
    ///
    /// Use `Checksum::new()` instead for clarity.
    ///
    /// # Errors
    ///
    /// Returns an error if the hash string is empty or invalid format.
    #[deprecated(since = "0.2.0", note = "Use Checksum::new() instead")]
    pub fn from_string(hash: String) -> Result<Self> {
        Self::new(hash)
    }

    /// Compute SHA-256 checksum from file path.
    ///
    /// # Deprecated
    ///
    /// This method violates domain purity by performing file I/O and using external
    /// cryptography dependencies. Use `ChecksumFactory::from_path()` from the
    /// infrastructure layer instead.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read.
    #[deprecated(
        since = "0.2.0",
        note = "Use ChecksumFactory::from_path() from infrastructure layer"
    )]
    pub fn compute(path: &std::path::Path) -> Result<Self> {
        use sha2::{Digest, Sha256};

        // Read file contents
        let content = std::fs::read(path).map_err(|e| AppError::Io {
            message: format!("Failed to read file for checksum: {}", e),
            kind: format!("{:?}", e.kind()),
        })?;

        // Compute SHA-256 hash
        let hash = Sha256::digest(&content);
        let checksum_str = format!("{:x}", hash);

        Ok(Self(checksum_str))
    }

    /// Create checksum from bytes (compute SHA-256).
    ///
    /// # Deprecated
    ///
    /// This method violates domain purity by using external cryptography dependencies (sha2).
    /// Use `ChecksumFactory::from_bytes()` from the infrastructure layer instead.
    ///
    /// This method is kept for backward compatibility and will be removed in a future version.
    #[deprecated(
        since = "0.2.0",
        note = "Use ChecksumFactory::from_bytes() from infrastructure layer"
    )]
    pub fn from_bytes(content: &[u8]) -> Self {
        use sha2::{Digest, Sha256};

        let hash = Sha256::digest(content);
        let checksum_str = format!("{:x}", hash);

        Self(checksum_str)
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

    /// Verify content matches this checksum.
    ///
    /// # Deprecated
    ///
    /// This method depends on `from_bytes()` which uses external cryptography.
    /// Use `ChecksumFactory::verify()` from the infrastructure layer instead.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use vault_desktop::domain::value_objects::checksum::Checksum;
    ///
    /// # #[allow(deprecated)]
    /// let content = b"test content";
    /// let checksum = Checksum::from_bytes(content);
    /// assert!(checksum.verify(content));
    /// ```
    #[deprecated(
        since = "0.2.0",
        note = "Use ChecksumFactory::verify() from infrastructure layer"
    )]
    pub fn verify(&self, content: &[u8]) -> bool {
        #[allow(deprecated)]
        let computed = Self::from_bytes(content);
        self.matches(&computed)
    }
}

impl From<String> for Checksum {
    fn from(s: String) -> Self {
        // Note: This implementation doesn't validate. Use from_string() for validation.
        Checksum(s)
    }
}

impl std::fmt::Display for Checksum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // ========================================================================
    // Unit Tests (13 tests - including deprecated)
    // ========================================================================

    #[test]
    #[allow(deprecated)]
    fn test_checksum_from_bytes() {
        let content = b"hello world";
        let checksum = Checksum::from_bytes(content);

        // Verify checksum format (64 hex characters)
        assert_eq!(checksum.as_str().len(), 64);
        assert!(checksum.as_str().chars().all(|c| c.is_ascii_hexdigit()));

        // SHA-256 of "hello world"
        let expected = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        assert_eq!(checksum.as_str(), expected);
    }

    #[test]
    #[allow(deprecated)]
    fn test_checksum_verify() {
        let content = b"test content";
        let checksum = Checksum::from_bytes(content);

        assert!(checksum.verify(content));
        assert!(!checksum.verify(b"different content"));
    }

    #[test]
    #[allow(deprecated)]
    fn test_checksum_compute() {
        // Create temporary file with known content
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "hello world").unwrap();
        temp_file.flush().unwrap();

        let checksum = Checksum::compute(temp_file.path()).unwrap();

        // Verify checksum format (64 hex characters)
        assert_eq!(checksum.as_str().len(), 64);
        assert!(checksum.as_str().chars().all(|c| c.is_ascii_hexdigit()));

        // SHA-256 of "hello world"
        let expected = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        assert_eq!(checksum.as_str(), expected);
    }

    #[test]
    #[allow(deprecated)]
    fn test_checksum_same_content_same_hash() {
        let content1 = b"identical content";
        let content2 = b"identical content";

        let checksum1 = Checksum::from_bytes(content1);
        let checksum2 = Checksum::from_bytes(content2);

        assert_eq!(checksum1, checksum2);
        assert!(checksum1.matches(&checksum2));
    }

    #[test]
    #[allow(deprecated)]
    fn test_checksum_different_content_different_hash() {
        let checksum1 = Checksum::from_bytes(b"content A");
        let checksum2 = Checksum::from_bytes(b"content B");

        assert_ne!(checksum1, checksum2);
        assert!(!checksum1.matches(&checksum2));
    }

    #[test]
    #[allow(deprecated)]
    fn test_checksum_from_string_valid() {
        let hash = "a".repeat(64);
        let checksum = Checksum::from_string(hash.clone()).unwrap();
        assert_eq!(checksum.as_str(), hash);
    }

    #[test]
    #[allow(deprecated)]
    fn test_checksum_from_string_empty() {
        let result = Checksum::from_string("".to_string());
        assert!(result.is_err());
    }

    #[test]
    #[allow(deprecated)]
    fn test_checksum_from_string_wrong_length() {
        let result = Checksum::from_string("abc123".to_string());
        assert!(result.is_err());
    }

    #[test]
    #[allow(deprecated)]
    fn test_checksum_from_string_invalid_chars() {
        let invalid = "g".repeat(64); // 'g' is not a hex digit
        let result = Checksum::from_string(invalid);
        assert!(result.is_err());
    }

    #[test]
    #[allow(deprecated)]
    fn test_checksum_display() {
        let hash = "abc".repeat(21) + "d"; // 64 chars
        let checksum = Checksum::from_string(hash.clone()).unwrap();
        assert_eq!(checksum.to_string(), hash);
    }

    #[test]
    #[allow(deprecated)]
    fn test_checksum_equality() {
        let hash = "a".repeat(64);
        let checksum1 = Checksum::from_string(hash.clone()).unwrap();
        let checksum2 = Checksum::from_string(hash).unwrap();

        assert_eq!(checksum1, checksum2);
        assert!(checksum1.matches(&checksum2));
    }

    #[test]
    #[allow(deprecated)]
    fn test_checksum_serialization() {
        let hash = "b".repeat(64);
        let checksum = Checksum::from_string(hash).unwrap();

        // Serialize to JSON
        let json = serde_json::to_string(&checksum).unwrap();

        // Deserialize from JSON
        let deserialized: Checksum = serde_json::from_str(&json).unwrap();

        assert_eq!(checksum, deserialized);
    }

    #[test]
    fn test_checksum_new_valid() {
        let hash = "a".repeat(64);
        let checksum = Checksum::new(hash.clone()).unwrap();
        assert_eq!(checksum.as_str(), hash);
    }

    // ========================================================================
    // Property-Based Tests (12 tests)
    // ========================================================================

    #[cfg(test)]
    mod property_tests {
        use super::*;
        use proptest::prelude::*;

        // === Arbitrary Strategies ===

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

        fn valid_file_content() -> impl Strategy<Value = Vec<u8>> {
            prop::collection::vec(any::<u8>(), 0..1000) // 0-1KB for test speed
        }

        // === Valid Construction (4 tests) ===

        proptest! {
            #[test]
            #[allow(deprecated)]
            fn prop_checksum_from_bytes_creates_valid_hash(content in valid_file_content()) {
                // GIVEN: File content
                // WHEN: Creating checksum from bytes
                let checksum = Checksum::from_bytes(&content);

                // THEN: Should produce valid 64-char hex hash
                prop_assert_eq!(checksum.as_str().len(), 64);
                prop_assert!(checksum.as_str().chars().all(|c| c.is_ascii_hexdigit()));
            }

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
            #[allow(deprecated)]
            fn prop_checksum_hex_only(content in valid_file_content()) {
                // GIVEN: Checksum from content
                let checksum = Checksum::from_bytes(&content);

                // THEN: All characters should be hex
                prop_assert!(checksum.as_str().chars().all(|c| c.is_ascii_hexdigit()));
            }

            #[test]
            #[allow(deprecated)]
            fn prop_checksum_length_64(content in valid_file_content()) {
                // GIVEN: Checksum from any content
                let checksum = Checksum::from_bytes(&content);

                // THEN: Length must always be 64 (SHA-256)
                prop_assert_eq!(checksum.as_str().len(), 64);
            }
        }

        // === Validation (4 tests) ===

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

        // === Value Object Properties (4 tests) ===

        proptest! {
            #[test]
            #[allow(deprecated)]
            fn prop_checksum_equality_by_value(content in valid_file_content()) {
                // GIVEN: Same content hashed twice
                let checksum1 = Checksum::from_bytes(&content);
                let checksum2 = Checksum::from_bytes(&content);

                // THEN: Checksums should be equal (value equality)
                prop_assert_eq!(checksum1, checksum2);
            }

            #[test]
            #[allow(deprecated)]
            fn prop_checksum_serde_roundtrip(content in valid_file_content()) {
                // GIVEN: Checksum instance
                let checksum = Checksum::from_bytes(&content);

                // WHEN: Serializing and deserializing
                let json = serde_json::to_string(&checksum).unwrap();
                let deserialized: Checksum = serde_json::from_str(&json).unwrap();

                // THEN: Should roundtrip successfully
                prop_assert_eq!(checksum, deserialized);
            }

            #[test]
            #[allow(deprecated)]
            fn prop_checksum_matches_reflexive(content in valid_file_content()) {
                // GIVEN: Checksum from content
                let checksum1 = Checksum::from_bytes(&content);
                let checksum2 = Checksum::from_bytes(&content);

                // THEN: Checksum should match the checksum from same content
                prop_assert!(checksum1.matches(&checksum2));
            }

            #[test]
            #[allow(deprecated)]
            fn prop_checksum_verify_consistent(content in valid_file_content()) {
                // GIVEN: Checksum from content
                let checksum = Checksum::from_bytes(&content);

                // WHEN: Verifying content
                let result = checksum.verify(&content);

                // THEN: Should verify successfully
                prop_assert_eq!(result, true);
            }
        }
    }
}
