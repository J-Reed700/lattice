//! Tests for reindexing: vector eviction, empty extractions and strategy changes.

use super::*;
use crate::features::embedding::entity::Embedding;
use async_trait::async_trait;
use lattice_core::application::ports::UnitOfWorkFactory;
use lattice_core::application::ports::{EmbeddingRepositoryPort, FileMetadata, Filter};
use lattice_core::domain::entities::Document;
use lattice_core::shared::types::ValidatedFilePath;
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use tempfile::TempDir;

/// Records what reindex asks the vector index to do, so tests can assert
/// the index is kept in step with the database rather than silently
/// diverging from it.
#[derive(Default)]
struct MockVectorSearch {
    added: Mutex<Vec<String>>,
    removed: Mutex<Vec<String>>,
    batches: Mutex<(usize, usize)>,
    published: Mutex<Vec<(String, String)>>,
}

impl VectorSearchPort for MockVectorSearch {
    fn search(
        &self,
        _query_embedding: &[f32],
        _top_k: usize,
        _threshold: f32,
    ) -> Result<Vec<crate::features::search::dto::SearchResultPortDto>> {
        Ok(Vec::new())
    }

    fn search_scoped(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        threshold: f32,
        allowed_document_ids: Option<&std::collections::HashSet<String>>,
    ) -> Result<Vec<crate::features::search::dto::SearchResultPortDto>> {
        let mut results = self.search(query_embedding, top_k, threshold)?;
        if let Some(scope) = allowed_document_ids {
            results.retain(|result| scope.contains(&result.doc_id));
            results.truncate(top_k);
        }
        Ok(results)
    }

    fn add_embedding(&self, id: String, _embedding: Vec<f32>) -> Result<()> {
        self.added.lock().unwrap().push(id);
        Ok(())
    }

    fn remove_embedding(&self, id: &str) -> Result<()> {
        self.removed.lock().unwrap().push(id.to_string());
        Ok(())
    }

    fn remove_embeddings(&self, ids: &[String]) -> Result<()> {
        self.batches.lock().unwrap().0 += 1;
        self.removed.lock().unwrap().extend_from_slice(ids);
        Ok(())
    }

    fn publish_embeddings(
        &self,
        entries: Vec<lattice_core::application::ports::vector_search_port::VectorIndexEntry>,
    ) -> Result<()> {
        self.batches.lock().unwrap().1 += 1;
        for entry in entries {
            self.added.lock().unwrap().push(entry.id);
            self.published
                .lock()
                .unwrap()
                .push((entry.document_id, entry.content));
        }
        Ok(())
    }

    fn clear(&self) -> Result<()> {
        Ok(())
    }

    fn count(&self) -> usize {
        self.added.lock().unwrap().len()
    }

    fn dimension(&self) -> usize {
        384
    }
}

struct MockFileStorage {
    content: &'static str,
}

impl MockFileStorage {
    fn updated() -> Self {
        Self {
            content: "Updated content for reindexing. This is a longer text that will be chunked properly with the default chunking strategy. \
            We need to ensure this content is long enough to create at least one chunk when using the default FixedSize strategy with size 512. \
            This text is being extended to reach that minimum length requirement so that the reindexing tests can verify that chunks are created successfully. \
            Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. \
            Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.",
        }
    }
}

#[async_trait]
impl FileStoragePort for MockFileStorage {
    async fn read_file(&self, _path: &Path) -> Result<String> {
        Ok(self.content.to_string())
    }

    async fn read_file_bytes(&self, _path: &Path) -> Result<Vec<u8>> {
        Ok(b"Updated content for reindexing. This is a longer text that will be chunked properly with the default chunking strategy.".to_vec())
    }

    async fn write_file(&self, _path: &Path, _content: &str) -> Result<()> {
        Ok(())
    }

    async fn write_file_bytes(&self, _path: &Path, _content: &[u8]) -> Result<()> {
        Ok(())
    }

