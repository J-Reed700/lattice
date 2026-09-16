//! Mock repository implementations for testing
//!
//! This module provides in-memory mock implementations of all repository traits.
//! These mocks enable fast, isolated unit testing without requiring a database.
//!
//! # Architecture
//!
//! Reusable repository test doubles:
//! - **Studs (Public Interface)**: Implement the same traits as production repositories
//! - **Bricks (Implementation)**: In-memory HashMaps for storage
//! - **Regeneratable**: Can be swapped with real repositories without code changes
//!
//! # Usage
//!
//! ```rust
//! use crate::infrastructure::persistence::repositories::mocks::MockDocumentRepository;
//! use crate::infrastructure::persistence::repositories::traits::DocumentRepositoryTrait;
//!
//! #[tokio::test]
//! async fn test_with_mock() {
//!     let repo = MockDocumentRepository::new();
//!     let doc = repo.create("/test.txt", "test.txt", "text/plain", 100, "2024-01-01", "abc123").await.unwrap();
//!     assert_eq!(doc.file_name, "test.txt");
//! }
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use parking_lot::RwLock;

use crate::application::ports::{
    ChunkRepositoryPort, DocumentRepositoryPort, EmbeddingRepositoryPort, Filter, RepositoryPort,
};
use crate::domain::entities::chunk::Chunk as ChunkEntity;
use crate::domain::entities::{Chunk, Document};
use crate::features::embedding::entity::Embedding as DomainEmbedding;
use crate::features::embedding::persistence_mapper::{EmbeddingDTO, EmbeddingMapper};
use crate::features::embedding::repository::Embedding;
use crate::features::mentions::entity::Mention;
use crate::infrastructure::persistence::repositories::traits::DocumentRepositoryTrait;
use crate::shared::error::{AppError, Result};
// Tag removed - migrated to DDD (MockTagRepository now in shared/traits.rs)
// Chunk removed - migrated to DDD (use crate::domain::entities::chunk::Chunk)
// Document removed - migrated to DDD (use crate::domain::entities::Document at line ~380)
// Mention types removed - migrated to DDD (use crate::application::ports::mention_repository_port)
// use crate::infrastructure::persistence::repositories::mention_repository::{DocumentMention, Mention, MentionWithContext};

/*
/// In-memory mock implementation of DocumentRepositoryTrait (LEGACY)
///
/// Stores documents in a HashMap. Thread-safe using Arc<RwLock>.
///
/// # Example
/// ```rust
/// let repo = MockDocumentRepository::new();
/// repo.create("/path/file.txt", "file.txt", "text/plain", 1024, "2024-01-01", "hash").await?;
/// ```
#[derive(Clone)]
pub struct MockDocumentRepository {
    documents: Arc<RwLock<HashMap<String, Document>>>,
    path_index: Arc<RwLock<HashMap<String, String>>>, // path -> id mapping
}

impl MockDocumentRepository {
    pub fn new() -> Self {
        Self {
            documents: Arc::new(RwLock::new(HashMap::new())),
            path_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get all documents (for testing/inspection)
    pub fn get_all_internal(&self) -> Vec<Document> {
        self.documents.read().values().cloned().collect()
    }

    /// Clear all data (for test cleanup)
    pub fn clear(&self) {
        self.documents.write().clear();
        self.path_index.write().clear();
    }
}

impl Default for MockDocumentRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DocumentRepositoryTrait for MockDocumentRepository {
    async fn create(
        &self,
        file_path: &str,
        file_name: &str,
        mime_type: &str,
        size_bytes: i64,
        modified_at: &str,
        checksum: &str,
    ) -> Result<Document> {
        let id = Uuid::new_v4().to_string();
        let indexed_at = Utc::now().to_rfc3339();

        let doc = Document {
            id: id.clone(),
            file_path: file_path.to_string(),
            file_name: file_name.to_string(),
            file_type: None,
            mime_type: mime_type.to_string(),
            size_bytes,
            modified_at: modified_at.to_string(),
            indexed_at,
            checksum: checksum.to_string(),
            status: "indexed".to_string(),
        };

        self.documents.write().insert(id.clone(), doc.clone());
        self.path_index.write().insert(file_path.to_string(), id);

        Ok(doc)
    }

    async fn upsert(
        &self,
        file_path: &str,
        file_name: &str,
        mime_type: &str,
        size_bytes: i64,
        modified_at: &str,
        checksum: &str,
    ) -> Result<String> {
        let path_index = self.path_index.read();

        if let Some(existing_id) = path_index.get(file_path) {
            let mut documents = self.documents.write();
            if let Some(doc) = documents.get_mut(existing_id) {
                doc.checksum = checksum.to_string();
                doc.indexed_at = Utc::now().to_rfc3339();
                doc.status = "indexed".to_string();
                return Ok(existing_id.clone());
            }
        }
        drop(path_index);

        let doc = self
            .create(
                file_path,
                file_name,
                mime_type,
                size_bytes,
                modified_at,
                checksum,
            )
            .await?;
        Ok(doc.id)
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<Document>> {
        Ok(self.documents.read().get(id).cloned())
    }

    async fn find_by_path(&self, file_path: &str) -> Result<Option<Document>> {
        let path_index = self.path_index.read();
        if let Some(id) = path_index.get(file_path) {
            Ok(self.documents.read().get(id).cloned())
        } else {
            Ok(None)
        }
    }

    async fn list_all(&self) -> Result<Vec<Document>> {
        let mut docs: Vec<Document> = self.documents.read().values().cloned().collect();
        docs.sort_by(|a, b| b.indexed_at.cmp(&a.indexed_at));
        Ok(docs)
    }

    async fn update_status(&self, id: &str, status: &str) -> Result<()> {
        let mut documents = self.documents.write();
        if let Some(doc) = documents.get_mut(id) {
            doc.status = status.to_string();
            Ok(())
        } else {
            Err(AppError::Database(format!("Document not found: {}", id)))
        }
    }

    async fn delete(&self, id: &str) -> Result<()> {
        let mut documents = self.documents.write();
        if let Some(doc) = documents.remove(id) {
            self.path_index.write().remove(&doc.file_path);
            Ok(())
        } else {
            Err(AppError::Database(format!("Document not found: {}", id)))
        }
    }

    async fn count(&self) -> Result<i64> {
        Ok(self.documents.read().len() as i64)
    }

    async fn exists(&self, file_path: &str) -> Result<bool> {
        Ok(self.path_index.read().contains_key(file_path))
    }

    async fn find_by_path_pattern(&self, pattern: &str) -> Result<Vec<Document>> {
        let pattern_str = pattern.replace('%', "");
        let docs: Vec<Document> = self
            .documents
            .read()
            .values()
            .filter(|doc| doc.file_path.contains(&pattern_str))
            .cloned()
            .collect();
        Ok(docs)
    }
}
*/

