//! Mock implementations of UnitOfWork and repositories for testing
//!
//! This module provides mock implementations using the `mockall` crate.
//! These mocks are designed to be used in service layer tests.
//!
//! # Example Usage
//!
//! ```rust
//! use lattice::domain::repositories::mocks::{MockUnitOfWork, MockUnitOfWorkFactory};
//! use mockall::predicate::*;
//!
//! #[tokio::test]
//! async fn test_with_mock_uow() {
//!     let mut mock_uow = MockUnitOfWork::new();
//!
//!     // Set expectations
//!     mock_uow.expect_commit()
//!         .times(1)
//!         .returning(|| Ok(()));
//!
//!     // Use in service
//!     let result = mock_uow.commit().await;
//!     assert!(result.is_ok());
//! }
//! ```

#![cfg(test)]

use mockall::mock;

#[cfg(test)]
use async_trait::async_trait;

#[cfg(test)]
use crate::shared::result::Result;

#[cfg(test)]
use crate::application::ports::{
    unit_of_work::{ModelFileRepositoryPort, ModelRepositoryPort},
    BatchJobRepositoryPort, ChunkRepositoryPort, DocumentRepositoryPort, EmbeddingRepositoryPort,
};

#[cfg(test)]
use crate::domain::repositories::{
    model_repository::ModelRepository, SearchRepository, SystemRepository,
};

#[cfg(test)]
use crate::domain::entities::Embedding;
#[cfg(test)]
use crate::domain::entities::{Chunk, Document};

#[cfg(test)]
use crate::domain::entities::model::Model;

#[cfg(test)]
use crate::domain::value_objects::model_status::{FileStatus, ModelStatus};

#[cfg(test)]
use crate::domain::repositories::search_repository::SearchResult;

#[cfg(test)]
use crate::application::ports::repository_port::Filter;
#[cfg(test)]
use crate::application::ports::repository_port::RepositoryPort;

#[cfg(test)]
pub struct MockChunkRepository;

#[cfg(test)]
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

#[cfg(test)]
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
        use crate::shared::domain_types::DocumentId;
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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
        async fn delete_by_chunk_id(&self, chunk_id: &str) -> Result<()>;
        async fn delete_by_document_id(&self, document_id: &str) -> Result<()>;
        async fn count(&self) -> Result<i64>;
        async fn create_batch(&self, entries: Vec<(Embedding, Vec<f32>)>) -> Result<Vec<String>>;
    }
}

#[cfg(test)]
mock! {
    /// Mock implementation of SearchRepository
    pub SearchRepo {}

    impl std::fmt::Debug for SearchRepo {
        fn fmt<'a>(&self, f: &mut std::fmt::Formatter<'a>) -> std::fmt::Result;
    }

    #[async_trait]
    impl SearchRepository for SearchRepo {
        async fn search_bm25(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>>;
        async fn count_searchable_chunks(&self) -> Result<i64>;
        async fn optimize_index(&self) -> Result<()>;
        async fn rebuild_index(&self) -> Result<()>;
    }
}

#[cfg(test)]
pub struct MockBatchJobRepository;

#[cfg(test)]
impl Default for MockBatchJobRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl MockBatchJobRepository {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(test)]
#[async_trait]
impl BatchJobRepositoryPort for MockBatchJobRepository {
    async fn create_batch_job(
        &self,
        _job_id: &str,
        _job_type: &str,
        _total_items: i64,
        _options: Option<&str>,
    ) -> Result<()> {
        Ok(())
    }

    async fn create_batch_items(&self, _job_id: &str, _urls: Vec<String>) -> Result<()> {
        Ok(())
    }

    async fn update_job_status(
        &self,
        _job_id: &str,
        _status: &str,
        _started_at: Option<String>,
        _completed_at: Option<String>,
    ) -> Result<()> {
        Ok(())
    }

    async fn update_progress(
        &self,
        _job_id: &str,
        _completed: i64,
        _failed: i64,
        _progress: f64,
    ) -> Result<()> {
        Ok(())
    }

    async fn update_item_status(
        &self,
        _item_id: &str,
        _status: crate::application::ports::BatchItemState,
        _document_id: Option<&str>,
        _error_message: Option<&str>,
    ) -> Result<()> {
        Ok(())
    }

    async fn get_batch_job(
        &self,
        _job_id: &str,
    ) -> Result<crate::application::ports::batch_job_repository_port::BatchJobStatus> {
        use crate::shared::error::AppError;
        Err(AppError::NotFound("Job not found".to_string()))
    }

