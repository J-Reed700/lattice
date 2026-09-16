//! # Checksum Factory
//!
//! Application layer factory for computing checksums with file system and cryptography operations.
//!
//! This factory isolates file I/O and cryptography dependencies from the domain layer,
//! maintaining domain purity as per Clean Architecture principles.
//!
//! **Architecture Note**: Moved from infrastructure to application layer because it creates
//! domain value objects (Checksum). Factories that create domain objects belong in the
//! application layer, not infrastructure.

use crate::domain::value_objects::checksum::Checksum;
use crate::shared::error::{AppError, Result};
use sha2::{Digest, Sha256};
use std::path::Path;

/// Factory for computing checksums with infrastructure dependencies.
///
/// This is the proper way to compute checksums from files or content, keeping
/// cryptography and file I/O in the infrastructure layer.
pub struct ChecksumFactory;

impl ChecksumFactory {
    /// Compute SHA-256 checksum from file path (with file I/O).
    ///
    /// This method performs file system operations, keeping file I/O out of the
    /// domain layer.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use lattice::application::factories::checksum_factory::ChecksumFactory;
    /// use std::path::Path;
    ///
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let checksum = ChecksumFactory::from_path(Path::new("/path/to/file.txt"))?;
    /// println!("Checksum: {}", checksum.as_str());
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_path(path: &Path) -> Result<Checksum> {
        let content = std::fs::read(path).map_err(|e| AppError::Io {
            message: format!("Failed to read file for checksum: {}", e),
            kind: format!("{:?}", e.kind()),
        })?;

        Self::from_bytes(&content)
    }

    /// Compute SHA-256 checksum from byte content.
    ///
    /// This method uses the sha2 cryptography library, keeping cryptography out of
    /// the domain layer.
    ///
    /// # Errors
    ///
    /// Returns an error if checksum validation fails (should not happen with SHA-256).
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use lattice::application::factories::checksum_factory::ChecksumFactory;
    ///
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let content = b"Hello, world!";
    /// let checksum = ChecksumFactory::from_bytes(content)?;
    /// println!("Checksum: {}", checksum.as_str());
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_bytes(content: &[u8]) -> Result<Checksum> {
        let hash = Sha256::digest(content);
        let checksum_str = format!("{:x}", hash);

        Checksum::new(checksum_str)
    }

    /// Verify that content matches the expected checksum.
    ///
    /// This method computes the checksum of the provided content and compares it
    /// with the expected checksum.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use lattice::application::factories::checksum_factory::ChecksumFactory;
    /// use lattice::domain::value_objects::checksum::Checksum;
    ///
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let content = b"test content";
    /// let expected = ChecksumFactory::from_bytes(content)?;
    ///
    /// assert!(ChecksumFactory::verify(&expected, content));
    /// assert!(!ChecksumFactory::verify(&expected, b"different content"));
    /// # Ok(())
    /// # }
    /// ```
    pub fn verify(expected: &Checksum, content: &[u8]) -> bool {
        match Self::from_bytes(content) {
            Ok(computed) => expected.matches(&computed),
            Err(_) => false,
        }
    }

