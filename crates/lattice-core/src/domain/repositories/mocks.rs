//! Mock implementations of repositories for testing
//!
//! This module provides mock implementations using the `mockall` crate.
//! These mocks are designed to be used in service layer tests.

#![cfg(any(test, feature = "test-support"))]

use mockall::mock;

use async_trait::async_trait;

use crate::shared::error::Result;

use crate::application::ports::{
    ChunkRepositoryPort, DocumentRepositoryPort, EmbeddingRepositoryPort,
};

use crate::domain::repositories::model_repository::ModelRepository;

use crate::domain::entities::Embedding;
use crate::domain::entities::{Chunk, Document};

use crate::domain::entities::model::Model;

use crate::domain::value_objects::model_status::{FileStatus, ModelStatus};

use crate::application::ports::repository_port::Filter;
use crate::application::ports::repository_port::RepositoryPort;

pub struct MockChunkRepository;

impl Default for MockChunkRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl MockChunkRepository {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ChunkRepositoryPort for MockChunkRepository {
    async fn create(
        &self,
        _document_id: &str,
        _content: &str,
        _context_prefix: Option<&str>,
        _contextualized_content: Option<&str>,
        _index: usize,
        _start_char: Option<i64>,
        _end_char: Option<i64>,
    ) -> Result<Chunk> {
        use crate::shared::types::DocumentId;
        let doc_id = DocumentId::from("doc-1".to_string());
        Ok(Chunk::new(doc_id, "test content".to_string(), 0))
    }

    async fn find_by_document(&self, _document_id: &str) -> Result<Vec<Chunk>> {
        Ok(vec![])
    }

    async fn find_by_ids(&self, _chunk_ids: &[String]) -> Result<Vec<Chunk>> {
        Ok(vec![])
    }

    async fn delete_by_document(&self, _document_id: &str) -> Result<()> {
        Ok(())
    }

    async fn count_all(&self) -> Result<i64> {
        Ok(0)
    }

    async fn count_indexed_documents(&self) -> Result<i64> {
        Ok(0)
    }

    async fn count_by_document(&self, _document_id: &str) -> Result<i64> {
        Ok(0)
    }

    async fn create_batch(&self, chunks: Vec<Chunk>) -> Result<Vec<Chunk>> {
        Ok(chunks)
    }
}

#[async_trait]
impl RepositoryPort<Chunk> for MockChunkRepository {
    async fn find_by_id(&self, _id: &str) -> Result<Option<Chunk>> {
        Ok(None)
    }

    async fn find_by_filter(&self, _filter: &dyn Filter) -> Result<Vec<Chunk>> {
        Ok(vec![])
    }

    async fn find_all(&self) -> Result<Vec<Chunk>> {
        Ok(vec![])
    }

    async fn save(&self, _entity: &Chunk) -> Result<()> {
        Ok(())
    }

    async fn save_batch(&self, _entities: &[Chunk]) -> Result<()> {
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

    async fn exists(&self, _id: &str) -> Result<bool> {
        Ok(false)
    }
}

pub struct DddMockDocumentRepository;

impl Default for DddMockDocumentRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl DddMockDocumentRepository {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl crate::application::ports::repository_port::RepositoryPort<Document>
    for DddMockDocumentRepository
{
    async fn find_by_id(&self, _id: &str) -> Result<Option<Document>> {
        Ok(None)
    }

    async fn find_by_filter(&self, _filter: &dyn Filter) -> Result<Vec<Document>> {
        Ok(vec![])
    }

    async fn find_all(&self) -> Result<Vec<Document>> {
        Ok(vec![])
    }

    async fn save(&self, _entity: &Document) -> Result<()> {
        Ok(())
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

    async fn exists(&self, _id: &str) -> Result<bool> {
        Ok(false)
    }
}

#[async_trait]
impl DocumentRepositoryPort for DddMockDocumentRepository {
    async fn find_file_path_by_id(&self, _document_id: &str) -> Result<String> {
        Ok("/mock/path".to_string())
    }

    async fn document_exists(&self, _document_id: &str) -> Result<bool> {
        Ok(false)
    }

    async fn find_id_by_path(&self, _file_path: &str) -> Result<Option<String>> {
        Ok(None)
    }

    async fn delete(&self, _document_id: &str) -> Result<()> {
        Ok(())
    }

    async fn delete_and_release_blob(&self, document_id: &str) -> Result<()> {
        <Self as DocumentRepositoryPort>::delete(self, document_id).await
    }

    async fn find_by_checksum(
        &self,
        _checksum: &crate::domain::value_objects::Checksum,
    ) -> Result<Option<crate::domain::entities::Document>> {
        Ok(None)
    }

    async fn count_documents(&self) -> Result<i64> {
        Ok(0)
    }

    async fn count_chunks(&self) -> Result<i64> {
        Ok(0)
    }

    async fn find_all_paginated(&self, _limit: usize) -> Result<Vec<Document>> {
        Ok(vec![])
    }
}

impl crate::application::ports::DocumentRepository for DddMockDocumentRepository {}

mock! {
    /// Mock implementation of EmbeddingRepositoryPort
    pub EmbeddingRepository {}

    #[async_trait]
    impl EmbeddingRepositoryPort for EmbeddingRepository {
        async fn create(&self, chunk_id: &str, vector: &[f32], model: &str) -> Result<String>;
        async fn find_by_chunk(&self, chunk_id: &str) -> Result<Option<Embedding>>;
        async fn save(&self, entity: &Embedding, vector: Vec<f32>) -> Result<()>;
        async fn save_batch(&self, entries: Vec<(Embedding, Vec<f32>)>) -> Result<()>;
        async fn find_by_chunk_id(&self, chunk_id: &str) -> Result<Option<(Embedding, Vec<f32>)>>;
        async fn find_by_document_id(&self, document_id: &str) -> Result<Vec<(Embedding, Vec<f32>)>>;
        async fn vectors_by_document_ids(&self, document_ids: &[String]) -> Result<crate::application::ports::embedding_repository_port::DocumentVectors>;
        async fn delete_by_chunk_id(&self, chunk_id: &str) -> Result<()>;
        async fn delete_by_document_id(&self, document_id: &str) -> Result<()>;
        async fn count(&self) -> Result<i64>;
        async fn create_batch(&self, entries: Vec<(Embedding, Vec<f32>)>) -> Result<Vec<String>>;
    }
}

mock! {
    /// Mock implementation of ModelRepository for testing ModelService
    ///
    /// This mock allows you to set expectations for model repository operations
    /// used by ModelService.
    pub ModelRepository {}

    #[async_trait]
    impl ModelRepository for ModelRepository {
        async fn create(&self, model: &Model) -> Result<()>;
        async fn find_by_model_id(&self, model_id: &str) -> Result<Option<Model>>;
        async fn find_all(&self) -> Result<Vec<Model>>;
        async fn update_status(&self, model_id: &str, status: ModelStatus) -> Result<()>;
        async fn update_file_progress(&self, model_id: &str, file_name: &str, downloaded_bytes: i64, status: FileStatus) -> Result<()>;
        async fn delete(&self, model_id: &str) -> Result<()>;
    }
}