    async fn get_pending_items(
        &self,
        _job_id: &str,
    ) -> Result<Vec<crate::application::ports::batch_job_repository_port::BatchJobItem>> {
        Ok(vec![])
    }

    async fn cancel_pending_items(&self, _job_id: &str) -> Result<usize> {
        Ok(0)
    }

    async fn list_batch_jobs(
        &self,
        _limit: Option<i64>,
        _offset: Option<i64>,
    ) -> Result<Vec<crate::application::ports::batch_job_repository_port::BatchJobSummary>> {
        Ok(vec![])
    }

    async fn delete_batch_job(&self, _job_id: &str) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mock! {
    /// Mock implementation of SystemRepository
    pub SystemRepo {}

    #[async_trait]
    impl SystemRepository for SystemRepo {
        async fn vacuum(&self) -> Result<()>;
        async fn analyze(&self) -> Result<()>;
        async fn get_database_size(&self) -> Result<i64>;
        async fn integrity_check(&self) -> Result<bool>;
    }
}

#[cfg(test)]
mock! {
    /// Mock implementation of ModelRepositoryPort
    pub ModelRepo {}

    #[async_trait]
    impl ModelRepositoryPort for ModelRepo {
        async fn find_by_model_id(&self, model_id: &str) -> Result<Option<crate::domain::entities::model::Model>>;
        async fn create_model_with_files(
            &self,
            model: &crate::domain::entities::model::Model,
            files: Vec<crate::domain::entities::model_file::ModelFile>,
        ) -> Result<()>;
        async fn update_status_completed(&self, model_id: &str) -> Result<()>;
        async fn update_status_failed(&self, model_id: &str) -> Result<()>;
    }
}

#[cfg(test)]
mock! {
    /// Mock implementation of ModelFileRepositoryPort
    pub ModelFileRepo {}

    #[async_trait]
    impl ModelFileRepositoryPort for ModelFileRepo {
        async fn create(&self, model_file: &crate::domain::entities::model_file::ModelFile) -> Result<()>;
        async fn find_by_model_id(&self, model_id: &str) -> Result<Vec<crate::domain::entities::model_file::ModelFile>>;
        async fn count_total_files(&self, model_id: &str) -> Result<usize>;
        async fn count_completed_files(&self, model_id: &str) -> Result<usize>;
        async fn update_file_status_by_model_and_name(
            &self,
            model_id: &str,
            file_name: &str,
            status: crate::domain::value_objects::model_status::FileStatus,
            size_bytes: Option<i64>,
        ) -> Result<()>;
    }
}

#[cfg(test)]
mock! {
    /// Mock implementation of ModelRepository for testing ModelService
    ///
    /// This mock allows you to set expectations for model repository operations
    /// used by ModelService (not to be confused with MockModelRepo which implements
    /// ModelRepositoryPort for UnitOfWork).
    ///
    /// # Example
    ///
    /// ```rust
    /// use lattice::domain::repositories::mocks::MockModelRepository;
    /// use mockall::predicate::*;
    ///
    /// #[tokio::test]
    /// async fn test_model_service_creates_model() {
    ///     let mut mock_repo = MockModelRepository::new();
    ///
    ///     // Expect find_by_model_id to return None (no duplicate)
    ///     mock_repo.expect_find_by_model_id()
    ///         .with(eq("test-model-id"))
    ///         .times(1)
    ///         .returning(|_| Ok(None));
    ///
    ///     // Expect create to succeed
    ///     mock_repo.expect_create()
    ///         .times(1)
    ///         .returning(|_| Ok(()));
    ///
    ///     // Use in ModelService...
    ///     let service = ModelService::new(Arc::new(mock_repo));
    /// }
    /// ```
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

#[cfg(test)]
mock! {
    /// Mock implementation of UnitOfWork trait
    ///
    /// This mock allows you to set expectations for repository access
    /// and transaction operations (commit/rollback).
    ///
    /// # Example
    ///
    /// ```rust
    /// use lattice::domain::repositories::mocks::MockUnitOfWork;
    /// use mockall::predicate::*;
    ///
    /// #[tokio::test]
    /// async fn test_service_commits() {
    ///     let mut mock_uow = MockUnitOfWork::new();
    ///
    ///     // Expect commit to be called once
    ///     mock_uow.expect_commit()
    ///         .times(1)
    ///         .returning(|| Ok(()));
    ///
    ///     // Use mock in service...
    ///     let result = mock_uow.commit().await;
    ///     assert!(result.is_ok());
    /// }
    /// ```
    pub UnitOfWork {}

    #[async_trait]
    impl crate::application::ports::unit_of_work::UnitOfWork for UnitOfWork {
        fn chunk_repository<'a>(&'a self) -> Result<Box<dyn ChunkRepositoryPort + Send + 'a>>;
        fn document_repository<'a>(&'a self) -> Result<Box<dyn DocumentRepositoryPort + Send + 'a>>;
        fn embedding_repository<'a>(&'a self) -> Result<Box<dyn EmbeddingRepositoryPort + Send + 'a>>;
        fn search_repository<'a>(&'a self) -> Result<Box<dyn SearchRepository + Send + 'a>>;
        fn batch_job_repository<'a>(&'a self) -> Result<Box<dyn BatchJobRepositoryPort + Send + 'a>>;
        fn system_repository<'a>(&'a self) -> Result<Box<dyn SystemRepository + Send + 'a>>;
        fn model_repository<'a>(&'a self) -> Result<Box<dyn ModelRepositoryPort + 'a>>;
        fn model_file_repository<'a>(&'a self) -> Result<Box<dyn ModelFileRepositoryPort + 'a>>;
        async fn commit(&mut self) -> Result<()>;
        async fn rollback(&mut self) -> Result<()>;
    }
}