    /// Verify that a file's content matches the expected checksum.
    ///
    /// This method reads the file and compares its checksum with the expected value.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use lattice::application::factories::checksum_factory::ChecksumFactory;
    /// use std::path::Path;
    ///
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let path = Path::new("/path/to/file.txt");
    /// let expected = ChecksumFactory::from_path(path)?;
    ///
    /// let is_valid = ChecksumFactory::verify_file(&expected, path)?;
    /// assert!(is_valid);
    /// # Ok(())
    /// # }
    /// ```
    pub fn verify_file(expected: &Checksum, path: &Path) -> Result<bool> {
        let computed = Self::from_path(path)?;
        Ok(expected.matches(&computed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_from_bytes_creates_valid_checksum() {
        let content = b"hello world";
        let checksum = ChecksumFactory::from_bytes(content).unwrap();

        assert_eq!(checksum.as_str().len(), 64);
        assert!(checksum.as_str().chars().all(|c| c.is_ascii_hexdigit()));

        // SHA-256 of "hello world"
        let expected = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        assert_eq!(checksum.as_str(), expected);
    }

    #[test]
    fn test_from_path_computes_correct_checksum() {
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "hello world").unwrap();
        temp_file.flush().unwrap();

        let checksum = ChecksumFactory::from_path(temp_file.path()).unwrap();

        // SHA-256 of "hello world"
        let expected = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        assert_eq!(checksum.as_str(), expected);
    }

    #[test]
    fn test_from_path_nonexistent_file_returns_error() {
        let result = ChecksumFactory::from_path(Path::new("/nonexistent/file.txt"));
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_with_matching_content() {
        let content = b"test content";
        let checksum = ChecksumFactory::from_bytes(content).unwrap();

        assert!(ChecksumFactory::verify(&checksum, content));
    }

    #[test]
    fn test_verify_with_different_content() {
        let original = b"original content";
        let checksum = ChecksumFactory::from_bytes(original).unwrap();

        assert!(!ChecksumFactory::verify(&checksum, b"different content"));
    }

    #[test]
    fn test_verify_file_with_matching_content() {
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "file content").unwrap();
        temp_file.flush().unwrap();

        let checksum = ChecksumFactory::from_path(temp_file.path()).unwrap();

        let is_valid = ChecksumFactory::verify_file(&checksum, temp_file.path()).unwrap();
        assert!(is_valid);
    }

    #[test]
    fn test_verify_file_after_modification() {
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "original").unwrap();
        temp_file.flush().unwrap();

        let original_checksum = ChecksumFactory::from_path(temp_file.path()).unwrap();

        // Modify file
        write!(temp_file, "modified").unwrap();
        temp_file.flush().unwrap();

        let is_valid = ChecksumFactory::verify_file(&original_checksum, temp_file.path()).unwrap();
        assert!(!is_valid);
    }

    #[test]
    fn test_same_content_produces_same_checksum() {
        let content1 = b"identical content";
        let content2 = b"identical content";

        let checksum1 = ChecksumFactory::from_bytes(content1).unwrap();
        let checksum2 = ChecksumFactory::from_bytes(content2).unwrap();

        assert!(checksum1.matches(&checksum2));
    }

    #[test]
    fn test_different_content_produces_different_checksum() {
        let checksum1 = ChecksumFactory::from_bytes(b"content A").unwrap();
        let checksum2 = ChecksumFactory::from_bytes(b"content B").unwrap();

        assert!(!checksum1.matches(&checksum2));
    }

    #[test]
    fn test_empty_content_produces_valid_checksum() {
        let checksum = ChecksumFactory::from_bytes(b"").unwrap();

        // SHA-256 of empty string
        let expected = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert_eq!(checksum.as_str(), expected);
    }

    #[cfg(test)]
    mod property_tests {
        use super::*;
        use proptest::prelude::*;

        fn valid_file_content() -> impl Strategy<Value = Vec<u8>> {
            prop::collection::vec(any::<u8>(), 0..1000) // 0-1KB for test speed
        }

        proptest! {
            #[test]
            fn prop_from_bytes_creates_valid_hash(content in valid_file_content()) {
                // GIVEN: File content
                // WHEN: Computing a checksum from bytes
                let checksum = ChecksumFactory::from_bytes(&content).unwrap();

                // THEN: Should produce a valid 64-char hex hash
                prop_assert_eq!(checksum.as_str().len(), 64);
                prop_assert!(checksum.as_str().chars().all(|c| c.is_ascii_hexdigit()));
            }

            #[test]
            fn prop_from_bytes_is_deterministic(content in valid_file_content()) {
                // GIVEN: The same content hashed twice
                let checksum1 = ChecksumFactory::from_bytes(&content).unwrap();
                let checksum2 = ChecksumFactory::from_bytes(&content).unwrap();

                // THEN: Checksums should be equal (value equality)
                prop_assert_eq!(&checksum1, &checksum2);
                prop_assert!(checksum1.matches(&checksum2));
            }

            #[test]
            fn prop_from_bytes_serde_roundtrip(content in valid_file_content()) {
                // GIVEN: A computed checksum
                let checksum = ChecksumFactory::from_bytes(&content).unwrap();

                // WHEN: Serializing and deserializing
                let json = serde_json::to_string(&checksum).unwrap();
                let deserialized: Checksum = serde_json::from_str(&json).unwrap();

                // THEN: Should roundtrip successfully
                prop_assert_eq!(checksum, deserialized);
            }

            #[test]
            fn prop_verify_consistent(content in valid_file_content()) {
                // GIVEN: Checksum computed from content
                let checksum = ChecksumFactory::from_bytes(&content).unwrap();

                // WHEN: Verifying the same content
                let result = ChecksumFactory::verify(&checksum, &content);

                // THEN: Should verify successfully
                prop_assert!(result);
            }

            #[test]
            fn prop_from_path_matches_from_bytes(content in valid_file_content()) {
                // GIVEN: A file containing the content
                let mut temp_file = NamedTempFile::new().unwrap();
                temp_file.write_all(&content).unwrap();
                temp_file.flush().unwrap();

                // WHEN: Computing checksums from the path and from the bytes
                let from_path = ChecksumFactory::from_path(temp_file.path()).unwrap();
                let from_bytes = ChecksumFactory::from_bytes(&content).unwrap();

                // THEN: They should agree
                prop_assert_eq!(from_path, from_bytes);
            }
        }
    }
}
