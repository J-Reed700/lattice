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
/// in the infrastructure layer as per Clean Architecture principles.
pub struct FileMetadataFactory;

impl FileMetadataFactory {
    /// Create file metadata from a file path (with file I/O).
    ///
    /// This method performs file system operations and should be used instead of
    /// the deprecated `FileMetadata::from_path()` method.
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
    /// use vault_desktop::application::factories::file_metadata_factory::FileMetadataFactory;
    /// use std::path::Path;
    ///
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let metadata = FileMetadataFactory::from_path(Path::new("/path/to/file.txt"))?;
    /// println!("File: {}, Size: {} bytes", metadata.file_name(), metadata.size_bytes());
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_path(path: &Path) -> Result<FileMetadata> {
        // Read file metadata from file system
        let fs_metadata = std::fs::metadata(path).map_err(|e| AppError::Io {
            message: format!("Failed to read file metadata for {:?}: {}", path, e),
            kind: format!("{:?}", e.kind()),
        })?;

        // Extract file name
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| AppError::InvalidInput("Invalid file name".into()))?
            .to_string();

        // Get modified timestamp
        let modified_at: DateTime<Utc> = fs_metadata
            .modified()
            .map_err(|e| AppError::Io {
                message: format!("Failed to get modified time: {}", e),
                kind: format!("{:?}", e.kind()),
            })?
            .into();

        // Detect MIME type
        let mime_type = Self::detect_mime_type(path);

        // Get file size
        let size_bytes = fs_metadata.len() as i64;

        // Use pure domain constructor
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_from_path_creates_valid_metadata() {
        // Create temporary file
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
}
