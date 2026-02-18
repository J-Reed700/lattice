//! # Document Mapper
//!
//! Converts between domain document models and DTOs.
//!
//! This mapper handles conversion between:
//! - `Document` (domain entity) ↔ `DocumentDto` (DTO)

use crate::application::dtos::document_dto::{DocumentDto, DocumentStatsDto};
use crate::domain::entities::Document;

/// Mapper for document-related conversions.
pub struct DocumentMapper;

impl DocumentMapper {
    /// Convert domain Document to DTO.
    ///
    /// # Arguments
    ///
    /// * `document` - Domain document entity
    ///
    /// # Returns
    ///
    /// Flat DTO representation for JSON serialization
    pub fn to_dto(document: &Document) -> DocumentDto {
        DocumentDto {
            id: document.id().to_string(),
            title: document.file_name().to_string(),
            path: document
                .file_path()
                .to_str()
                .unwrap_or_default()
                .to_string(),
            created_at: document.indexed_at().to_rfc3339(),
            modified_at: document.modified_at().to_rfc3339(),
            chunk_count: document.chunks().len(),
            file_size: Some(document.size_bytes()),
            mime_type: Some(document.mime_type().to_string()),
            checksum: Some(document.checksum().to_string()),
            tags: document
                .tags()
                .iter()
                .map(|tag_id| tag_id.to_string())
                .collect(),
        }
    }

    /// Convert multiple documents to DTOs.
    ///
    /// # Arguments
    ///
    /// * `documents` - Vector of document entities
    ///
    /// # Returns
    ///
    /// Vector of DTOs
    pub fn to_dtos(documents: &[Document]) -> Vec<DocumentDto> {
        documents.iter().map(Self::to_dto).collect()
    }

