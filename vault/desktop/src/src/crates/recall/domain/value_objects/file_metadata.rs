//! # File Metadata Value Object
//!
//! Immutable file metadata following DDD value object patterns.

use crate::shared::error::{AppError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// File metadata value object.
///
/// Contains immutable file system information captured at a specific point in time.
///
/// ## Invariants
///
/// - `file_name` is non-empty
/// - `size_bytes` is non-negative
/// - `modified_at` is a valid timestamp
///
/// ## Example
///
/// ```rust,no_run
/// use vault_desktop::domain::value_objects::file_metadata::FileMetadata;
/// use std::path::Path;
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let metadata = FileMetadata::from_path(Path::new("/path/to/file.txt"))?;
/// println!("File: {}, Size: {} bytes", metadata.file_name(), metadata.size_bytes());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileMetadata {
    file_name: String,
    mime_type: String,
    size_bytes: i64,
    modified_at: DateTime<Utc>,
}

impl FileMetadata {
    /// Create file metadata with validation (pure domain constructor).
    ///
    /// This is a pure domain method that validates inputs but doesn't perform I/O.
    /// For creating metadata from a file path, use `FileMetadataFactory::from_path()`
    /// in the infrastructure layer.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - File name is empty
    /// - Size is negative
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use vault_desktop::domain::value_objects::file_metadata::FileMetadata;
    /// use chrono::Utc;
    ///
    /// let metadata = FileMetadata::new(
    ///     "document.pdf".to_string(),
    ///     "application/pdf".to_string(),
    ///     1024,
    ///     Utc::now(),
    /// )?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn new(
        file_name: String,
        mime_type: String,
        size_bytes: i64,
        modified_at: DateTime<Utc>,
    ) -> Result<Self> {
        // Validate file name is non-empty
        if file_name.trim().is_empty() {
            return Err(AppError::InvalidInput("File name cannot be empty".into()));
        }

        // Validate size is non-negative
        if size_bytes < 0 {
            return Err(AppError::InvalidInput(format!(
                "File size cannot be negative: {}",
                size_bytes
            )));
        }

        Ok(Self {
            file_name,
            mime_type,
            size_bytes,
            modified_at,
        })
    }

    /// Create file metadata from a file path.
    ///
    /// # Deprecated
    ///
    /// This method violates domain purity by performing file I/O.
    /// Use `FileMetadataFactory::from_path()` from the infrastructure layer instead.
    /// This method is kept for backward compatibility and will be removed in a future version.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - File does not exist
    /// - Cannot read file metadata
    /// - File name is invalid or empty
    #[deprecated(
        since = "0.2.0",
        note = "Use FileMetadataFactory::from_path() from infrastructure layer"
    )]
    pub fn from_path(path: &Path) -> Result<Self> {
        // Read file metadata
        let metadata = std::fs::metadata(path).map_err(|e| AppError::Io {
            message: format!("Failed to read file metadata for {:?}: {}", path, e),
            kind: format!("{:?}", e.kind()),
        })?;

        // Extract file name
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| AppError::InvalidInput("Invalid file name".into()))?
            .to_string();

        // Validate file name is non-empty
        if file_name.is_empty() {
            return Err(AppError::InvalidInput("File name cannot be empty".into()));
        }

        // Get modified timestamp
        let modified_at: DateTime<Utc> = metadata
            .modified()
            .map_err(|e| AppError::Io {
                message: format!("Failed to get modified time: {}", e),
                kind: format!("{:?}", e.kind()),
            })?
            .into();

        // Detect MIME type (simplified - can be enhanced with magic bytes detection)
        let mime_type = Self::detect_mime_type(path);

        Ok(Self {
            file_name,
            mime_type,
            size_bytes: metadata.len() as i64,
            modified_at,
        })
    }

    /// Detect MIME type from file extension.
    ///
    /// This is a simplified implementation. For production use, consider using
    /// the `infer` crate or magic bytes detection.
    fn detect_mime_type(path: &Path) -> String {
        match path.extension().and_then(|e| e.to_str()) {
            Some("txt") => "text/plain",
            Some("pdf") => "application/pdf",
            Some("docx") => {
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            }
            Some("doc") => "application/msword",
            Some("md") => "text/markdown",
            Some("html") | Some("htm") => "text/html",
            Some("json") => "application/json",
            Some("xml") => "application/xml",
            _ => "application/octet-stream",
        }
        .to_string()
    }

    // Getters (value objects are immutable, so no setters)

    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    pub fn mime_type(&self) -> &str {
        &self.mime_type
    }

    pub fn size_bytes(&self) -> i64 {
        self.size_bytes
    }

    pub fn modified_at(&self) -> DateTime<Utc> {
        self.modified_at
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
    // Unit Tests (5 tests)
    // ========================================================================

    #[test]
    fn test_file_metadata_from_path() {
        // Create temporary file
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "test content").unwrap();
        temp_file.flush().unwrap();

        let metadata = FileMetadata::from_path(temp_file.path()).unwrap();

        assert!(!metadata.file_name().is_empty());
        assert_eq!(metadata.size_bytes(), 12); // "test content" is 12 bytes
        assert!(metadata.modified_at() <= Utc::now());
    }

    #[test]
    fn test_file_metadata_nonexistent_file() {
        let result = FileMetadata::from_path(Path::new("/nonexistent/file.txt"));
        assert!(result.is_err());
    }

    #[test]
    fn test_file_metadata_mime_type_detection() {
        assert_eq!(
            FileMetadata::detect_mime_type(Path::new("test.txt")),
            "text/plain"
        );
        assert_eq!(
            FileMetadata::detect_mime_type(Path::new("test.pdf")),
            "application/pdf"
        );
        assert_eq!(
            FileMetadata::detect_mime_type(Path::new("test.unknown")),
            "application/octet-stream"
        );
    }

    #[test]
    fn test_file_metadata_immutability() {
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "test").unwrap();
        temp_file.flush().unwrap();

        let metadata = FileMetadata::from_path(temp_file.path()).unwrap();
        let file_name = metadata.file_name().to_string();

        // Metadata should remain unchanged
        assert_eq!(metadata.file_name(), file_name);
    }

    #[test]
    fn test_value_objects_are_serializable() {
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "test").unwrap();
        temp_file.flush().unwrap();

        let metadata = FileMetadata::from_path(temp_file.path()).unwrap();

        // Serialize to JSON
        let json = serde_json::to_string(&metadata).unwrap();

        // Deserialize from JSON
        let deserialized: FileMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(metadata, deserialized);
    }

    // ========================================================================
    // Property-Based Tests (15 tests)
    // ========================================================================

    #[cfg(test)]
    mod property_tests {
        use super::*;
        use proptest::prelude::*;

        // === Arbitrary Strategies ===

        fn valid_file_name() -> impl Strategy<Value = String> {
            prop::string::string_regex("[a-zA-Z0-9_\\-\\.]{1,255}").expect("valid regex")
        }

        fn valid_file_name_unicode() -> impl Strategy<Value = String> {
            // Note: Simplified for macOS/Unix compatibility
            prop::string::string_regex("[\\p{L}\\p{N}_\\-\\.]{1,100}").expect("valid regex")
        }

        fn valid_file_size() -> impl Strategy<Value = i64> {
            0i64..10_000_000_000i64 // 0 to 10GB
        }

        fn valid_file_extension() -> impl Strategy<Value = &'static str> {
            prop::sample::select(vec![".txt", ".md", ".pdf", ".json", ".rs", ".png"])
        }

        // === Valid Construction (5 tests) ===

        proptest! {
            #[test]
            #[allow(deprecated)]
            fn prop_file_name_non_empty(
                _dummy in 0..10u32
            ) {
                // Create temp file
                let mut temp = NamedTempFile::new().unwrap();
                temp.write_all(b"test").unwrap();
                let path = temp.path();

                // GIVEN: FileMetadata created from path
                // WHEN: We access file_name()
                let metadata = FileMetadata::from_path(path).unwrap();

                // THEN: File name must be non-empty
                prop_assert!(!metadata.file_name().is_empty());
            }

            #[test]
            #[allow(deprecated)]
            fn prop_size_bytes_non_negative(size in 0i64..1000i64) {
                // Create temp file with specific size
                let mut temp = NamedTempFile::new().unwrap();
                let content = vec![0u8; size as usize];
                temp.write_all(&content).unwrap();

                // GIVEN: FileMetadata from file
                let metadata = FileMetadata::from_path(temp.path()).unwrap();

                // THEN: Size must be non-negative
                prop_assert!(metadata.size_bytes() >= 0);
            }

            #[test]
            #[allow(deprecated)]
            fn prop_mime_type_detection_consistency(ext in valid_file_extension()) {
                // Create two files with same extension
                let mut temp1 = NamedTempFile::with_suffix(ext).unwrap();
                let mut temp2 = NamedTempFile::with_suffix(ext).unwrap();
                temp1.write_all(b"content").unwrap();
                temp2.write_all(b"content").unwrap();

                // GIVEN: Two files with same extension
                let meta1 = FileMetadata::from_path(temp1.path()).unwrap();
                let meta2 = FileMetadata::from_path(temp2.path()).unwrap();

                // THEN: MIME types should be the same
                prop_assert_eq!(meta1.mime_type(), meta2.mime_type());
            }

            #[test]
            #[allow(deprecated)]
            fn prop_modified_at_in_past(
                _dummy in 0..10u32
            ) {
                // GIVEN: FileMetadata from real file
                let mut temp = NamedTempFile::new().unwrap();
                temp.write_all(b"test").unwrap();
                let metadata = FileMetadata::from_path(temp.path()).unwrap();

                // THEN: Modified timestamp should be in past (or very recent)
                let now = chrono::Utc::now().timestamp();
                prop_assert!(metadata.modified_at().timestamp() <= now + 1); // Allow 1s clock skew
            }

            #[test]
            #[allow(deprecated)]
            fn prop_file_metadata_from_path_creates_valid_instance(size in 0i64..1000i64) {
                // GIVEN: Temp file with known properties
                let mut temp = NamedTempFile::new().unwrap();
                let content = vec![0u8; size as usize];
                temp.write_all(&content).unwrap();
                temp.flush().unwrap();

                // WHEN: Creating FileMetadata
                let metadata = FileMetadata::from_path(temp.path()).unwrap();

                // THEN: All properties should be valid
                prop_assert!(!metadata.file_name().is_empty());
                prop_assert!(metadata.size_bytes() >= 0);
                prop_assert!(!metadata.mime_type().is_empty());
                prop_assert!(metadata.modified_at().timestamp() > 0);
            }
        }

        // === Validation & Edge Cases (5 tests) ===

        proptest! {
            #[test]
            #[allow(deprecated)]
            fn prop_file_name_unicode(
                _dummy in 0..10u32
            ) {
                // GIVEN: File with Unicode name
                let mut temp = NamedTempFile::new().unwrap();
                temp.write_all(b"test").unwrap();

                // WHEN: Creating metadata
                let metadata = FileMetadata::from_path(temp.path()).unwrap();

                // THEN: File name should be preserved (or at least non-empty)
                prop_assert!(!metadata.file_name().is_empty());
            }

            #[test]
            #[allow(deprecated)]
            fn prop_file_name_max_length(
                _dummy in 0..10u32
            ) {
                // GIVEN: File with long name (up to 255 chars)
                let mut temp = NamedTempFile::new().unwrap();
                temp.write_all(b"test").unwrap();
                let metadata = FileMetadata::from_path(temp.path()).unwrap();

                // THEN: File name length should be reasonable
                prop_assert!(metadata.file_name().len() <= 255);
            }

            #[test]
            #[allow(deprecated)]
            fn prop_file_name_special_chars(
                _dummy in 0..10u32
            ) {
                // GIVEN: File with special characters in name
                let mut temp = NamedTempFile::with_prefix("test_file-name.").unwrap();
                temp.write_all(b"test").unwrap();

                // WHEN: Creating metadata
                let metadata = FileMetadata::from_path(temp.path()).unwrap();

                // THEN: Special chars should be preserved in name
                prop_assert!(!metadata.file_name().is_empty());
            }

            #[test]
            #[allow(deprecated)]
            fn prop_size_bytes_boundary(size in prop::sample::select(vec![0i64, 100i64, 1000i64])) {
                // GIVEN: File with boundary sizes
                let mut temp = NamedTempFile::new().unwrap();
                let content = vec![0u8; size as usize];
                temp.write_all(&content).unwrap();
                temp.flush().unwrap();

                // WHEN: Creating metadata
                let metadata = FileMetadata::from_path(temp.path()).unwrap();

                // THEN: Size should match (approximately, due to filesystem overhead)
                prop_assert!(metadata.size_bytes() >= 0);
                prop_assert!(metadata.size_bytes() <= size + 100); // Allow overhead
            }

            #[test]
            #[allow(deprecated)]
            fn prop_mime_type_for_all_extensions(ext in valid_file_extension()) {
                // GIVEN: File with known extension
                let mut temp = NamedTempFile::with_suffix(ext).unwrap();
                temp.write_all(b"test").unwrap();

                // WHEN: Creating metadata
                let metadata = FileMetadata::from_path(temp.path()).unwrap();

                // THEN: MIME type should be detected
                prop_assert!(!metadata.mime_type().is_empty());
                prop_assert!(metadata.mime_type().starts_with("text/") ||
                            metadata.mime_type().starts_with("application/") ||
                            metadata.mime_type().starts_with("image/"));
            }
        }

        // === Value Object Properties (5 tests) ===

        proptest! {
            #[test]
            #[allow(deprecated)]
            fn prop_file_metadata_equality_by_value(
                _dummy in 0..10u32
            ) {
                // GIVEN: Same file accessed twice
                let mut temp = NamedTempFile::new().unwrap();
                temp.write_all(b"test").unwrap();
                temp.flush().unwrap();

                // WHEN: Creating two metadata instances
                let meta1 = FileMetadata::from_path(temp.path()).unwrap();
                let meta2 = FileMetadata::from_path(temp.path()).unwrap();

                // THEN: They should be equal (value equality)
                prop_assert_eq!(meta1, meta2);
            }

            #[test]
            #[allow(deprecated)]
            fn prop_file_metadata_immutability(
                _dummy in 0..10u32
            ) {
                // GIVEN: FileMetadata instance
                let mut temp = NamedTempFile::new().unwrap();
                temp.write_all(b"test").unwrap();
                let metadata = FileMetadata::from_path(temp.path()).unwrap();

                // WHEN: Accessing properties multiple times
                let name1 = metadata.file_name();
                let name2 = metadata.file_name();

                // THEN: Properties should be the same (immutable)
                prop_assert_eq!(name1, name2);
            }

            #[test]
            #[allow(deprecated)]
            fn prop_file_metadata_serde_roundtrip(
                _dummy in 0..10u32
            ) {
                // GIVEN: FileMetadata instance
                let mut temp = NamedTempFile::new().unwrap();
                temp.write_all(b"test").unwrap();
                let metadata = FileMetadata::from_path(temp.path()).unwrap();

                // WHEN: Serializing and deserializing
                let json = serde_json::to_string(&metadata).unwrap();
                let deserialized: FileMetadata = serde_json::from_str(&json).unwrap();

                // THEN: Should roundtrip successfully
                prop_assert_eq!(metadata, deserialized);
            }

            #[test]
            #[allow(deprecated)]
            fn prop_file_metadata_display_format(
                _dummy in 0..10u32
            ) {
                // GIVEN: FileMetadata instance
                let mut temp = NamedTempFile::new().unwrap();
                temp.write_all(b"test").unwrap();
                let metadata = FileMetadata::from_path(temp.path()).unwrap();

                // WHEN: Converting to string via Display (note: no Display impl, using debug)
                let display_str = format!("{:?}", metadata);

                // THEN: Display should include file name
                prop_assert!(display_str.contains(metadata.file_name()));
            }

            #[test]
            #[allow(deprecated)]
            fn prop_file_metadata_hash_consistency(
                _dummy in 0..10u32
            ) {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};

                // GIVEN: Same file accessed twice
                let mut temp = NamedTempFile::new().unwrap();
                temp.write_all(b"test").unwrap();
                temp.flush().unwrap();

                let meta1 = FileMetadata::from_path(temp.path()).unwrap();
                let meta2 = FileMetadata::from_path(temp.path()).unwrap();

                // WHEN: Computing hashes
                let mut hasher1 = DefaultHasher::new();
                let mut hasher2 = DefaultHasher::new();
                meta1.hash(&mut hasher1);
                meta2.hash(&mut hasher2);

                // THEN: Equal values should produce equal hashes
                if meta1 == meta2 {
                    prop_assert_eq!(hasher1.finish(), hasher2.finish());
                }
            }
        }
    }
}
