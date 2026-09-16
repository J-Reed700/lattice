//! # File Metadata Value Object
//!
//! Immutable file metadata following DDD value object patterns.

use crate::shared::error::{AppError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
/// use lattice::domain::value_objects::file_metadata::FileMetadata;
/// use chrono::Utc;
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Pure domain construction; to read metadata off disk use
/// // `FileMetadataFactory::from_path()` from the application layer.
/// let metadata = FileMetadata::new(
///     "file.txt".to_string(),
///     "text/plain".to_string(),
///     1024,
///     Utc::now(),
/// )?;
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
    /// in the application layer.
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
    /// use lattice::domain::value_objects::file_metadata::FileMetadata;
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
        if file_name.trim().is_empty() {
            return Err(AppError::InvalidInput("File name cannot be empty".into()));
        }

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

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata(file_name: &str, mime_type: &str, size_bytes: i64) -> FileMetadata {
        FileMetadata::new(
            file_name.to_string(),
            mime_type.to_string(),
            size_bytes,
            Utc::now(),
        )
        .unwrap()
    }

    #[test]
    fn test_file_metadata_new_valid() {
        let meta = metadata("test.txt", "text/plain", 12);

        assert_eq!(meta.file_name(), "test.txt");
        assert_eq!(meta.mime_type(), "text/plain");
        assert_eq!(meta.size_bytes(), 12);
        assert!(meta.modified_at() <= Utc::now());
    }

    #[test]
    fn test_file_metadata_new_rejects_empty_name() {
        let result = FileMetadata::new("   ".to_string(), "text/plain".to_string(), 10, Utc::now());
        assert!(result.is_err());
    }

    #[test]
    fn test_file_metadata_new_rejects_negative_size() {
        let result = FileMetadata::new(
            "test.txt".to_string(),
            "text/plain".to_string(),
            -1,
            Utc::now(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_file_metadata_immutability() {
        let meta = metadata("test.txt", "text/plain", 4);
        let file_name = meta.file_name().to_string();

        // Metadata should remain unchanged
        assert_eq!(meta.file_name(), file_name);
    }

    #[test]
    fn test_value_objects_are_serializable() {
        let meta = metadata("test.txt", "text/plain", 4);

        // Serialize to JSON
        let json = serde_json::to_string(&meta).unwrap();

        // Deserialize from JSON
        let deserialized: FileMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(meta, deserialized);
    }

    #[cfg(test)]
    mod property_tests {
        use super::*;
        use proptest::prelude::*;

        fn valid_file_name() -> impl Strategy<Value = String> {
            prop::string::string_regex("[A-Za-z0-9_.-]{1,64}").expect("valid regex")
        }

        fn valid_mime_type() -> impl Strategy<Value = &'static str> {
            prop::sample::select(vec![
                "text/plain",
                "text/markdown",
                "application/pdf",
                "application/json",
                "application/octet-stream",
                "image/png",
            ])
        }

        proptest! {
            #[test]
            fn prop_file_name_non_empty(name in valid_file_name()) {
                // GIVEN: FileMetadata built from a non-empty name
                let meta = FileMetadata::new(
                    name,
                    "text/plain".to_string(),
                    0,
                    Utc::now(),
                ).expect("valid metadata");

                // THEN: File name must be non-empty
                prop_assert!(!meta.file_name().is_empty());
            }

            #[test]
            fn prop_size_bytes_non_negative(size in 0i64..1000i64) {
                // GIVEN: FileMetadata built with a non-negative size
                let meta = FileMetadata::new(
                    "test.txt".to_string(),
                    "text/plain".to_string(),
                    size,
                    Utc::now(),
                ).expect("valid metadata");

                // THEN: Size must be non-negative and preserved
                prop_assert!(meta.size_bytes() >= 0);
                prop_assert_eq!(meta.size_bytes(), size);
            }

            #[test]
            fn prop_negative_size_rejected(size in -1000i64..0i64) {
                // GIVEN: A negative size
                let result = FileMetadata::new(
                    "test.txt".to_string(),
                    "text/plain".to_string(),
                    size,
                    Utc::now(),
                );

                // THEN: Construction must be rejected
                prop_assert!(result.is_err());
            }

            #[test]
            fn prop_mime_type_preserved(mime in valid_mime_type()) {
                // GIVEN: FileMetadata built with a known MIME type
                let meta = FileMetadata::new(
                    "test.txt".to_string(),
                    mime.to_string(),
                    0,
                    Utc::now(),
                ).expect("valid metadata");

                // THEN: MIME type should be preserved verbatim
                prop_assert_eq!(meta.mime_type(), mime);
            }

            #[test]
            fn prop_file_name_max_length(name in valid_file_name()) {
                // GIVEN: FileMetadata built from a bounded name
                let meta = FileMetadata::new(
                    name,
                    "text/plain".to_string(),
                    0,
                    Utc::now(),
                ).expect("valid metadata");

                // THEN: File name length should be reasonable
                prop_assert!(meta.file_name().len() <= 255);
            }
        }

        proptest! {
            #[test]
            fn prop_file_metadata_equality_by_value(name in valid_file_name()) {
                // GIVEN: Two instances built from identical field values
                let modified_at = Utc::now();
                let meta1 = FileMetadata::new(
                    name.clone(),
                    "text/plain".to_string(),
                    42,
                    modified_at,
                ).expect("valid metadata");
                let meta2 = FileMetadata::new(
                    name,
                    "text/plain".to_string(),
                    42,
                    modified_at,
                ).expect("valid metadata");

                // THEN: They should be equal (value equality)
                prop_assert_eq!(meta1, meta2);
            }

            #[test]
            fn prop_file_metadata_immutability(name in valid_file_name()) {
                // GIVEN: FileMetadata instance
                let meta = FileMetadata::new(
                    name,
                    "text/plain".to_string(),
                    0,
                    Utc::now(),
                ).expect("valid metadata");

                // WHEN: Accessing properties multiple times
                let name1 = meta.file_name();
                let name2 = meta.file_name();

                // THEN: Properties should be the same (immutable)
                prop_assert_eq!(name1, name2);
            }

            #[test]
            fn prop_file_metadata_serde_roundtrip(name in valid_file_name()) {
                // GIVEN: FileMetadata instance
                let meta = FileMetadata::new(
                    name,
                    "text/plain".to_string(),
                    7,
                    Utc::now(),
                ).expect("valid metadata");

                // WHEN: Serializing and deserializing
                let json = serde_json::to_string(&meta).unwrap();
                let deserialized: FileMetadata = serde_json::from_str(&json).unwrap();

                // THEN: Should roundtrip successfully
                prop_assert_eq!(meta, deserialized);
            }

            #[test]
            fn prop_file_metadata_display_format(name in valid_file_name()) {
                // GIVEN: FileMetadata instance
                let meta = FileMetadata::new(
                    name,
                    "text/plain".to_string(),
                    0,
                    Utc::now(),
                ).expect("valid metadata");

                // WHEN: Converting to string via Debug (note: no Display impl)
                let display_str = format!("{:?}", meta);

                // THEN: Debug output should include file name
                prop_assert!(display_str.contains(meta.file_name()));
            }

            #[test]
            fn prop_file_metadata_hash_consistency(name in valid_file_name()) {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};

                // GIVEN: Two instances built from identical field values
                let modified_at = Utc::now();
                let meta1 = FileMetadata::new(
                    name.clone(),
                    "text/plain".to_string(),
                    42,
                    modified_at,
                ).expect("valid metadata");
                let meta2 = FileMetadata::new(
                    name,
                    "text/plain".to_string(),
                    42,
                    modified_at,
                ).expect("valid metadata");

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