    /// Create document statistics DTO.
    ///
    /// # Arguments
    ///
    /// * `documents` - Vector of all document entities
    ///
    /// # Returns
    ///
    /// Statistics DTO with aggregate metrics
    pub fn to_stats_dto(documents: &[Document]) -> DocumentStatsDto {
        let total_documents = documents.len();
        let total_chunks: usize = documents.iter().map(|d| d.chunks().len()).sum();
        let total_size_bytes: i64 = documents.iter().map(|d| d.size_bytes()).sum();

        let avg_chunks_per_document = if total_documents > 0 {
            total_chunks as f32 / total_documents as f32
        } else {
            0.0
        };

        DocumentStatsDto {
            total_documents,
            total_chunks,
            total_size_bytes,
            avg_chunks_per_document,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::value_objects::chunking_strategy::ChunkingStrategy;
    use crate::shared::domain_types::ValidatedFilePath;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_file(dir: &TempDir, name: &str, content: &str) -> PathBuf {
        let path = dir.path().join(name);
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    #[ignore] // TODO: Modernize for RC2 (Refactor Drift)
    fn test_document_aggregate_to_dto() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = create_test_file(&temp_dir, "test.txt", "Test content for chunking");

        let validated_path = ValidatedFilePath::new(file_path).unwrap();
        let content = "Test content for chunking".to_string();
        let strategy = ChunkingStrategy::FixedSize { size: 10 };
        let metadata = crate::domain::value_objects::FileMetadata::new(
            "test.txt".to_string(),
            "text/plain".to_string(),
            content.len() as i64,
            chrono::Utc::now(),
        )
        .unwrap();
        let checksum = crate::domain::value_objects::Checksum::new("a".repeat(64)).unwrap();

        let aggregate =
            Document::from_file(validated_path, metadata, checksum, content, strategy).unwrap();

        let dto = DocumentMapper::to_dto(&aggregate);

        assert!(!dto.id.is_empty());
        assert_eq!(dto.title, "test.txt");
        assert!(dto.path.ends_with("test.txt"));
        assert_eq!(dto.chunk_count, 0); // Chunks are created separately from document creation
        assert!(dto.file_size.is_some());
        assert!(dto.mime_type.is_some());
    }

    #[test]
    fn test_to_dtos_multiple_documents() {
        let temp_dir = TempDir::new().unwrap();
        let file1 = create_test_file(&temp_dir, "doc1.txt", "Content one");
        let file2 = create_test_file(&temp_dir, "doc2.txt", "Content two");

        let path1 = ValidatedFilePath::new(file1).unwrap();
        let path2 = ValidatedFilePath::new(file2).unwrap();
        let strategy = ChunkingStrategy::FixedSize { size: 10 };

        let content1 = "Content one".to_string();
        let metadata1 = crate::domain::value_objects::FileMetadata::new(
            "doc1.txt".to_string(),
            "text/plain".to_string(),
            content1.len() as i64,
            chrono::Utc::now(),
        )
        .unwrap();
        let checksum1 = crate::domain::value_objects::Checksum::new("a".repeat(64)).unwrap();

        let content2 = "Content two".to_string();
        let metadata2 = crate::domain::value_objects::FileMetadata::new(
            "doc2.txt".to_string(),
            "text/plain".to_string(),
            content2.len() as i64,
            chrono::Utc::now(),
        )
        .unwrap();
        let checksum2 = crate::domain::value_objects::Checksum::new("b".repeat(64)).unwrap();

        let agg1 = Document::from_file(path1, metadata1, checksum1, content1, strategy).unwrap();
        let agg2 = Document::from_file(path2, metadata2, checksum2, content2, strategy).unwrap();

        let aggregates = vec![agg1, agg2];
        let dtos = DocumentMapper::to_dtos(&aggregates);

        assert_eq!(dtos.len(), 2);
        assert_eq!(dtos[0].title, "doc1.txt");
        assert_eq!(dtos[1].title, "doc2.txt");
    }

    #[test]
    #[ignore] // TODO: Modernize for RC2 (Refactor Drift)
    fn test_to_stats_dto() {
        let temp_dir = TempDir::new().unwrap();
        let file1 = create_test_file(&temp_dir, "doc1.txt", "Content one");
        let file2 = create_test_file(&temp_dir, "doc2.txt", "Content two");

        let path1 = ValidatedFilePath::new(file1).unwrap();
        let path2 = ValidatedFilePath::new(file2).unwrap();
        let strategy = ChunkingStrategy::FixedSize { size: 10 };

        let content1 = "Content one".to_string();
        let metadata1 = crate::domain::value_objects::FileMetadata::new(
            "doc1.txt".to_string(),
            "text/plain".to_string(),
            content1.len() as i64,
            chrono::Utc::now(),
        )
        .unwrap();
        let checksum1 = crate::domain::value_objects::Checksum::new("a".repeat(64)).unwrap();

        let content2 = "Content two".to_string();
        let metadata2 = crate::domain::value_objects::FileMetadata::new(
            "doc2.txt".to_string(),
            "text/plain".to_string(),
            content2.len() as i64,
            chrono::Utc::now(),
        )
        .unwrap();
        let checksum2 = crate::domain::value_objects::Checksum::new("b".repeat(64)).unwrap();

        let agg1 = Document::from_file(path1, metadata1, checksum1, content1, strategy).unwrap();
        let agg2 = Document::from_file(path2, metadata2, checksum2, content2, strategy).unwrap();

        let aggregates = vec![agg1, agg2];
        let stats = DocumentMapper::to_stats_dto(&aggregates);

        assert_eq!(stats.total_documents, 2);
        assert_eq!(stats.total_chunks, 0); // Chunks are created separately from document creation
        assert!(stats.total_size_bytes > 0);
        assert_eq!(stats.avg_chunks_per_document, 0.0); // No chunks yet
    }

    #[test]
    fn test_to_stats_dto_empty() {
        let stats = DocumentMapper::to_stats_dto(&[]);

        assert_eq!(stats.total_documents, 0);
        assert_eq!(stats.total_chunks, 0);
        assert_eq!(stats.total_size_bytes, 0);
        assert_eq!(stats.avg_chunks_per_document, 0.0);
    }
}