#[cfg(test)]
mock! {
    /// Mock implementation of UnitOfWorkFactory trait
    ///
    /// This mock allows you to control what UnitOfWork instances are created.
    ///
    /// # Example
    ///
    /// ```rust
    /// use lattice::domain::repositories::mocks::{MockUnitOfWorkFactory, MockUnitOfWork};
    /// use mockall::predicate::*;
    ///
    /// #[tokio::test]
    /// async fn test_factory_creates_uow() {
    ///     let mut mock_factory = MockUnitOfWorkFactory::new();
    ///     let mut mock_uow = MockUnitOfWork::new();
    ///
    ///     mock_uow.expect_commit()
    ///         .returning(|| Ok(()));
    ///
    ///     mock_factory.expect_create()
    ///         .times(1)
    ///         .returning(move || Ok(Box::new(mock_uow)));
    ///
    ///     let uow = mock_factory.create().await.unwrap();
    ///     // Use uow...
    /// }
    /// ```
    pub UnitOfWorkFactory {}

    #[async_trait]
    impl crate::application::ports::unit_of_work::UnitOfWorkFactory for UnitOfWorkFactory {
        async fn create(&self) -> Result<Box<dyn crate::application::ports::unit_of_work::UnitOfWork + Send>>;
    }
}

#[cfg(test)]
/// Create a default MockUnitOfWork with basic expectations set
///
/// This helper sets up common default behaviors:
/// - commit() returns Ok(())
/// - rollback() returns Ok(())
///
/// You can override these with your own expectations.
pub fn create_default_mock_uow() -> MockUnitOfWork {
    let mut mock = MockUnitOfWork::new();

    // Default: commit succeeds
    mock.expect_commit().returning(|| Ok(()));

    // Default: rollback succeeds
    mock.expect_rollback().returning(|| Ok(()));

    mock
}

#[cfg(test)]
/// Create a MockUnitOfWork that simulates a commit failure
pub fn create_failing_commit_mock_uow() -> MockUnitOfWork {
    let mut mock = MockUnitOfWork::new();

    mock.expect_commit().returning(|| {
        Err(crate::shared::error::AppError::Database(
            "Commit failed".to_string(),
        ))
    });

    mock
}

#[cfg(test)]
/// Create a MockUnitOfWork that simulates a rollback failure
pub fn create_failing_rollback_mock_uow() -> MockUnitOfWork {
    let mut mock = MockUnitOfWork::new();

    mock.expect_rollback().returning(|| {
        Err(crate::shared::error::AppError::Database(
            "Rollback failed".to_string(),
        ))
    });

    mock
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::unit_of_work::UnitOfWork;

    #[tokio::test]
    async fn test_mock_uow_commit() {
        let mut mock = create_default_mock_uow();
        let result = mock.commit().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mock_uow_rollback() {
        let mut mock = create_default_mock_uow();
        let result = mock.rollback().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_failing_commit() {
        let mut mock = create_failing_commit_mock_uow();
        let result = mock.commit().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_failing_rollback() {
        let mut mock = create_failing_rollback_mock_uow();
        let result = mock.rollback().await;
        assert!(result.is_err());
    }
}
