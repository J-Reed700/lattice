//! Tests for preparing, committing and publishing a single imported file.

use super::*;
use crate::application::ports::ImportedBlob;
use crate::application::ports::UnitOfWorkFactory;
use crate::application::ports::{ContentAddressedStoragePort, EmbeddingRepositoryPort};
use crate::domain::entities::Document;
use crate::features::embedding::entity::Embedding;
use async_trait::async_trait;
use std::path::Path;

/// Library that indexes files where they already are: every source path
/// is its own "blob", so tests exercise the pipeline without copying.
struct MockContentStorage;

#[async_trait]
impl ContentAddressedStoragePort for MockContentStorage {
    async fn import_file(&self, source_path: &Path) -> Result<ImportedBlob> {
        let hash = "a".repeat(64);
        Ok(ImportedBlob {
            path: source_path.to_path_buf(),
            lease: crate::application::ports::BlobLease::detached(&hash),
            hash,
        })
    }

    async fn exists_by_hash(&self, _hash: &str) -> Result<bool> {
        Ok(false)
    }

    async fn get_path_by_hash(&self, _hash: &str) -> Result<Option<std::path::PathBuf>> {
        Ok(None)
    }

    fn owns(&self, _path: &Path) -> bool {
        false
    }

    async fn list_hashes(&self) -> Result<Vec<String>> {
        Ok(Vec::new())
    }

    async fn remove_if_unreferenced(
        &self,
        _hash: &str,
        _refs: &dyn crate::application::ports::BlobReferenceCheck,
    ) -> Result<crate::application::ports::BlobRemoval> {
        Ok(crate::application::ports::BlobRemoval::Retained(
            crate::application::ports::RetainReason::Missing,
        ))
    }
}

struct MockFileStorage;

#[async_trait]
impl FileStoragePort for MockFileStorage {
    async fn read_file(&self, _path: &Path) -> Result<String> {
        Ok("This is test file content for indexing and chunking.".to_string())
    }

    async fn write_file(&self, _path: &Path, _content: &str) -> Result<()> {
        Ok(())
    }

    async fn delete_file(&self, _path: &Path) -> Result<()> {
        Ok(())
    }

    async fn read_file_bytes(&self, _path: &Path) -> Result<Vec<u8>> {
        Ok(b"Test content".to_vec())
    }

    async fn write_file_bytes(&self, _path: &Path, _content: &[u8]) -> Result<()> {
        Ok(())
    }

    async fn compute_hash(&self, _path: &Path) -> Result<String> {
        Ok("a".repeat(64))
    }

    async fn exists(&self, _path: &Path) -> bool {
        true
    }

    async fn metadata(
        &self,
        _path: &Path,
    ) -> Result<crate::application::ports::file_storage_port::FileMetadata> {
        use crate::application::ports::file_storage_port::FileMetadata;
        use chrono::Utc;
        Ok(FileMetadata {
            size: 100,
            modified_at: Utc::now().timestamp(),
            is_file: true,
            is_directory: false,
        })
    }
}

struct MockContentExtractor;

#[async_trait]
impl ContentExtractionPort for MockContentExtractor {
    async fn extract_content(
        &self,
        path: &Path,
    ) -> Result<crate::application::ports::ExtractedContentData> {
        use crate::application::ports::ExtractedContentData;
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|_| "This is test file content for indexing and chunking.".to_string());
        let char_count = text.chars().count();
        let word_count = text.split_whitespace().count();

        Ok(ExtractedContentData {
            text,
            mime_type: "text/plain".to_string(),
            page_count: None,
            page_ranges: Vec::new(),
            ocr_pages: Vec::new(),
            word_count,
            char_count,
        })
    }

    fn is_supported(&self, _path: &Path) -> bool {
        true
    }
}

struct MockEmbedder;

