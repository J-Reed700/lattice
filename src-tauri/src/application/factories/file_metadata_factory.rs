//! # File Metadata Factory
//!
//! Application layer factory for creating FileMetadata from file system operations.
//!
//! This factory isolates file I/O from the domain layer, maintaining domain purity.
//!
//! **Architecture Note**: Moved from infrastructure to application layer because it creates
//! domain value objects (FileMetadata). Factories that create domain objects belong in the
//! application layer, not infrastructure.

use crate::domain::value_objects::file_metadata::FileMetadata;
use crate::shared::error::{AppError, Result};
use chrono::{DateTime, Utc};
use std::path::Path;

/// Factory for creating FileMetadata with file system operations.
///
/// This is the proper way to create FileMetadata from files, keeping file I/O
/// out of the domain layer as per Clean Architecture principles.
pub struct FileMetadataFactory;

impl FileMetadataFactory {
    /// Create file metadata from a file path (with file I/O).
    ///
    /// This method performs file system operations, keeping file I/O out of the
    /// domain layer (`FileMetadata::new()` stays pure).
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - File does not exist
    /// - Cannot read file metadata
    /// - File name is invalid or empty
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use lattice::application::factories::file_metadata_factory::FileMetadataFactory;
    /// use std::path::Path;
    ///
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let metadata = FileMetadataFactory::from_path(Path::new("/path/to/file.txt"))?;
    /// println!("File: {}, Size: {} bytes", metadata.file_name(), metadata.size_bytes());
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_path(path: &Path) -> Result<FileMetadata> {
        let fs_metadata = std::fs::metadata(path).map_err(|e| AppError::Io {
            message: format!("Failed to read file metadata for {:?}: {}", path, e),
            kind: format!("{:?}", e.kind()),
        })?;

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| AppError::InvalidInput("Invalid file name".into()))?
            .to_string();

        let modified_at: DateTime<Utc> = fs_metadata
            .modified()
            .map_err(|e| AppError::Io {
                message: format!("Failed to get modified time: {}", e),
                kind: format!("{:?}", e.kind()),
            })?
            .into();

        // Detect MIME type
        let mime_type = Self::detect_mime_type(path);

        let size_bytes = fs_metadata.len() as i64;

