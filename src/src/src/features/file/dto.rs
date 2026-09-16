//! # File DTOs
//!
//! Data Transfer Objects for file management operations.
//!
//! These DTOs represent file metadata and content across application boundaries.

use crate::application::services::FileType;
use serde::{Deserialize, Serialize};

/// File metadata representation.
///
/// Flat DTO for file metadata including size, modified time, permissions, and MIME type.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FileMetadataDto {
    /// File name
    pub file_name: String,

    /// MIME type
    pub mime_type: String,

    /// File size in bytes
    pub size_bytes: i64,

    /// Last modified timestamp (ISO 8601)
    pub modified_at: String,

    /// Whether the file is readable
    pub is_readable: bool,

    /// Whether the file is writable
    pub is_writable: bool,

    /// Full file path
    pub path: String,
}

/// File content representation.
///
/// DTO for file content operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContentDto {
    /// File path
    pub path: String,

    /// File content as UTF-8 string
    pub content: String,

    /// File size in bytes
    pub size_bytes: i64,

    /// Content encoding (typically "UTF-8")
    pub encoding: String,
}

/// Request to open a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenFileRequestDto {
    /// File path to open
    pub path: String,
}

/// Request to open a file by document ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenFileByIdRequestDto {
    /// Document ID
    pub document_id: String,
}

/// Request to get file metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFileMetadataRequestDto {
    /// File path
    pub path: String,
}

/// Request to show a file in its folder.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowInFolderRequestDto {
    /// File path to reveal
    pub path: String,
}

/// Request to read file content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadFileContentRequestDto {
    /// File path to read
    pub path: String,
}

/// Request to read file bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadFileBytesRequestDto {
    /// File path to read
    pub path: String,
}

/// Request to get file path by document ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFilePathByIdRequestDto {
    /// Document ID
    pub document_id: String,
}

/// Response for file path lookup.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFilePathByIdResponseDto {
    /// File path
    pub path: String,

    /// Whether file exists on disk
    pub exists: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateFileMetadataRequestDto {
    pub document_id: String,
    pub tags: Option<Vec<String>>,
}

/// Generic success response for file operations.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct FileOperationSuccessDto {
    /// Status message
    pub status: String,
}

/// Response from open_file command.
///
/// Indicates what action was taken when opening a file:
/// - `render_internal`: File will be rendered in the app (e.g., HTML web articles)
/// - `opened_external`: File was opened in the default system application
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OpenFileResponseDto {
    /// Action taken: "render_internal" or "opened_external"
    pub action: String,

    /// File type detected (enum: WebArticleHtml, Pdf, Image, Text, Unknown)
    pub file_type: FileType,

    /// Path to content (HTML file for internal rendering, original file for external)
    pub content_path: String,

    /// Title (for web articles)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl OpenFileResponseDto {
    /// Create response for internal HTML rendering.
    ///
    /// Used when opening web article HTML files that should be rendered in the app.
    pub fn render_internal(html_path: String, title: String) -> Self {
        Self {
            action: "render_internal".to_string(),
            file_type: FileType::WebArticleHtml,
            content_path: html_path,
            title: Some(title),
        }
    }

    /// Create response for internal rendering with specific file type.
    ///
    /// Used when opening files that can be rendered internally (PDF, images, text, etc).
    pub fn render_internal_with_type(
        content_path: String,
        file_type: FileType,
        title: Option<String>,
    ) -> Self {
        Self {
            action: "render_internal".to_string(),
            file_type,
            content_path,
            title,
        }
    }

    /// Create response for external opening.
    ///
    /// Used when opening files in the default system application.
    pub fn opened_external(path: String, file_type: FileType) -> Self {
        Self {
            action: "opened_external".to_string(),
            file_type,
            content_path: path,
            title: None,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_metadata_dto_serialization() {
        let metadata = FileMetadataDto {
            file_name: "document.txt".to_string(),
            mime_type: "text/plain".to_string(),
            size_bytes: 1024,
            modified_at: "2024-01-01T00:00:00Z".to_string(),
            is_readable: true,
            is_writable: true,
            path: "/path/to/document.txt".to_string(),
        };

        let json = serde_json::to_string(&metadata).unwrap();
        let deserialized: FileMetadataDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.file_name, "document.txt");
        assert_eq!(deserialized.size_bytes, 1024);
    }

    #[test]
    fn test_file_content_dto_serialization() {
        let content = FileContentDto {
            path: "/path/to/file.txt".to_string(),
            content: "Hello, World!".to_string(),
            size_bytes: 13,
            encoding: "UTF-8".to_string(),
        };

        let json = serde_json::to_string(&content).unwrap();
        let deserialized: FileContentDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.content, "Hello, World!");
        assert_eq!(deserialized.encoding, "UTF-8");
    }

    #[test]
    fn test_open_file_request_dto_serialization() {
        let request = OpenFileRequestDto {
            path: "/path/to/file.txt".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: OpenFileRequestDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.path, "/path/to/file.txt");
    }

    #[test]
    fn test_get_file_path_by_id_response_dto_serialization() {
        let response = GetFilePathByIdResponseDto {
            path: "/lattice/documents/file.txt".to_string(),
            exists: true,
        };

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: GetFilePathByIdResponseDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.path, "/lattice/documents/file.txt");
        assert!(deserialized.exists);
    }

    #[test]
    fn test_open_file_response_dto_render_internal() {
        let response = OpenFileResponseDto::render_internal(
            "/path/to/article.html".to_string(),
            "My Article Title".to_string(),
        );

        assert_eq!(response.action, "render_internal");
        assert_eq!(response.file_type, FileType::WebArticleHtml);
        assert_eq!(response.content_path, "/path/to/article.html");
        assert_eq!(response.title, Some("My Article Title".to_string()));

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: OpenFileResponseDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.action, "render_internal");
        assert_eq!(deserialized.file_type, FileType::WebArticleHtml);
        assert_eq!(deserialized.title, Some("My Article Title".to_string()));
    }

    #[test]
    fn test_open_file_response_dto_render_internal_with_type() {
        let response = OpenFileResponseDto::render_internal_with_type(
            "/path/to/document.pdf".to_string(),
            FileType::Pdf,
            Some("My PDF".to_string()),
        );

        assert_eq!(response.action, "render_internal");
        assert_eq!(response.file_type, FileType::Pdf);
        assert_eq!(response.content_path, "/path/to/document.pdf");
        assert_eq!(response.title, Some("My PDF".to_string()));
    }

    #[test]
    fn test_open_file_response_dto_opened_external() {
        let response = OpenFileResponseDto::opened_external(
            "/path/to/document.pdf".to_string(),
            FileType::Pdf,
        );

        assert_eq!(response.action, "opened_external");
        assert_eq!(response.file_type, FileType::Pdf);
        assert_eq!(response.content_path, "/path/to/document.pdf");
        assert_eq!(response.title, None);

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: OpenFileResponseDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.action, "opened_external");
        assert_eq!(deserialized.file_type, FileType::Pdf);
        assert_eq!(deserialized.title, None);
    }

    #[test]
    fn test_open_file_response_dto_serialization_format() {
        let response = OpenFileResponseDto::render_internal_with_type(
            "/test.pdf".to_string(),
            FileType::Pdf,
            None,
        );

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"fileType\":\"pdf\""));
        assert!(json.contains("\"action\":\"render_internal\""));
    }
}