#[async_trait]
impl EmbeddingPort for MockEmbedder {
    async fn embed_single(&self, _text: &str) -> Result<Vec<f32>> {
        Ok(vec![0.1, 0.2, 0.3])
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| vec![0.1, 0.2, 0.3]).collect())
    }

    fn dimension(&self) -> usize {
        3
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// Embedder with a real input budget: it splits a passage into fixed-width
/// windows. `embedding_input::prepare_structured_with_spans` rebuilds the chunk list
/// from `EmbeddingPort::split_text`, so this is what actually decides how
/// many chunks an indexed document ends up with.
struct MockSplittingEmbedder {
    window: usize,
}

#[async_trait]
impl EmbeddingPort for MockSplittingEmbedder {
    async fn embed_single(&self, _text: &str) -> Result<Vec<f32>> {
        Ok(vec![0.1, 0.2, 0.3])
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| vec![0.1, 0.2, 0.3]).collect())
    }

    fn split_text(
        &self,
        text: &str,
        _prefix: &str,
    ) -> Result<Vec<crate::application::ports::embedding_port::EmbeddingTextChunk>> {
        use crate::application::ports::embedding_port::EmbeddingTextChunk;

        let mut parts = Vec::new();
        let mut start = 0usize;
        while start < text.len() {
            let mut end = (start + self.window).min(text.len());
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            parts.push(EmbeddingTextChunk {
                text: text[start..end].to_string(),
                start,
                end,
                // Keeps the context-prefix budget loop in `prepare_structured_with_spans`
                // from shrinking the prefix.
                token_count: 0,
            });
            start = end;
        }
        Ok(parts)
    }

    fn dimension(&self) -> usize {
        3
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

struct MockEmbeddingRepo;

#[async_trait]
impl EmbeddingRepositoryPort for MockEmbeddingRepo {
    async fn create(&self, _chunk_id: &str, _vector: &[f32], _model: &str) -> Result<String> {
        unimplemented!()
    }

    async fn find_by_chunk(&self, _chunk_id: &str) -> Result<Option<Embedding>> {
        unimplemented!()
    }

    async fn save(&self, _entity: &Embedding, _vector: Vec<f32>) -> Result<()> {
        Ok(())
    }

    async fn save_batch(&self, _entries: Vec<(Embedding, Vec<f32>)>) -> Result<()> {
        Ok(())
    }

    async fn find_by_chunk_id(&self, _chunk_id: &str) -> Result<Option<(Embedding, Vec<f32>)>> {
        Ok(None)
    }

    async fn find_by_document_id(&self, _document_id: &str) -> Result<Vec<(Embedding, Vec<f32>)>> {
        Ok(vec![])
    }

    async fn delete_by_chunk_id(&self, _chunk_id: &str) -> Result<()> {
        Ok(())
    }

    async fn delete_by_document_id(&self, _document_id: &str) -> Result<()> {
        Ok(())
    }

    async fn count(&self) -> Result<i64> {
        Ok(0)
    }

    async fn create_batch(&self, _entries: Vec<(Embedding, Vec<f32>)>) -> Result<Vec<String>> {
        Ok(vec![])
    }
}

struct MockDocumentRepo;

#[async_trait]
impl DocumentRepositoryPort for MockDocumentRepo {
    async fn find_by_checksum(
        &self,
        _checksum: &crate::domain::value_objects::Checksum,
    ) -> Result<Option<Document>> {
        Ok(None)
    }

    async fn count_documents(&self) -> Result<i64> {
        Ok(0)
    }

    async fn count_chunks(&self) -> Result<i64> {
        Ok(0)
    }

    async fn find_file_path_by_id(&self, _document_id: &str) -> Result<String> {
        Ok(String::new())
    }

    async fn find_id_by_path(&self, _file_path: &str) -> Result<Option<String>> {
        Ok(None)
    }

    async fn document_exists(&self, _document_id: &str) -> Result<bool> {
        Ok(false)
    }

    async fn delete(&self, _document_id: &str) -> Result<()> {
        Ok(())
    }

    async fn find_all_paginated(&self, _limit: usize) -> Result<Vec<Document>> {
        Ok(vec![])
    }
}

#[async_trait]
impl crate::application::ports::RepositoryPort<Document> for MockDocumentRepo {
    async fn save(&self, _entity: &Document) -> Result<()> {
        Ok(())
    }

    async fn find_by_id(&self, _id: &str) -> Result<Option<Document>> {
        Ok(None)
    }

    async fn find_all(&self) -> Result<Vec<Document>> {
        Ok(vec![])
    }

    async fn exists(&self, _id: &str) -> Result<bool> {
        Ok(false)
    }

    async fn find_by_filter(
        &self,
        _filter: &dyn crate::application::ports::repository_port::Filter,
    ) -> Result<Vec<Document>> {
        Ok(vec![])
    }

    async fn save_batch(&self, _entities: &[Document]) -> Result<()> {
        Ok(())
    }

    async fn delete(&self, _id: &str) -> Result<()> {
        Ok(())
    }

    async fn delete_batch(&self, _ids: &[&str]) -> Result<()> {
        Ok(())
    }

    async fn count(&self) -> Result<usize> {
        Ok(0)
    }
}

struct MockUnitOfWork;

#[async_trait]
impl crate::application::ports::UnitOfWork for MockUnitOfWork {
    fn chunk_repository(
        &self,
    ) -> Result<Box<dyn crate::application::ports::ChunkRepositoryPort + Send + '_>> {
        Err(AppError::InvalidState(
            "Chunk repository not used in test".to_string(),
        ))
    }

    fn document_repository(&self) -> Result<Box<dyn DocumentRepositoryPort + Send + '_>> {
        Ok(Box::new(MockDocumentRepo))
    }

    fn embedding_repository(&self) -> Result<Box<dyn EmbeddingRepositoryPort + Send + '_>> {
        Ok(Box::new(MockEmbeddingRepo))
    }

    fn search_repository(
        &self,
    ) -> Result<Box<dyn crate::domain::repositories::SearchRepository + Send + '_>> {
        Err(AppError::InvalidState(
            "Search repository not used in test".to_string(),
        ))
    }

    fn system_repository(
        &self,
    ) -> Result<Box<dyn crate::domain::repositories::SystemRepository + Send + '_>> {
        Err(AppError::InvalidState(
            "System repository not used in test".to_string(),
        ))
    }

    fn model_repository(
        &self,
    ) -> Result<Box<dyn crate::application::ports::unit_of_work::ModelRepositoryPort + '_>> {
        Err(AppError::InvalidState(
            "Model repository not used in test".to_string(),
        ))
    }

    fn model_file_repository(
        &self,
    ) -> Result<Box<dyn crate::application::ports::unit_of_work::ModelFileRepositoryPort + '_>>
    {
        Err(AppError::InvalidState(
            "Model file repository not used in test".to_string(),
        ))
    }

    async fn commit(&mut self) -> Result<()> {
        Ok(())
    }

    async fn rollback(&mut self) -> Result<()> {
        Ok(())
    }
}