#[derive(Clone)]
pub struct MockChunkRepository {
    chunks: Arc<RwLock<HashMap<String, Chunk>>>,
    document_index: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl MockChunkRepository {
    pub fn new() -> Self {
        Self {
            chunks: Arc::new(RwLock::new(HashMap::new())),
            document_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn clear(&self) {
        self.chunks.write().clear();
        self.document_index.write().clear();
    }
}

impl Default for MockChunkRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ChunkRepositoryPort for MockChunkRepository {
    async fn create(
        &self,
        document_id: &str,
        content: &str,
        _context_prefix: Option<&str>,
        _contextualized_content: Option<&str>,
        index: usize,
        _start_char: Option<i64>,
        _end_char: Option<i64>,
    ) -> Result<Chunk> {
        use crate::shared::domain_types::DocumentId;
        let doc_id = DocumentId::from(document_id.to_string());
        let chunk = Chunk::new(doc_id, content.to_string(), index);
        self.save(&chunk).await?;
        Ok(chunk)
    }

    async fn find_by_document(&self, document_id: &str) -> Result<Vec<Chunk>> {
        let document_index = self.document_index.read();
        let chunk_ids = document_index.get(document_id);

        if let Some(ids) = chunk_ids {
            let chunks_map = self.chunks.read();
            let mut chunks: Vec<Chunk> = ids
                .iter()
                .filter_map(|id| chunks_map.get(id).cloned())
                .collect();
            chunks.sort_by_key(|c| c.chunk_index());
            Ok(chunks)
        } else {
            Ok(Vec::new())
        }
    }

    async fn find_by_ids(&self, chunk_ids: &[String]) -> Result<Vec<Chunk>> {
        if chunk_ids.is_empty() {
            return Ok(Vec::new());
        }

        let chunks_map = self.chunks.read();
        let chunks = chunk_ids
            .iter()
            .filter_map(|id| chunks_map.get(id).cloned())
            .collect();

        Ok(chunks)
    }

    async fn delete_by_document(&self, document_id: &str) -> Result<()> {
        let mut document_index = self.document_index.write();
        if let Some(chunk_ids) = document_index.remove(document_id) {
            let mut chunks = self.chunks.write();
            for id in chunk_ids {
                chunks.remove(&id);
            }
        }
        Ok(())
    }

    async fn count_all(&self) -> Result<i64> {
        Ok(self.chunks.read().len() as i64)
    }

    async fn count_indexed_documents(&self) -> Result<i64> {
        Ok(self.document_index.read().len() as i64)
    }

    async fn count_by_document(&self, document_id: &str) -> Result<i64> {
        let document_index = self.document_index.read();
        Ok(document_index
            .get(document_id)
            .map(|ids| ids.len() as i64)
            .unwrap_or(0))
    }

    async fn create_batch(&self, chunks: Vec<Chunk>) -> Result<Vec<Chunk>> {
        for chunk in &chunks {
            self.save(chunk).await?;
        }
        Ok(chunks)
    }
}

#[async_trait]
impl RepositoryPort<ChunkEntity> for MockChunkRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<ChunkEntity>> {
        Ok(self.chunks.read().get(id).cloned())
    }

    async fn find_all(&self) -> Result<Vec<ChunkEntity>> {
        Ok(self.chunks.read().values().cloned().collect())
    }

    async fn save(&self, entity: &ChunkEntity) -> Result<()> {
        let id = entity.id().as_str().to_string();
        let doc_id = entity.document_id().as_str().to_string();

        self.chunks.write().insert(id.clone(), entity.clone());

        let mut doc_index = self.document_index.write();
        doc_index.entry(doc_id).or_default().push(id);

        Ok(())
    }

    async fn save_batch(&self, entities: &[ChunkEntity]) -> Result<()> {
        for entity in entities {
            self.save(entity).await?;
        }
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<()> {
        if let Some(chunk) = self.chunks.write().remove(id) {
            let doc_id = chunk.document_id().as_str();
            let mut doc_index = self.document_index.write();
            if let Some(chunks) = doc_index.get_mut(doc_id) {
                chunks.retain(|chunk_id| chunk_id != id);
            }
        }
        Ok(())
    }

    async fn delete_batch(&self, ids: &[&str]) -> Result<()> {
        for id in ids {
            <Self as RepositoryPort<ChunkEntity>>::delete(self, id).await?;
        }
        Ok(())
    }

    async fn count(&self) -> Result<usize> {
        Ok(self.chunks.read().len())
    }

    async fn exists(&self, id: &str) -> Result<bool> {
        Ok(self.chunks.read().contains_key(id))
    }

    async fn find_by_filter(
        &self,
        _filter: &dyn crate::application::ports::Filter,
    ) -> Result<Vec<ChunkEntity>> {
        // For mock, just return all chunks
        self.find_all().await
    }
}
// END REMOVED MockChunkRepository

// DocumentRepositoryPort, Filter, RepositoryPort already imported at top
// Document already imported at line 34 with Chunk and Mention

/// Mock document repository for testing with DDD ports.
///
/// Implements both `RepositoryPort<Document>` and `DocumentRepositoryPort`
/// for comprehensive testing without database dependencies.
///
/// # Example
///
/// ```rust
/// use crate::infrastructure::persistence::repositories::mocks::MockDocumentRepository;
/// use crate::application::ports::{RepositoryPort, DocumentRepositoryPort};
///
/// let mock = Arc::new(MockDocumentRepository::new());
/// let repo: Arc<dyn RepositoryPort<Document> + DocumentRepositoryPort> = mock;
///
/// // Use in tests
/// repo.save(&document).await?;
/// let found = repo.find_by_id(doc.id()).await?;
/// ```
pub struct MockDocumentRepository {
    documents: Arc<RwLock<HashMap<String, Document>>>,
    paths: Arc<RwLock<HashMap<String, String>>>, // file_path -> id
}

impl MockDocumentRepository {
    /// Create a new mock document repository
    pub fn new() -> Self {
        Self {
            documents: Arc::new(RwLock::new(HashMap::new())),
            paths: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a document to the mock repository for testing
    pub fn add_document(&self, doc: Document) {
        let id = doc.id().as_str().to_string();
        let path = doc.file_path().to_string_lossy().to_string();
        self.documents.write().insert(id.clone(), doc);
        self.paths.write().insert(path, id);
    }

    /// Clear all documents
    pub fn clear(&self) {
        self.documents.write().clear();
        self.paths.write().clear();
    }

    /// Get document count
    pub fn len(&self) -> usize {
        self.documents.read().len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.documents.read().is_empty()
    }
}

impl Default for MockDocumentRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RepositoryPort<Document> for MockDocumentRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Document>> {
        Ok(self.documents.read().get(id).cloned())
    }

    async fn find_by_filter(&self, _filter: &dyn Filter) -> Result<Vec<Document>> {
        Ok(self.documents.read().values().cloned().collect())
    }

    async fn find_all(&self) -> Result<Vec<Document>> {
        let mut docs: Vec<_> = self.documents.read().values().cloned().collect();
        docs.sort_by(|a, b| a.file_path().cmp(b.file_path()));
        Ok(docs)
    }

    async fn save(&self, entity: &Document) -> Result<()> {
        let id = entity.id().as_str().to_string();
        let path = entity.file_path().to_string_lossy().to_string();
        self.documents.write().insert(id.clone(), entity.clone());
        self.paths.write().insert(path, id);
        Ok(())
    }

    async fn save_batch(&self, entities: &[Document]) -> Result<()> {
        for entity in entities {
            self.save(entity).await?;
        }
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<()> {
        if let Some(doc) = self.documents.write().remove(id) {
            let path = doc.file_path().to_string_lossy().to_string();
            self.paths.write().remove(&path);
        }
        Ok(())
    }

    async fn delete_batch(&self, ids: &[&str]) -> Result<()> {
        for id in ids {
            <Self as RepositoryPort<Document>>::delete(self, id).await?;
        }
        Ok(())
    }

    async fn count(&self) -> Result<usize> {
        Ok(self.documents.read().len())
    }

    async fn exists(&self, id: &str) -> Result<bool> {
        Ok(self.documents.read().contains_key(id))
    }
}

#[async_trait]
impl DocumentRepositoryPort for MockDocumentRepository {
    async fn find_file_path_by_id(&self, document_id: &str) -> Result<String> {
        self.documents
            .read()
            .get(document_id)
            .map(|doc| doc.file_path().to_string_lossy().to_string())
            .ok_or_else(|| AppError::NotFound(format!("Document not found: {}", document_id)))
    }

    async fn document_exists(&self, document_id: &str) -> Result<bool> {
        Ok(self.documents.read().contains_key(document_id))
    }

    async fn find_id_by_path(&self, file_path: &str) -> Result<Option<String>> {
        Ok(self.paths.read().get(file_path).cloned())
    }

    async fn delete(&self, document_id: &str) -> Result<()> {
        RepositoryPort::delete(self, document_id).await
    }

    async fn find_by_checksum(
        &self,
        checksum: &crate::domain::value_objects::Checksum,
    ) -> Result<Option<crate::domain::entities::Document>> {
        let docs = self.documents.read();
        for doc in docs.values() {
            if doc.checksum().as_str() == checksum.as_str() {
                return Ok(Some(doc.clone()));
            }
        }
        Ok(None)
    }

    async fn count_documents(&self) -> Result<i64> {
        Ok(self.documents.read().len() as i64)
    }

    async fn count_chunks(&self) -> Result<i64> {
        let docs = self.documents.read();
        let total: usize = docs.values().map(|doc| doc.chunks().len()).sum();
        Ok(total as i64)
    }

    async fn find_all_paginated(&self, limit: usize) -> Result<Vec<Document>> {
        let mut docs: Vec<_> = self.documents.read().values().cloned().collect();
        docs.sort_by(|a, b| a.file_path().cmp(b.file_path()));
        docs.truncate(limit);
        Ok(docs)
    }
}

#[async_trait]
impl DocumentRepositoryTrait for MockDocumentRepository {
    async fn create(
        &self,
        file_path: &str,
        file_name: &str,
        mime_type: &str,
        size_bytes: i64,
        _modified_at: &str,
        checksum: &str,
    ) -> Result<Document> {
        use crate::shared::domain_types::ValidatedFilePath;
        use std::path::PathBuf;

        let validated_path = ValidatedFilePath::new(PathBuf::from(file_path))?;
        let checksum_vo =
            crate::domain::value_objects::checksum::Checksum::new(checksum.to_string())?;
        let mut doc = Document::new(
            validated_path,
            file_name.to_string(),
            mime_type.to_string(),
            size_bytes,
            checksum_vo,
        );

        // Mark as indexed for testing purposes (mock assumes successful indexing)
        doc.mark_indexed();

        self.add_document(doc.clone());
        Ok(doc)
    }

    async fn upsert(
        &self,
        file_path: &str,
        file_name: &str,
        mime_type: &str,
        size_bytes: i64,
        modified_at: &str,
        checksum: &str,
    ) -> Result<String> {
        if let Some(id) = self.paths.read().get(file_path).cloned() {
            if let Some(_doc) = self.documents.read().get(&id).cloned() {
                // For mock, just return existing ID
                return Ok(id);
            }
        }

        let doc = self
            .create(
                file_path,
                file_name,
                mime_type,
                size_bytes,
                modified_at,
                checksum,
            )
            .await?;
        Ok(doc.id().as_str().to_string())
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<Document>> {
        RepositoryPort::find_by_id(self, id).await
    }

    async fn find_by_path(&self, file_path: &str) -> Result<Option<Document>> {
        let id = self.paths.read().get(file_path).cloned();
        if let Some(id) = id {
            RepositoryPort::find_by_id(self, &id).await
        } else {
            Ok(None)
        }
    }

    async fn list_all(&self) -> Result<Vec<Document>> {
        RepositoryPort::find_all(self).await
    }

    async fn update_status(&self, id: &str, _status: &str) -> Result<()> {
        // For mock, just check if exists
        if !self.documents.read().contains_key(id) {
            return Err(AppError::NotFound(format!("Document not found: {}", id)));
        }
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<()> {
        RepositoryPort::delete(self, id).await
    }

    async fn count(&self) -> Result<i64> {
        let count = RepositoryPort::count(self).await?;
        Ok(count as i64)
    }

    async fn exists(&self, file_path: &str) -> Result<bool> {
        Ok(self.paths.read().contains_key(file_path))
    }

    async fn find_by_path_pattern(&self, pattern: &str) -> Result<Vec<Document>> {
        // Simple pattern matching for mock
        let pattern = pattern.replace("%", "");
        let docs: Vec<Document> = self
            .documents
            .read()
            .values()
            .filter(|doc| doc.file_path().to_string_lossy().contains(&pattern))
            .cloned()
            .collect();
        Ok(docs)
    }
}

/// In-memory mock implementation of EmbeddingRepositoryTrait
#[derive(Clone)]
pub struct MockEmbeddingRepository {
    embeddings: Arc<RwLock<HashMap<String, Embedding>>>,
    chunk_index: Arc<RwLock<HashMap<String, String>>>, // chunk_id -> embedding_id
    chunk_to_document: Arc<RwLock<HashMap<String, String>>>, // chunk_id -> document_id
    document_index: Arc<RwLock<HashMap<String, Vec<String>>>>, // document_id -> Vec<embedding_id>
}

impl MockEmbeddingRepository {
    pub fn new() -> Self {
        Self {
            embeddings: Arc::new(RwLock::new(HashMap::new())),
            chunk_index: Arc::new(RwLock::new(HashMap::new())),
            chunk_to_document: Arc::new(RwLock::new(HashMap::new())),
            document_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register the document that a chunk belongs to
    ///
    /// This should be called before creating embeddings for the chunk to enable
    /// find_by_document() to work correctly.
    ///
    /// # Example
    /// ```rust
    /// embedding_repo.register_chunk_document(&chunk.id, &document.id);
    /// embedding_repo.create(&chunk.id, &embedding_vec, "model").await?;
    /// ```
    pub fn register_chunk_document(&self, chunk_id: &str, document_id: &str) {
        self.chunk_to_document
            .write()
            .insert(chunk_id.to_string(), document_id.to_string());
    }

    pub fn clear(&self) {
        self.embeddings.write().clear();
        self.chunk_index.write().clear();
        self.chunk_to_document.write().clear();
        self.document_index.write().clear();
    }
}

impl Default for MockEmbeddingRepository {
    fn default() -> Self {
        Self::new()
    }
}

/*
#[async_trait]
impl EmbeddingRepositoryTrait for MockEmbeddingRepository {
    async fn create(&self, chunk_id: &str, embedding: &[f32], model_name: &str) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let dimension = embedding.len() as i32;

        let emb = Embedding {
            id: id.clone(),
            chunk_id: chunk_id.to_string(),
            embedding: embedding.to_vec(),
            model_name: model_name.to_string(),
            dimension,
        };

        self.embeddings.write().insert(id.clone(), emb);
        self.chunk_index
            .write()
            .insert(chunk_id.to_string(), id.clone());

        if let Some(document_id) = self.chunk_to_document.read().get(chunk_id) {
            self.document_index
                .write()
                .entry(document_id.clone())
                .or_insert_with(Vec::new)
                .push(id.clone());
        }

        Ok(id)
    }

    async fn create_batch(
        &self,
        chunk_embeddings: Vec<(&str, Vec<f32>)>,
        model_name: &str,
    ) -> Result<Vec<String>> {
        let mut embedding_ids = Vec::new();

        for (chunk_id, embedding) in chunk_embeddings {
            let id = self.create(chunk_id, &embedding, model_name).await?;
            embedding_ids.push(id);
        }

        Ok(embedding_ids)
    }

    async fn find_by_chunk(&self, chunk_id: &str) -> Result<Option<Embedding>> {
        let chunk_index = self.chunk_index.read();
        if let Some(embedding_id) = chunk_index.get(chunk_id) {
            Ok(self.embeddings.read().get(embedding_id).cloned())
        } else {
            Ok(None)
        }
    }

    async fn find_by_document(&self, document_id: &str) -> Result<Vec<Embedding>> {
        let doc_index = self.document_index.read();
        let embedding_ids = doc_index.get(document_id).cloned().unwrap_or_default();

        let embeddings = self.embeddings.read();
        Ok(embedding_ids
            .iter()
            .filter_map(|id| embeddings.get(id).cloned())
            .collect())
    }

    async fn delete_by_chunk(&self, chunk_id: &str) -> Result<()> {
        let mut chunk_index = self.chunk_index.write();
        if let Some(embedding_id) = chunk_index.remove(chunk_id) {
            self.embeddings.write().remove(&embedding_id);

            // Also remove from document index if tracked
            if let Some(document_id) = self.chunk_to_document.read().get(chunk_id) {
                if let Some(embedding_ids) = self.document_index.write().get_mut(document_id) {
                    embedding_ids.retain(|id| id != &embedding_id);
                }
            }
        }
        Ok(())
    }

    async fn delete_by_document(&self, document_id: &str) -> Result<()> {
        let mut document_index = self.document_index.write();
        if let Some(embedding_ids) = document_index.remove(document_id) {
            let mut embeddings = self.embeddings.write();
            let mut chunk_index = self.chunk_index.write();

            for embedding_id in embedding_ids {
                if let Some(embedding) = embeddings.remove(&embedding_id) {
                    chunk_index.remove(&embedding.chunk_id);
                }
            }
        }
        Ok(())
    }

    async fn count(&self) -> Result<i64> {
        Ok(self.embeddings.read().len() as i64)
    }
}
*/

#[async_trait]
impl EmbeddingRepositoryPort for MockEmbeddingRepository {
    async fn create(&self, chunk_id: &str, vector: &[f32], model: &str) -> Result<String> {
        use crate::shared::domain_types::ChunkId;
        let chunk_id_typed = ChunkId::from(chunk_id.to_string());
        let embedding = DomainEmbedding::new(chunk_id_typed, model.to_string(), vector.len());
        self.save(&embedding, vector.to_vec()).await?;
        Ok(embedding.id().to_string())
    }

    async fn find_by_chunk(&self, chunk_id: &str) -> Result<Option<DomainEmbedding>> {
        let result = self.find_by_chunk_id(chunk_id).await?;
        Ok(result.map(|(entity, _vector)| entity))
    }

    async fn save(&self, entity: &DomainEmbedding, vector: Vec<f32>) -> Result<()> {
        EmbeddingMapper::validate_dimension(entity, &vector)?;

        let dto = EmbeddingMapper::to_dto(entity, vector);
        let embedding_id = uuid::Uuid::new_v4().to_string();

        let embedding = Embedding {
            id: embedding_id.clone(),
            chunk_id: dto.chunk_id.clone(),
            embedding: dto.embedding,
            model_name: dto.model_name,
            created_at: Utc::now().to_rfc3339(),
        };

        self.embeddings
            .write()
            .insert(embedding_id.clone(), embedding);
        self.chunk_index
            .write()
            .insert(dto.chunk_id.clone(), embedding_id.clone());

        if let Some(document_id) = self.chunk_to_document.read().get(&dto.chunk_id) {
            self.document_index
                .write()
                .entry(document_id.clone())
                .or_default()
                .push(embedding_id);
        }

        Ok(())
    }

    async fn save_batch(&self, entries: Vec<(DomainEmbedding, Vec<f32>)>) -> Result<()> {
        for (entity, vector) in entries {
            self.save(&entity, vector).await?;
        }
        Ok(())
    }

    async fn find_by_chunk_id(
        &self,
        chunk_id: &str,
    ) -> Result<Option<(DomainEmbedding, Vec<f32>)>> {
        let chunk_index = self.chunk_index.read();
        if let Some(embedding_id) = chunk_index.get(chunk_id) {
            let embeddings = self.embeddings.read();
            if let Some(emb) = embeddings.get(embedding_id) {
                let dto = EmbeddingDTO {
                    chunk_id: emb.chunk_id.clone(),
                    embedding: emb.embedding.clone(),
                    model_name: emb.model_name.clone(),
                    dimension: emb.embedding.len(), // Derive from vector length
                    computed_at: Utc::now(),
                    model_version: None,
                };

                let entity = EmbeddingMapper::from_dto(&dto)?;
                Ok(Some((entity, emb.embedding.clone())))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    async fn find_by_document_id(
        &self,
        document_id: &str,
    ) -> Result<Vec<(DomainEmbedding, Vec<f32>)>> {
        let document_index = self.document_index.read();
        if let Some(embedding_ids) = document_index.get(document_id) {
            let embeddings = self.embeddings.read();
            let mut results = Vec::new();

            for embedding_id in embedding_ids {
                if let Some(emb) = embeddings.get(embedding_id) {
                    let dto = EmbeddingDTO {
                        chunk_id: emb.chunk_id.clone(),
                        embedding: emb.embedding.clone(),
                        model_name: emb.model_name.clone(),
                        dimension: emb.embedding.len(), // Derive from vector length
                        computed_at: Utc::now(),
                        model_version: None,
                    };

                    let entity = EmbeddingMapper::from_dto(&dto)?;
                    results.push((entity, emb.embedding.clone()));
                }
            }

            Ok(results)
        } else {
            Ok(Vec::new())
        }
    }

    async fn delete_by_chunk_id(&self, chunk_id: &str) -> Result<()> {
        let mut chunk_index = self.chunk_index.write();
        if let Some(embedding_id) = chunk_index.remove(chunk_id) {
            self.embeddings.write().remove(&embedding_id);

            // Also remove from document index if tracked
            if let Some(document_id) = self.chunk_to_document.read().get(chunk_id) {
                if let Some(embedding_ids) = self.document_index.write().get_mut(document_id) {
                    embedding_ids.retain(|id| id != &embedding_id);
                }
            }
        }
        Ok(())
    }

    async fn delete_by_document_id(&self, document_id: &str) -> Result<()> {
        let mut document_index = self.document_index.write();
        if let Some(embedding_ids) = document_index.remove(document_id) {
            let mut embeddings = self.embeddings.write();
            let mut chunk_index = self.chunk_index.write();

            for embedding_id in embedding_ids {
                if let Some(emb) = embeddings.remove(&embedding_id) {
                    chunk_index.remove(&emb.chunk_id);
                }
            }
        }
        Ok(())
    }

    async fn count(&self) -> Result<i64> {
        Ok(self.embeddings.read().len() as i64)
    }

    async fn create_batch(&self, entries: Vec<(DomainEmbedding, Vec<f32>)>) -> Result<Vec<String>> {
        let mut ids = Vec::new();
        for (entity, vector) in entries {
            let id = entity.id().to_string();
            self.save(&entity, vector).await?;
            ids.push(id);
        }
        Ok(ids)
    }
}

// MockTagRepository REMOVED - migrated to DDD
// The old MockTagRepository using crate::models::tag::Tag has been removed.
// Use the DDD version instead:
// - Location: src/shared/traits.rs
// - Uses: crate::domain::entities::tag::Tag (domain entity)
// - Trait: crate::shared::traits::TagRepositoryTrait
// Migration note: This mock was obsolete and using the old Tag model.
// The canonical mock is in shared/traits.rs with proper DDD structure.

/// In-memory mock implementation of MentionRepositoryTrait (LEGACY - migrated to DDD)
#[derive(Clone)]
pub struct MockMentionRepository {
    mentions: Arc<RwLock<HashMap<String, Mention>>>,
    name_index: Arc<RwLock<HashMap<String, String>>>, // name -> id
    mention_documents: Arc<RwLock<HashMap<String, Vec<String>>>>, // mention_id -> document_ids
}

impl MockMentionRepository {
    pub fn new() -> Self {
        Self {
            mentions: Arc::new(RwLock::new(HashMap::new())),
            name_index: Arc::new(RwLock::new(HashMap::new())),
            mention_documents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn clear(&self) {
        self.mentions.write().clear();
        self.name_index.write().clear();
        self.mention_documents.write().clear();
    }
}

impl Default for MockMentionRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl crate::application::ports::MentionRepositoryPort for MockMentionRepository {
    async fn create_mention(
        &self,
        name: &str,
        mention_type: &str,
        metadata: Option<&str>,
    ) -> Result<crate::application::ports::MentionData, AppError> {
        use crate::shared::domain_types::{ChunkId, DocumentId};
        use uuid::Uuid;

        // For the mock, we need to create a simplified representation
        // Real Mention entity requires document_id and chunk_id which we don't have here
        // So we'll just store the data we need for the port without using the full entity

        let id_str = Uuid::new_v4().to_string();
        let created_at = Utc::now().to_rfc3339();

        if let Some(existing_id) = self.name_index.read().get(name).cloned() {
            if let Some(_mention) = self.mentions.read().get(&existing_id) {
                return Ok(crate::application::ports::MentionData {
                    id: existing_id,
                    name: name.to_string(),
                    mention_type: mention_type.to_string(),
                    metadata: metadata.map(|s| s.to_string()),
                    created_at,
                });
            }
        }

        let mention = Mention::new(
            DocumentId::new(), // Dummy document ID
            ChunkId::new(),    // Dummy chunk ID
            name.to_string(),
            mention_type.parse().unwrap_or_default(),
            0,
            name.len(),
        );

        self.mentions.write().insert(id_str.clone(), mention);
        self.name_index
            .write()
            .insert(name.to_string(), id_str.clone());

        Ok(crate::application::ports::MentionData {
            id: id_str,
            name: name.to_string(),
            mention_type: mention_type.to_string(),
            metadata: metadata.map(|s| s.to_string()),
            created_at,
        })
    }

    async fn find_mention_by_name(
        &self,
        name: &str,
    ) -> Result<Option<crate::application::ports::MentionData>, AppError> {
        let name_index = self.name_index.read();
        if let Some(id) = name_index.get(name) {
            if let Some(mention) = self.mentions.read().get(id).cloned() {
                return Ok(Some(crate::application::ports::MentionData {
                    id: id.clone(),
                    name: mention.text().to_string(),
                    mention_type: mention.mention_type().to_string(),
                    metadata: None,
                    created_at: mention.created_at().to_rfc3339(),
                }));
            }
        }
        Ok(None)
    }

    async fn search_mentions(
        &self,
        query: &str,
        limit: i64,
    ) -> Result<Vec<crate::application::ports::MentionData>, AppError> {
        let mentions = self.mentions.read();
        let name_index = self.name_index.read();

        let mut id_to_name: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for (name, id) in name_index.iter() {
            id_to_name.insert(id.clone(), name.clone());
        }

        let mut results: Vec<crate::application::ports::MentionData> = mentions
            .iter()
            .filter(|(_, m)| m.text().contains(query))
            .map(|(id, m)| crate::application::ports::MentionData {
                id: id.clone(),
                name: id_to_name
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| m.text().to_string()),
                mention_type: m.mention_type().to_string(),
                metadata: None,
                created_at: m.created_at().to_rfc3339(),
            })
            .collect();
        results.sort_by(|a, b| a.name.cmp(&b.name));
        results.truncate(limit as usize);
        Ok(results)
    }

    async fn get_mentions_by_type(
        &self,
        mention_type: &str,
    ) -> Result<Vec<crate::application::ports::MentionData>, AppError> {
        let mentions = self.mentions.read();
        let name_index = self.name_index.read();

        let mut id_to_name: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for (name, id) in name_index.iter() {
            id_to_name.insert(id.clone(), name.clone());
        }

        let mut results: Vec<crate::application::ports::MentionData> = mentions
            .iter()
            .filter(|(_, m)| m.mention_type().to_string() == mention_type)
            .map(|(id, m)| crate::application::ports::MentionData {
                id: id.clone(),
                name: id_to_name
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| m.text().to_string()),
                mention_type: m.mention_type().to_string(),
                metadata: None,
                created_at: m.created_at().to_rfc3339(),
            })
            .collect();
        results.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(results)
    }

    async fn get_mentions_for_document(
        &self,
        _document_id: &str,
    ) -> Result<Vec<crate::application::ports::MentionWithContextData>, AppError> {
        Ok(vec![])
    }

    async fn get_documents_with_mention(&self, mention_id: &str) -> Result<Vec<String>, AppError> {
        Ok(self
            .mention_documents
            .read()
            .get(mention_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn extract_and_store_mentions(
        &self,
        _document_id: &str,
        _text: &str,
    ) -> Result<Vec<crate::application::ports::MentionWithContextData>, AppError> {
        Ok(vec![])
    }

    async fn update_mention(
        &self,
        id: &str,
        mention_type: Option<&str>,
        metadata: Option<&str>,
    ) -> Result<crate::application::ports::MentionData, AppError> {
        let mentions = self.mentions.read();
        let mention = mentions
            .get(id)
            .ok_or_else(|| AppError::NotFound(format!("Mention not found: {}", id)))?;

        let name_index = self.name_index.read();
        let name = name_index
            .iter()
            .find(|(_, mid)| *mid == id)
            .map(|(n, _)| n.clone())
            .unwrap_or_else(|| mention.text().to_string());

        // Create updated data - convert to owned string to avoid lifetime issues
        let current_type = mention.mention_type().to_string();
        let new_type = mention_type.unwrap_or(&current_type);
        let created_at = mention.created_at().to_rfc3339();

        Ok(crate::application::ports::MentionData {
            id: id.to_string(),
            name,
            mention_type: new_type.to_string(),
            metadata: metadata.map(|s| s.to_string()),
            created_at,
        })
    }

    async fn delete_mention(&self, id: &str) -> Result<(), AppError> {
        if let Some(_mention) = self.mentions.write().remove(id) {
            let mut name_to_remove = None;
            for (name, mention_id) in self.name_index.read().iter() {
                if mention_id == id {
                    name_to_remove = Some(name.clone());
                    break;
                }
            }
            if let Some(name) = name_to_remove {
                self.name_index.write().remove(&name);
            }
            self.mention_documents.write().remove(id);
        }
        Ok(())
    }
}

// Legacy MentionRepositoryTrait implementation - MIGRATED TO DDD
// This implementation has been replaced with MentionRepositoryPort.
// See src/infrastructure/services/mocks/mock_mention.rs for the new DDD implementation.
// See src/infrastructure/services/traits/mention.rs for migration guide.
// Migration:
// - OLD: Arc<dyn MentionRepositoryTrait>
// - NEW: Arc<dyn MentionRepositoryPort>
// #[async_trait]
// impl MentionRepositoryTrait for MockMentionRepository {
//     async fn create_mention(
//         &self,
//         name: &str,
//         mention_type: &str,
//         metadata: Option<&str>,
//     ) -> Result<Mention> {
//         // Check if exists, update if so
//         if let Some(existing_id) = self.name_index.read().get(name).cloned() {
//             let mut mentions = self.mentions.write();
//             if let Some(mention) = mentions.get_mut(&existing_id) {
//                 mention.mention_type = mention_type.to_string();
//                 mention.metadata = metadata.map(|s| s.to_string());
//                 return Ok(mention.clone());
//             }
//         }
//         let id = Uuid::new_v4().to_string();
//         let created_at = Utc::now().to_rfc3339();
//         let mention = Mention {
//             id: id.clone(),
//             name: name.to_string(),
//             mention_type: mention_type.to_string(),
//             metadata: metadata.map(|s| s.to_string()),
//             created_at,
//         };
//         self.mentions.write().insert(id.clone(), mention.clone());
//         self.name_index.write().insert(name.to_string(), id);
//         Ok(mention)
//     }
//     async fn find_mention_by_name(&self, name: &str) -> Result<Option<Mention>> {
//         let name_index = self.name_index.read();
//         if let Some(id) = name_index.get(name) {
//             Ok(self.mentions.read().get(id).cloned())
//         } else {
//             Ok(None)
//         }
//     }
//     async fn search_mentions(&self, query: &str, limit: i64) -> Result<Vec<Mention>> {
//         let mentions = self.mentions.read();
//         let mut results: Vec<Mention> = mentions
//             .values()
//             .filter(|m| m.name.contains(query))
//             .cloned()
//             .collect();
//         results.sort_by(|a, b| a.name.cmp(&b.name));
//         results.truncate(limit as usize);
//         Ok(results)
//     }
//     async fn get_mentions_by_type(&self, mention_type: &str) -> Result<Vec<Mention>> {
//         let mentions = self.mentions.read();
//         let mut results: Vec<Mention> = mentions
//             .values()
//             .filter(|m| m.mention_type == mention_type)
//             .cloned()
//             .collect();
//         results.sort_by(|a, b| a.name.cmp(&b.name));
//         Ok(results)
//     }
//     async fn link_mention_to_document(
//         &self,
//         document_id: &str,
//         mention_id: &str,
//         context: Option<&str>,
//         position: Option<i64>,
//     ) -> Result<DocumentMention> {
//         let id = Uuid::new_v4().to_string();
//         let created_at = Utc::now().to_rfc3339();
//         let doc_mention = DocumentMention {
//             id,
//             document_id: document_id.to_string(),
//             mention_id: mention_id.to_string(),
//             context: context.map(|s| s.to_string()),
//             position,
//             created_at,
//         };
//         self.document_mentions
//             .write()
//             .entry(document_id.to_string())
//             .or_insert_with(Vec::new)
//             .push(doc_mention.clone());
//         self.mention_documents
//             .write()
//             .entry(mention_id.to_string())
//             .or_insert_with(Vec::new)
//             .push(document_id.to_string());
//         Ok(doc_mention)
//     }
//     async fn get_mentions_for_document(
//         &self,
//         document_id: &str,
//     ) -> Result<Vec<MentionWithContext>> {
//         let document_mentions = self.document_mentions.read();
//         if let Some(doc_mentions) = document_mentions.get(document_id) {
//             let mentions_map = self.mentions.read();
//             let mut results: Vec<MentionWithContext> = doc_mentions
//                 .iter()
//                 .filter_map(|dm| {
//                     mentions_map
//                         .get(&dm.mention_id)
//                         .map(|m| MentionWithContext {
//                             mention: m.clone(),
//                             context: dm.context.clone(),
//                             position: dm.position,
//                         })
//                 })
//                 .collect();
//             results.sort_by(|a, b| a.position.cmp(&b.position));
//             Ok(results)
//         } else {
//             Ok(Vec::new())
//         }
//     }
//     async fn get_documents_with_mention(&self, mention_id: &str) -> Result<Vec<String>> {
//         Ok(self
//             .mention_documents
//             .read()
//             .get(mention_id)
//             .cloned()
//             .unwrap_or_default())
//     }
//     async fn clear_document_mentions(&self, document_id: &str) -> Result<()> {
//         if let Some(doc_mentions) = self.document_mentions.write().remove(document_id) {
//             let mut mention_documents = self.mention_documents.write();
//             for dm in doc_mentions {
//                 if let Some(docs) = mention_documents.get_mut(&dm.mention_id) {
//                     docs.retain(|id| id != document_id);
//                 }
//             }
//         }
//         Ok(())
//     }
//     async fn delete_mention(&self, id: &str) -> Result<()> {
//         if let Some(mention) = self.mentions.write().remove(id) {
//             self.name_index.write().remove(&mention.name);
//             if let Some(doc_ids) = self.mention_documents.write().remove(id) {
//                 let mut document_mentions = self.document_mentions.write();
//                 for doc_id in doc_ids {
//                     if let Some(mentions) = document_mentions.get_mut(&doc_id) {
//                         mentions.retain(|dm| dm.mention_id != id);
//                     }
//                 }
//             }
//         }
//         Ok(())
//     }
//     fn extract_mentions_from_text(&self, text: &str) -> HashMap<String, Vec<(String, usize)>> {
//         use once_cell::sync::Lazy;
//         use regex::Regex;
//         static AT_MENTION_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"@\[([^\]]+)\]").unwrap());
//         static WIKILINK_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[\[([^\]]+)\]\]").unwrap());
//         let mut mentions: HashMap<String, Vec<(String, usize)>> = HashMap::new();
//         for cap in AT_MENTION_RE.captures_iter(text) {
//             let name = cap[1].to_string();
//             let position = cap.get(0).unwrap().start();
//             mentions
//                 .entry("person".to_string())
//                 .or_insert_with(Vec::new)
//                 .push((name, position));
//         }
//         for cap in WIKILINK_RE.captures_iter(text) {
//             let name = cap[1].to_string();
//             let position = cap.get(0).unwrap().start();
//             mentions
//                 .entry("wikilink".to_string())
//                 .or_insert_with(Vec::new)
//                 .push((name, position));
//         }
//         mentions
//     }
//     async fn extract_and_store_mentions(
//         &self,
//         document_id: &str,
//         text: &str,
//     ) -> Result<Vec<MentionWithContext>> {
//         self.clear_document_mentions(document_id).await?;
//         let extracted = self.extract_mentions_from_text(text);
//         let mut results = Vec::new();
//         for (mention_type, occurrences) in extracted {
//             for (name, position) in occurrences {
//                 let mention = self.create_mention(&name, &mention_type, None).await?;
//                 let start = position.saturating_sub(50);
//                 let end = (position + 50).min(text.len());
//                 let context = text[start..end].to_string();
//                 self.link_mention_to_document(
//                     document_id,
//                     &mention.id,
//                     Some(&context),
//                     Some(position as i64),
//                 )
//                 .await?;
//                 results.push(MentionWithContext {
//                     mention,
//                     context: Some(context),
//                     position: Some(position as i64),
//                 });
//             }
//         }
//         Ok(results)
//     }
// }