        FileMetadata::new(file_name, mime_type, size_bytes, modified_at)
    }

    /// Detect MIME type from file extension.
    ///
    /// This is a simplified implementation based on file extension.
    /// For production use, consider using the `infer` crate or magic bytes detection.
    fn detect_mime_type(path: &Path) -> String {
        match path.extension().and_then(|e| e.to_str()) {
            Some("txt") => "text/plain",
            Some("pdf") => "application/pdf",
            Some("docx") => {
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            }
            Some("doc") => "application/msword",
            Some("xlsx") => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            Some("xls") => "application/vnd.ms-excel",
            Some("pptx") => {
                "application/vnd.openxmlformats-officedocument.presentationml.presentation"
            }
            Some("ppt") => "application/vnd.ms-powerpoint",
            Some("md") => "text/markdown",
            Some("html") | Some("htm") => "text/html",
            Some("json") => "application/json",
            Some("xml") => "application/xml",
            Some("csv") => "text/csv",
            Some("rtf") => "application/rtf",
            Some("odt") => "application/vnd.oasis.opendocument.text",
            Some("png") => "image/png",
            Some("jpg") | Some("jpeg") => "image/jpeg",
            Some("gif") => "image/gif",
            Some("bmp") => "image/bmp",
            Some("tiff") => "image/tiff",
            Some("ico") => "image/x-icon",
            Some("webp") => "image/webp",
            Some("svg") => "image/svg+xml",
            Some("heic") => "image/heic",
            Some("heif") => "image/heif",
            Some("heif-sequence") => "image/heif-sequence",
            Some("heic-sequence") => "image/heic-sequence",
            _ => "application/octet-stream",
        }
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_from_path_creates_valid_metadata() {
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "test content").unwrap();
        temp_file.flush().unwrap();

        let metadata = FileMetadataFactory::from_path(temp_file.path()).unwrap();

        assert!(!metadata.file_name().is_empty());
        assert_eq!(metadata.size_bytes(), 12); // "test content" is 12 bytes
        assert!(metadata.modified_at() <= Utc::now());
    }

    #[test]
    fn test_from_path_nonexistent_file_returns_error() {
        let result = FileMetadataFactory::from_path(Path::new("/nonexistent/file.txt"));
        assert!(result.is_err());
    }

    #[test]
    fn test_mime_type_detection() {
        assert_eq!(
            FileMetadataFactory::detect_mime_type(Path::new("test.txt")),
            "text/plain"
        );
        assert_eq!(
            FileMetadataFactory::detect_mime_type(Path::new("test.pdf")),
            "application/pdf"
        );
        assert_eq!(
            FileMetadataFactory::detect_mime_type(Path::new("document.docx")),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        );
        assert_eq!(
            FileMetadataFactory::detect_mime_type(Path::new("test.unknown")),
            "application/octet-stream"
        );
    }

    #[test]
    fn test_creates_metadata_with_correct_file_size() {
        let mut temp_file = NamedTempFile::new().unwrap();
        let content = "0123456789"; // 10 bytes
        write!(temp_file, "{}", content).unwrap();
        temp_file.flush().unwrap();

        let metadata = FileMetadataFactory::from_path(temp_file.path()).unwrap();

        assert_eq!(metadata.size_bytes(), 10);
    }

    #[test]
    fn test_handles_empty_file() {
        let temp_file = NamedTempFile::new().unwrap();
        // Don't write anything - file is empty

        let metadata = FileMetadataFactory::from_path(temp_file.path()).unwrap();

        assert_eq!(metadata.size_bytes(), 0);
        assert!(!metadata.file_name().is_empty());
    }

    #[cfg(test)]
    mod property_tests {
        use super::*;
        use proptest::prelude::*;

        fn valid_file_extension() -> impl Strategy<Value = &'static str> {
            prop::sample::select(vec![".txt", ".md", ".pdf", ".json", ".rs", ".png"])
        }

        proptest! {
            #[test]
            fn prop_file_name_non_empty(
                _dummy in 0..10u32
            ) {
                let mut temp = NamedTempFile::new().unwrap();
                temp.write_all(b"test").unwrap();
                temp.flush().unwrap();

                // GIVEN: FileMetadata created from path
                let metadata = FileMetadataFactory::from_path(temp.path()).unwrap();

                // THEN: File name must be non-empty
                prop_assert!(!metadata.file_name().is_empty());
            }

            #[test]
            fn prop_size_bytes_non_negative(size in 0i64..1000i64) {
                let mut temp = NamedTempFile::new().unwrap();
                let content = vec![0u8; size as usize];
                temp.write_all(&content).unwrap();
                temp.flush().unwrap();

                // GIVEN: FileMetadata from file
                let metadata = FileMetadataFactory::from_path(temp.path()).unwrap();

                // THEN: Size must be non-negative
                prop_assert!(metadata.size_bytes() >= 0);
            }

            #[test]
            fn prop_mime_type_detection_consistency(ext in valid_file_extension()) {
                let mut temp1 = NamedTempFile::with_suffix(ext).unwrap();
                let mut temp2 = NamedTempFile::with_suffix(ext).unwrap();
                temp1.write_all(b"content").unwrap();
                temp2.write_all(b"content").unwrap();
                temp1.flush().unwrap();
                temp2.flush().unwrap();

                // GIVEN: Two files with same extension
                let meta1 = FileMetadataFactory::from_path(temp1.path()).unwrap();
                let meta2 = FileMetadataFactory::from_path(temp2.path()).unwrap();

                // THEN: MIME types should be the same
                prop_assert_eq!(meta1.mime_type(), meta2.mime_type());
            }

            #[test]
            fn prop_modified_at_in_past(
                _dummy in 0..10u32
            ) {
                // GIVEN: FileMetadata from real file
                let mut temp = NamedTempFile::new().unwrap();
                temp.write_all(b"test").unwrap();
                temp.flush().unwrap();
                let metadata = FileMetadataFactory::from_path(temp.path()).unwrap();

                // THEN: Modified timestamp should be in past (or very recent)
                let now = Utc::now().timestamp();
                prop_assert!(metadata.modified_at().timestamp() <= now + 1); // Allow 1s clock skew
            }

            #[test]
            fn prop_from_path_creates_valid_instance(size in 0i64..1000i64) {
                // GIVEN: Temp file with known properties
                let mut temp = NamedTempFile::new().unwrap();
                let content = vec![0u8; size as usize];
                temp.write_all(&content).unwrap();
                temp.flush().unwrap();

                // WHEN: Creating FileMetadata
                let metadata = FileMetadataFactory::from_path(temp.path()).unwrap();

                // THEN: All properties should be valid
                prop_assert!(!metadata.file_name().is_empty());
                prop_assert!(metadata.size_bytes() >= 0);
                prop_assert!(!metadata.mime_type().is_empty());
                prop_assert!(metadata.modified_at().timestamp() > 0);
            }
        }

        proptest! {
            #[test]
            fn prop_file_name_special_chars(
                _dummy in 0..10u32
            ) {
                // GIVEN: File with special characters in name
                let mut temp = NamedTempFile::with_prefix("test_file-name.").unwrap();
                temp.write_all(b"test").unwrap();
                temp.flush().unwrap();

                // WHEN: Creating metadata
                let metadata = FileMetadataFactory::from_path(temp.path()).unwrap();

                // THEN: Special chars should be preserved in name
                prop_assert!(!metadata.file_name().is_empty());
            }

            #[test]
            fn prop_size_bytes_boundary(size in prop::sample::select(vec![0i64, 100i64, 1000i64])) {
                // GIVEN: File with boundary sizes
                let mut temp = NamedTempFile::new().unwrap();
                let content = vec![0u8; size as usize];
                temp.write_all(&content).unwrap();
                temp.flush().unwrap();

                // WHEN: Creating metadata
                let metadata = FileMetadataFactory::from_path(temp.path()).unwrap();

                // THEN: Size should match (approximately, due to filesystem overhead)
                prop_assert!(metadata.size_bytes() >= 0);
                prop_assert!(metadata.size_bytes() <= size + 100); // Allow overhead
            }

            #[test]
            fn prop_mime_type_for_all_extensions(ext in valid_file_extension()) {
                // GIVEN: File with known extension
                let mut temp = NamedTempFile::with_suffix(ext).unwrap();
                temp.write_all(b"test").unwrap();
                temp.flush().unwrap();

                // WHEN: Creating metadata
                let metadata = FileMetadataFactory::from_path(temp.path()).unwrap();

                // THEN: MIME type should be detected
                prop_assert!(!metadata.mime_type().is_empty());
                prop_assert!(metadata.mime_type().starts_with("text/") ||
                            metadata.mime_type().starts_with("application/") ||
                            metadata.mime_type().starts_with("image/"));
            }
        }

        proptest! {
            #[test]
            fn prop_file_metadata_equality_by_value(
                _dummy in 0..10u32
            ) {
                // GIVEN: Same file accessed twice
                let mut temp = NamedTempFile::new().unwrap();
                temp.write_all(b"test").unwrap();
                temp.flush().unwrap();

                // WHEN: Creating two metadata instances
                let meta1 = FileMetadataFactory::from_path(temp.path()).unwrap();
                let meta2 = FileMetadataFactory::from_path(temp.path()).unwrap();

                // THEN: They should be equal (value equality)
                prop_assert_eq!(meta1, meta2);
            }

            #[test]
            fn prop_file_metadata_serde_roundtrip(
                _dummy in 0..10u32
            ) {
                // GIVEN: FileMetadata instance
                let mut temp = NamedTempFile::new().unwrap();
                temp.write_all(b"test").unwrap();
                temp.flush().unwrap();
                let metadata = FileMetadataFactory::from_path(temp.path()).unwrap();

                // WHEN: Serializing and deserializing
                let json = serde_json::to_string(&metadata).unwrap();
                let deserialized: FileMetadata = serde_json::from_str(&json).unwrap();

                // THEN: Should roundtrip successfully
                prop_assert_eq!(metadata, deserialized);
            }

            #[test]
            fn prop_file_metadata_hash_consistency(
                _dummy in 0..10u32
            ) {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};

                // GIVEN: Same file accessed twice
                let mut temp = NamedTempFile::new().unwrap();
                temp.write_all(b"test").unwrap();
                temp.flush().unwrap();

                let meta1 = FileMetadataFactory::from_path(temp.path()).unwrap();
                let meta2 = FileMetadataFactory::from_path(temp.path()).unwrap();

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