struct MockUnitOfWorkFactory;

#[async_trait]
impl UnitOfWorkFactory for MockUnitOfWorkFactory {
    async fn create(&self) -> Result<Box<dyn crate::application::ports::UnitOfWork + Send>> {
        Ok(Box::new(MockUnitOfWork))
    }
}

#[tokio::test]
async fn test_index_file_execution() {
    use std::fs;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    fs::write(&file_path, "Test content").unwrap();

    let use_case = IndexFileUseCase::new(
        Arc::new(MockContentStorage),
        Arc::new(MockFileStorage),
        Arc::new(MockContentExtractor),
        Arc::new(MockEmbedder),
        Arc::new(MockDocumentRepo),
        Arc::new(MockEmbeddingRepo),
        Arc::new(MockUnitOfWorkFactory),
    );

    let request = IndexFileRequestDto {
        path: file_path.to_str().unwrap().to_string(),
        chunking_strategy: crate::features::indexing::dto::ChunkingStrategyDto::FixedSize {
            size: 512,
        },
        tags: None,
        metadata: None,
        space_id: None,
    };

    let response = use_case.execute(request).await.unwrap();

    assert!(!response.document_id.is_empty());
    // `prepare_structured_with_spans` discards the strategy-produced chunks and rebuilds
    // them from `EmbeddingPort::split_text`. `MockEmbedder` keeps the port's
    // default, which returns the whole passage as a single part, and this
    // short unstructured file is one structure span, so exactly one chunk.
    assert_eq!(response.chunks_created, 1);
    assert_eq!(response.status, "imported");
    assert!(response.error.is_none());
    assert!(!response.file_path.is_empty());
}

#[tokio::test]
async fn test_index_file_with_chunking() {
    use std::fs;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("long.txt");
    fs::write(&file_path, "A".repeat(1000)).unwrap();

    let use_case = IndexFileUseCase::new(
        Arc::new(MockContentStorage),
        Arc::new(MockFileStorage),
        Arc::new(MockContentExtractor),
        Arc::new(MockSplittingEmbedder { window: 100 }),
        Arc::new(MockDocumentRepo),
        Arc::new(MockEmbeddingRepo),
        Arc::new(MockUnitOfWorkFactory),
    );

    let request = IndexFileRequestDto {
        path: file_path.to_str().unwrap().to_string(),
        chunking_strategy: crate::features::indexing::dto::ChunkingStrategyDto::FixedSize {
            size: 100,
        },
        tags: None,
        metadata: None,
        space_id: None,
    };

    let response = use_case.execute(request).await.unwrap();

    // The 1000-byte file has no headings, so it is one structure span that the
    // embedder splits into 1000 / 100 windows.
    assert_eq!(response.chunks_created, 10);
}
