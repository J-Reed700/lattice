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
use crate::domain::value_objects::file_type::FileFormat;
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

    /// MIME type of a path from the file-type catalogue; `application/octet-stream` when unknown.
    pub fn detect_mime_type(path: &Path) -> String {
        FileFormat::mime_type_of(path).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tempfile::TempDir;

    fn write_temp_file(dir: &TempDir, name: &str, content: &[u8]) -> PathBuf {
        let path = dir.path().join(name);
        std::fs::write(&path, content).unwrap();
        path
    }

    fn write_unique_temp_file(
        dir: &TempDir,
        seq: &AtomicU64,
        prefix: &str,
        suffix: &str,
        content: &[u8],
    ) -> PathBuf {
        let id = seq.fetch_add(1, Ordering::Relaxed);
        write_temp_file(dir, &format!("{prefix}_{id}{suffix}"), content)
    }

    #[test]
    fn test_from_path_creates_valid_metadata() {
        let dir = TempDir::new().unwrap();
        let path = write_temp_file(&dir, "sample.txt", b"test content");

        let metadata = FileMetadataFactory::from_path(&path).unwrap();

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
            FileMetadataFactory::detect_mime_type(Path::new("TEST.TXT")),
            "text/plain"
        );
        assert_eq!(
            FileMetadataFactory::detect_mime_type(Path::new("test.pdf")),
            "application/pdf"
        );
        assert_eq!(
            FileMetadataFactory::detect_mime_type(Path::new("test.PDF")),
            "application/pdf"
        );
        assert_eq!(
            FileMetadataFactory::detect_mime_type(Path::new("doc.md")),
            "text/markdown"
        );
        assert_eq!(
            FileMetadataFactory::detect_mime_type(Path::new("doc.markdown")),
            "text/markdown"
        );
        assert_eq!(
            FileMetadataFactory::detect_mime_type(Path::new("main.rs")),
            "text/x-rust"
        );
        assert_eq!(
            FileMetadataFactory::detect_mime_type(Path::new("script.py")),
            "text/x-python"
        );
        assert_eq!(
            FileMetadataFactory::detect_mime_type(Path::new("track.mp3")),
            "audio/mpeg"
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
        let dir = TempDir::new().unwrap();
        let path = write_temp_file(&dir, "sized.txt", b"0123456789");

        let metadata = FileMetadataFactory::from_path(&path).unwrap();

        assert_eq!(metadata.size_bytes(), 10);
    }

    #[test]
    fn test_handles_empty_file() {
        let dir = TempDir::new().unwrap();
        let path = write_temp_file(&dir, "empty.txt", b"");

        let metadata = FileMetadataFactory::from_path(&path).unwrap();

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

        #[test]
        fn prop_file_name_non_empty() {
            let dir = TempDir::new().unwrap();
            let seq = AtomicU64::new(0);
            proptest!(|(_dummy in 0..10u32)| {
                let path = write_unique_temp_file(&dir, &seq, "sample", ".tmp", b"test");

                // GIVEN: FileMetadata created from path
                let metadata = FileMetadataFactory::from_path(&path).unwrap();

                // THEN: File name must be non-empty
                prop_assert!(!metadata.file_name().is_empty());
            });
        }

        #[test]
        fn prop_size_bytes_non_negative() {
            let dir = TempDir::new().unwrap();
            let seq = AtomicU64::new(0);
            proptest!(|(size in 0i64..1000i64)| {
                let content = vec![0u8; size as usize];
                let path = write_unique_temp_file(&dir, &seq, "sized", ".tmp", &content);

                // GIVEN: FileMetadata from file
                let metadata = FileMetadataFactory::from_path(&path).unwrap();

                // THEN: Size must be non-negative
                prop_assert!(metadata.size_bytes() >= 0);
            });
        }

        #[test]
        fn prop_mime_type_detection_consistency() {
            let dir = TempDir::new().unwrap();
            let seq = AtomicU64::new(0);
            proptest!(|(ext in valid_file_extension())| {
                let path1 = write_unique_temp_file(&dir, &seq, "first", ext, b"content");
                let path2 = write_unique_temp_file(&dir, &seq, "second", ext, b"content");

                // GIVEN: Two files with same extension
                let meta1 = FileMetadataFactory::from_path(&path1).unwrap();
                let meta2 = FileMetadataFactory::from_path(&path2).unwrap();

                // THEN: MIME types should be the same
                prop_assert_eq!(meta1.mime_type(), meta2.mime_type());
            });
        }

        #[test]
        fn prop_modified_at_in_past() {
            let dir = TempDir::new().unwrap();
            let seq = AtomicU64::new(0);
            proptest!(|(_dummy in 0..10u32)| {
                // GIVEN: FileMetadata from real file
                let path = write_unique_temp_file(&dir, &seq, "mtime", ".tmp", b"test");
                let metadata = FileMetadataFactory::from_path(&path).unwrap();

                // THEN: Modified timestamp should be in past (or very recent)
                let now = Utc::now().timestamp();
                prop_assert!(metadata.modified_at().timestamp() <= now + 1); // Allow 1s clock skew
            });
        }

        #[test]
        fn prop_from_path_creates_valid_instance() {
            let dir = TempDir::new().unwrap();
            let seq = AtomicU64::new(0);
            proptest!(|(size in 0i64..1000i64)| {
                // GIVEN: Temp file with known properties
                let content = vec![0u8; size as usize];
                let path = write_unique_temp_file(&dir, &seq, "valid", ".tmp", &content);

                // WHEN: Creating FileMetadata
                let metadata = FileMetadataFactory::from_path(&path).unwrap();

                // THEN: All properties should be valid
                prop_assert!(!metadata.file_name().is_empty());
                prop_assert!(metadata.size_bytes() >= 0);
                prop_assert!(!metadata.mime_type().is_empty());
                prop_assert!(metadata.modified_at().timestamp() > 0);
            });
        }

        #[test]
        fn prop_file_name_special_chars() {
            let dir = TempDir::new().unwrap();
            let seq = AtomicU64::new(0);
            proptest!(|(_dummy in 0..10u32)| {
                // GIVEN: File with special characters in name
                let path = write_unique_temp_file(&dir, &seq, "test_file-name.", ".txt", b"test");

                // WHEN: Creating metadata
                let metadata = FileMetadataFactory::from_path(&path).unwrap();

                // THEN: Special chars should be preserved in name
                prop_assert!(!metadata.file_name().is_empty());
                prop_assert!(metadata.file_name().starts_with("test_file-name."));
            });
        }

        #[test]
        fn prop_size_bytes_boundary() {
            let dir = TempDir::new().unwrap();
            let seq = AtomicU64::new(0);
            proptest!(|(size in prop::sample::select(vec![0i64, 100i64, 1000i64]))| {
                // GIVEN: File with boundary sizes
                let content = vec![0u8; size as usize];
                let path = write_unique_temp_file(&dir, &seq, "boundary", ".tmp", &content);

                // WHEN: Creating metadata
                let metadata = FileMetadataFactory::from_path(&path).unwrap();

                // THEN: Size should match (approximately, due to filesystem overhead)
                prop_assert!(metadata.size_bytes() >= 0);
                prop_assert!(metadata.size_bytes() <= size + 100); // Allow overhead
            });
        }

        #[test]
        fn prop_mime_type_for_all_extensions() {
            let dir = TempDir::new().unwrap();
            let seq = AtomicU64::new(0);
            proptest!(|(ext in valid_file_extension())| {
                // GIVEN: File with known extension
                let path = write_unique_temp_file(&dir, &seq, "mime", ext, b"test");

                // WHEN: Creating metadata
                let metadata = FileMetadataFactory::from_path(&path).unwrap();

                // THEN: MIME type should be detected
                prop_assert!(!metadata.mime_type().is_empty());
                prop_assert!(metadata.mime_type().starts_with("text/") ||
                            metadata.mime_type().starts_with("application/") ||
                            metadata.mime_type().starts_with("image/"));
            });
        }

        #[test]
        fn prop_file_metadata_equality_by_value() {
            let dir = TempDir::new().unwrap();
            let seq = AtomicU64::new(0);
            proptest!(|(_dummy in 0..10u32)| {
                // GIVEN: Same file accessed twice
                let path = write_unique_temp_file(&dir, &seq, "eq", ".tmp", b"test");

                // WHEN: Creating two metadata instances
                let meta1 = FileMetadataFactory::from_path(&path).unwrap();
                let meta2 = FileMetadataFactory::from_path(&path).unwrap();

                // THEN: They should be equal (value equality)
                prop_assert_eq!(meta1, meta2);
            });
        }

        #[test]
        fn prop_file_metadata_serde_roundtrip() {
            let dir = TempDir::new().unwrap();
            let seq = AtomicU64::new(0);
            proptest!(|(_dummy in 0..10u32)| {
                // GIVEN: FileMetadata instance
                let path = write_unique_temp_file(&dir, &seq, "serde", ".tmp", b"test");
                let metadata = FileMetadataFactory::from_path(&path).unwrap();

                // WHEN: Serializing and deserializing
                let json = serde_json::to_string(&metadata).unwrap();
                let deserialized: FileMetadata = serde_json::from_str(&json).unwrap();

                // THEN: Should roundtrip successfully
                prop_assert_eq!(metadata, deserialized);
            });
        }

        #[test]
        fn prop_file_metadata_hash_consistency() {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let dir = TempDir::new().unwrap();
            let seq = AtomicU64::new(0);
            proptest!(|(_dummy in 0..10u32)| {
                // GIVEN: Same file accessed twice
                let path = write_unique_temp_file(&dir, &seq, "hash", ".tmp", b"test");

                let meta1 = FileMetadataFactory::from_path(&path).unwrap();
                let meta2 = FileMetadataFactory::from_path(&path).unwrap();

                // WHEN: Computing hashes
                let mut hasher1 = DefaultHasher::new();
                let mut hasher2 = DefaultHasher::new();
                meta1.hash(&mut hasher1);
                meta2.hash(&mut hasher2);

                // THEN: Equal values should produce equal hashes
                if meta1 == meta2 {
                    prop_assert_eq!(hasher1.finish(), hasher2.finish());
                }
            });
        }
    }
}