    async fn delete_file(&self, _path: &Path) -> Result<()> {
        Ok(())
    }

    async fn compute_hash(&self, _path: &Path) -> Result<String> {
        Ok("mock_hash".to_string())
    }

    async fn exists(&self, _path: &Path) -> bool {
        true
    }

    async fn metadata(&self, _path: &Path) -> Result<FileMetadata> {
        Ok(FileMetadata {
            size: 1000,
            modified_at: 1640000000,
            is_file: true,
            is_directory: false,
        })
    }
}

struct MockEmbedder;

#[async_trait]
impl EmbeddingPort for MockEmbedder {
    async fn embed_single(&self, _text: &str) -> Result<Vec<f32>> {
        Ok(vec![0.1, 0.2])
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| vec![0.1, 0.2]).collect())
    }

    fn dimension(&self) -> usize {
        2
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

struct MockUowDocumentRepo;

#[async_trait]
impl RepositoryPort<Document> for MockUowDocumentRepo {
    async fn save(&self, _entity: &Document) -> Result<()> {
        Ok(())
    }

    async fn find_by_id(&self, _id: &str) -> Result<Option<Document>> {
        Ok(None)
    }

    async fn find_all(&self) -> Result<Vec<Document>> {
        Ok(vec![])
    }

    async fn find_by_filter(&self, _filter: &dyn Filter) -> Result<Vec<Document>> {
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

    async fn exists(&self, _id: &str) -> Result<bool> {
        Ok(false)
    }
}

#[async_trait]
impl lattice_core::application::ports::DocumentRepositoryPort for MockUowDocumentRepo {
    async fn find_by_checksum(
        &self,
        _checksum: &lattice_core::domain::value_objects::Checksum,
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

struct MockUowEmbeddingRepo;

#[async_trait]
impl EmbeddingRepositoryPort for MockUowEmbeddingRepo {
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

struct MockUnitOfWork;

#[async_trait]
impl lattice_core::application::ports::UnitOfWork for MockUnitOfWork {
    fn chunk_repository(
        &self,
    ) -> Result<Box<dyn lattice_core::application::ports::ChunkRepositoryPort + Send + '_>> {
        Err(AppError::InvalidState(
            "Chunk repository not used in test".to_string(),
        ))
    }

    fn document_repository(
        &self,
    ) -> Result<Box<dyn lattice_core::application::ports::DocumentRepositoryPort + Send + '_>> {
        Ok(Box::new(MockUowDocumentRepo))
    }

    fn embedding_repository(&self) -> Result<Box<dyn EmbeddingRepositoryPort + Send + '_>> {
        Ok(Box::new(MockUowEmbeddingRepo))
    }

    fn system_repository(
        &self,
    ) -> Result<Box<dyn lattice_core::domain::repositories::SystemRepository + Send + '_>> {
        Err(AppError::InvalidState(
            "System repository not used in test".to_string(),
        ))
    }

    fn model_repository(
        &self,
    ) -> Result<Box<dyn lattice_core::application::ports::unit_of_work::ModelRepositoryPort + '_>>
    {
        Err(AppError::InvalidState(
            "Model repository not used in test".to_string(),
        ))
    }

    fn model_file_repository(
        &self,
    ) -> Result<Box<dyn lattice_core::application::ports::unit_of_work::ModelFileRepositoryPort + '_>>
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
    async fn create(&self) -> Result<Box<dyn lattice_core::application::ports::UnitOfWork + Send>> {
        Ok(Box::new(MockUnitOfWork))
    }
}

struct MockDocumentRepo {
    document: Mutex<Option<Document>>,
}

impl MockDocumentRepo {
    fn new() -> Self {
        Self {
            document: Mutex::new(None),
        }
    }

    fn set_document(&self, doc: Document) {
        *self.document.lock().unwrap() = Some(doc);
    }
}

#[async_trait]
impl RepositoryPort<Document> for MockDocumentRepo {
    async fn save(&self, entity: &Document) -> Result<()> {
        *self.document.lock().unwrap() = Some(entity.clone());
        Ok(())
    }

    async fn find_by_id(&self, _id: &str) -> Result<Option<Document>> {
        Ok(self.document.lock().unwrap().clone())
    }

    async fn find_all(&self) -> Result<Vec<Document>> {
        Ok(self.document.lock().unwrap().iter().cloned().collect())
    }

    async fn find_by_filter(&self, _filter: &dyn Filter) -> Result<Vec<Document>> {
        Ok(vec![])
    }

    async fn save_batch(&self, entities: &[Document]) -> Result<()> {
        if let Some(entity) = entities.first() {
            *self.document.lock().unwrap() = Some(entity.clone());
        }
        Ok(())
    }

    async fn delete(&self, _id: &str) -> Result<()> {
        *self.document.lock().unwrap() = None;
        Ok(())
    }

    async fn delete_batch(&self, _ids: &[&str]) -> Result<()> {
        *self.document.lock().unwrap() = None;
        Ok(())
    }

    async fn count(&self) -> Result<usize> {
        Ok(if self.document.lock().unwrap().is_some() {
            1
        } else {
            0
        })
    }

    async fn exists(&self, _id: &str) -> Result<bool> {
        Ok(self.document.lock().unwrap().is_some())
    }
}

fn create_test_aggregate() -> (Document, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    fs::write(&file_path, "Original content").unwrap();

    let validated_path = ValidatedFilePath::new(file_path).unwrap();
    let content = "Original content".to_string();
    let strategy = ChunkingStrategy::FixedSize { size: 10 };
    let metadata = lattice_core::domain::value_objects::FileMetadata::new(
        "test.txt".to_string(),
        "text/plain".to_string(),
        content.len() as i64,
        chrono::Utc::now(),
    )
    .unwrap();
    let checksum = lattice_core::domain::value_objects::Checksum::new("a".repeat(64)).unwrap();

    let document =
        Document::from_file(validated_path, metadata, checksum, content, strategy).unwrap();
    (document, temp_dir)
}

#[tokio::test]

async fn test_reindex_document() {
    let repo = Arc::new(MockDocumentRepo::new());
    let (aggregate, _temp_dir) = create_test_aggregate();
    let doc_id = aggregate.id().to_string();
    let _original_chunks = aggregate.chunks().len();

    repo.set_document(aggregate);

    let use_case = ReindexDocumentUseCase::new(
        Arc::new(MockFileStorage::updated()),
        Arc::new(MockEmbedder),
        repo.clone(),
        Arc::new(MockUnitOfWorkFactory),
        Arc::new(MockVectorSearch::default()),
    );

    let response = use_case.execute(doc_id.clone()).await.unwrap();

    assert_eq!(response.document_id, doc_id);
    assert_eq!(response.status, "reindexed");
    assert!(response.error.is_none());
    // Chunks may differ due to different content
    assert!(response.chunks_created > 0);
}

/// Reindex must keep the live vector index in step with the database.
/// Updating SQLite alone left the index holding vectors for chunks that
/// no longer existed, and never containing the new ones — so semantic
/// search returned dangling hits and missed the edited content, and a
/// restart did not repair it because the persisted index is only rebuilt
/// when empty.
#[tokio::test]
async fn reindex_evicts_stale_vectors_and_adds_new_ones() {
    let repo = Arc::new(MockDocumentRepo::new());
    let (aggregate, _temp_dir) = create_test_aggregate();
    let doc_id = aggregate.id().to_string();

    let stale_keys: Vec<String> = aggregate
        .chunks()
        .iter()
        .map(|c| {
            lattice_core::shared::persistence::embedding_encoding::vector_key(&c.id().to_string())
        })
        .collect();
    assert!(!stale_keys.is_empty(), "fixture must start with chunks");

    repo.set_document(aggregate);

    let vector_search = Arc::new(MockVectorSearch::default());
    let use_case = ReindexDocumentUseCase::new(
        Arc::new(MockFileStorage::updated()),
        Arc::new(MockEmbedder),
        repo.clone(),
        Arc::new(MockUnitOfWorkFactory),
        vector_search.clone(),
    );

    let response = use_case.execute(doc_id).await.unwrap();

    let removed = vector_search.removed.lock().unwrap().clone();
    for key in &stale_keys {
        assert!(
            removed.contains(key),
            "stale vector {} was not evicted from the index",
            key
        );
    }

    assert_eq!(*vector_search.batches.lock().unwrap(), (1, 1));
    assert!(vector_search
        .published
        .lock()
        .unwrap()
        .iter()
        .all(|(id, text)| !id.is_empty() && !text.is_empty()));
    let added = vector_search.added.lock().unwrap().clone();
    assert_eq!(
        added.len(),
        response.chunks_created,
        "every new chunk must be added to the vector index"
    );
    assert!(
        added.iter().all(|k| k.starts_with("emb_")),
        "added keys must use the canonical scheme: {:?}",
        added
    );
}

#[tokio::test]
async fn a_file_that_now_extracts_to_whitespace_fails_and_keeps_its_vectors() {
    let repo = Arc::new(MockDocumentRepo::new());
    let (aggregate, _temp_dir) = create_test_aggregate();
    let doc_id = aggregate.id().to_string();
    repo.set_document(aggregate);
    let vector_search = Arc::new(MockVectorSearch::default());
    let use_case = ReindexDocumentUseCase::new(
        Arc::new(MockFileStorage {
            content: " \n\t\n ",
        }),
        Arc::new(MockEmbedder),
        repo,
        Arc::new(MockUnitOfWorkFactory),
        vector_search.clone(),
    );

    let error = use_case.execute(doc_id).await.unwrap_err();
    assert!(
        matches!(&error, AppError::InvalidInput(m) if m.contains("no extractable text")),
        "{error:?}"
    );
    assert!(vector_search.removed.lock().unwrap().is_empty());
    assert!(vector_search.added.lock().unwrap().is_empty());
}

#[tokio::test]
async fn test_reindex_document_not_found() {
    let repo = Arc::new(MockDocumentRepo::new());
    // Don't set any document

    let use_case = ReindexDocumentUseCase::new(
        Arc::new(MockFileStorage::updated()),
        Arc::new(MockEmbedder),
        repo,
        Arc::new(MockUnitOfWorkFactory),
        Arc::new(MockVectorSearch::default()),
    );

    let result = use_case.execute("nonexistent-id".to_string()).await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), AppError::NotFound(_)));
}

#[tokio::test]

async fn test_reindex_with_different_strategy() {
    let repo = Arc::new(MockDocumentRepo::new());
    let (aggregate, _temp_dir) = create_test_aggregate();
    let doc_id = aggregate.id().to_string();

    repo.set_document(aggregate);

    let vector_search = Arc::new(MockVectorSearch::default());
    let use_case = ReindexDocumentUseCase::new(
        Arc::new(MockFileStorage::updated()),
        Arc::new(MockEmbedder),
        repo,
        Arc::new(MockUnitOfWorkFactory),
        vector_search.clone(),
    );

    // Reindex with different chunk size
    let new_strategy = ChunkingStrategy::FixedSize { size: 5 };
    let response = use_case
        .execute_with_strategy(doc_id, new_strategy)
        .await
        .unwrap();

    assert!(response.chunks_created > 0);
    assert_eq!(response.status, "reindexed");
    assert_eq!(*vector_search.batches.lock().unwrap(), (1, 1));
    assert_eq!(
        vector_search.published.lock().unwrap().len(),
        response.chunks_created
    );
}
