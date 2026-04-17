//! # Document DTOs
//!
//! Data Transfer Objects for document management operations.
//!
//! These DTOs represent documents and their metadata across application boundaries.

use serde::{Deserialize, Serialize};

/// Document representation.
///
/// Flat DTO for document metadata and statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentDto {
    /// Document identifier
    pub id: String,

    /// Document title or filename
    pub title: String,

    /// File path
    pub path: String,

    /// Creation timestamp (ISO 8601)
    pub created_at: String,

    /// Last modified timestamp (ISO 8601)
    pub modified_at: String,

    /// Number of chunks in this document
    pub chunk_count: usize,

    /// File size in bytes
    pub file_size: Option<i64>,

    /// MIME type
    pub mime_type: Option<String>,

    /// Checksum for content verification
    pub checksum: Option<String>,

    /// Tags assigned to this document
    pub tags: Vec<String>,
}

/// Request to get a document by ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetDocumentRequestDto {
    /// Document ID
    pub id: String,
}

/// Request to list documents with optional filtering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListDocumentsRequestDto {
    /// Optional limit on number of documents
    pub limit: Option<usize>,

    /// Optional offset for pagination
    pub offset: Option<usize>,

    /// Optional tag filter (only documents with these tags)
    pub tags: Option<Vec<String>>,

    /// Optional search in title/path
    pub search: Option<String>,
}

/// Response listing documents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListDocumentsResponseDto {
    /// List of documents
    pub documents: Vec<DocumentDto>,

    /// Total count (before pagination)
    pub total: usize,

    /// Number of documents returned
    pub count: usize,
}

/// Request to delete a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteDocumentRequestDto {
    /// Document ID to delete
    pub id: String,

    /// Whether to also delete the file from disk
    pub delete_file: bool,
}

/// Response from deleting a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteDocumentResponseDto {
    /// Status message
    pub status: String,

    /// Number of chunks deleted
    pub chunks_deleted: usize,
}

/// Document statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentStatsDto {
    /// Total number of documents
    pub total_documents: usize,

    /// Total number of chunks across all documents
    pub total_chunks: usize,

    /// Total size of all documents in bytes
    pub total_size_bytes: i64,

    /// Average chunks per document
    pub avg_chunks_per_document: f32,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_dto_serialization() {
        let doc = DocumentDto {
            id: "doc-123".to_string(),
            title: "Test Document".to_string(),
            path: "/path/to/doc.txt".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            modified_at: "2024-01-02T00:00:00Z".to_string(),
            chunk_count: 5,
            file_size: Some(1024),
            mime_type: Some("text/plain".to_string()),
            checksum: Some("abc123".to_string()),
            tags: vec!["work".to_string(), "important".to_string()],
        };

        let json = serde_json::to_string(&doc).unwrap();
        let deserialized: DocumentDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, "doc-123");
        assert_eq!(deserialized.chunk_count, 5);
    }

    #[test]
    fn test_list_documents_request_dto_serialization() {
        let request = ListDocumentsRequestDto {
            limit: Some(10),
            offset: Some(0),
            tags: Some(vec!["work".to_string()]),
            search: Some("test".to_string()),
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: ListDocumentsRequestDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.limit, Some(10));
        assert_eq!(deserialized.offset, Some(0));
    }

    #[test]
    fn test_list_documents_response_dto_serialization() {
        let response = ListDocumentsResponseDto {
            documents: vec![DocumentDto {
                id: "doc-1".to_string(),
                title: "Document 1".to_string(),
                path: "/docs/1.txt".to_string(),
                created_at: "2024-01-01T00:00:00Z".to_string(),
                modified_at: "2024-01-01T00:00:00Z".to_string(),
                chunk_count: 3,
                file_size: Some(512),
                mime_type: Some("text/plain".to_string()),
                checksum: None,
                tags: vec![],
            }],
            total: 1,
            count: 1,
        };

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: ListDocumentsResponseDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.total, 1);
        assert_eq!(deserialized.count, 1);
    }

    #[test]
    fn test_document_stats_dto_serialization() {
        let stats = DocumentStatsDto {
            total_documents: 100,
            total_chunks: 500,
            total_size_bytes: 1024000,
            avg_chunks_per_document: 5.0,
        };

        let json = serde_json::to_string(&stats).unwrap();
        let deserialized: DocumentStatsDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.total_documents, 100);
        assert_eq!(deserialized.avg_chunks_per_document, 5.0);
    }
}
