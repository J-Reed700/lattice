//! # Indexing DTOs
//!
//! Data Transfer Objects for document indexing operations.
//!
//! These DTOs handle requests and responses for indexing files and directories.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Request to index a file.
///
/// Contains the file path and chunking configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexFileRequestDto {
    /// Path to the file to index
    pub path: String,

    /// Chunking strategy to use
    pub chunking_strategy: ChunkingStrategyDto,

    /// Optional tags for the document
    #[serde(default)]
    pub tags: Option<Vec<String>>,

    /// Optional metadata key-value pairs
    #[serde(default)]
    pub metadata: Option<HashMap<String, String>>,

    /// Optional target conversation space for document scoping.
    #[serde(default)]
    pub space_id: Option<String>,
}

/// Chunking strategy configuration.
///
/// Defines how to split document content into chunks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ChunkingStrategyDto {
    /// Fixed-size chunks by character count
    FixedSize { size: usize },

    /// Semantic chunking by meaning
    Semantic { max_tokens: usize },

    /// Chunk by paragraph boundaries
    Paragraph,

    /// Chunk by sentence boundaries
    Sentence,
}

/// Response from indexing a file.
///
/// Contains the created document ID and statistics.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct IndexFileResponseDto {
    /// ID of the created document
    pub document_id: String,

    /// Number of chunks created
    pub chunks_created: usize,

    /// Indexing status
    pub status: String,

    /// Optional error message if indexing failed
    pub error: Option<String>,

    /// Library path where file is stored (content-addressed storage)
    pub file_path: String,
}

/// Request to index a directory.
///
/// Recursively indexes all supported files in a directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexDirectoryRequestDto {
    /// Path to the directory to index
    pub path: String,

    /// Whether to index recursively
    pub recursive: bool,

    /// Chunking strategy to use for all files
    pub chunking_strategy: ChunkingStrategyDto,

    /// Optional file extensions to include (e.g., ["txt", "md"])
    pub include_extensions: Option<Vec<String>>,

    /// Optional target conversation space for document scoping.
    #[serde(default)]
    pub space_id: Option<String>,
}

/// Response from indexing a directory.
///
/// Contains statistics about the indexing operation.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct IndexDirectoryResponseDto {
    /// Number of files successfully indexed
    pub files_indexed: usize,

    /// Number of files that failed to index
    pub files_failed: usize,

    /// Total chunks created across all files
    pub total_chunks: usize,

    /// List of document IDs created
    pub document_ids: Vec<String>,

    /// List of errors encountered
    pub errors: Vec<String>,
}

/// Overall indexing statistics.
///
/// Provides aggregate statistics about all indexed documents and chunks.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct IndexingStatsDto {
    /// Total number of indexed documents
    pub indexed_documents: i64,

    /// Total number of chunks across all documents
    pub total_chunks: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_file_request_dto_serialization() {
        let request = IndexFileRequestDto {
            path: "/path/to/file.txt".to_string(),
            chunking_strategy: ChunkingStrategyDto::FixedSize { size: 512 },
            tags: Some(vec!["tag1".to_string(), "tag2".to_string()]),
            metadata: Some(HashMap::from([
                ("key1".to_string(), "value1".to_string()),
                ("key2".to_string(), "value2".to_string()),
            ])),
            space_id: Some("space_general".to_string()),
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: IndexFileRequestDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.path, "/path/to/file.txt");
        assert!(deserialized.tags.is_some());
        assert!(deserialized.metadata.is_some());
        assert_eq!(deserialized.space_id.as_deref(), Some("space_general"));
    }

    #[test]
    fn test_chunking_strategy_dto_serialization() {
        let strategies = vec![
            ChunkingStrategyDto::FixedSize { size: 512 },
            ChunkingStrategyDto::Semantic { max_tokens: 1024 },
            ChunkingStrategyDto::Paragraph,
            ChunkingStrategyDto::Sentence,
        ];

        for strategy in strategies {
            let json = serde_json::to_string(&strategy).unwrap();
            let _deserialized: ChunkingStrategyDto = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn test_index_file_response_dto_serialization() {
        let response = IndexFileResponseDto {
            document_id: "doc-123".to_string(),
            chunks_created: 5,
            status: "success".to_string(),
            error: None,
            file_path: "/library/doc-123.txt".to_string(),
        };

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: IndexFileResponseDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.document_id, "doc-123");
        assert_eq!(deserialized.chunks_created, 5);
        assert_eq!(deserialized.file_path, "/library/doc-123.txt");
    }

    #[test]
    fn test_index_directory_response_dto_serialization() {
        let response = IndexDirectoryResponseDto {
            files_indexed: 10,
            files_failed: 2,
            total_chunks: 50,
            document_ids: vec!["doc-1".to_string(), "doc-2".to_string()],
            errors: vec!["Failed to read file.txt".to_string()],
        };

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: IndexDirectoryResponseDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.files_indexed, 10);
        assert_eq!(deserialized.files_failed, 2);
    }
}
