//! In-memory `MockDocumentRepository` over the DDD document ports.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;

use crate::infrastructure::persistence::repositories::traits::DocumentRepositoryTrait;
use lattice_core::application::ports::{DocumentRepositoryPort, Filter, RepositoryPort};
use lattice_core::domain::entities::Document;
use lattice_core::shared::error::{AppError, Result};

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
/// use lattice_core::application::ports::{RepositoryPort, DocumentRepositoryPort};
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
        checksum: &lattice_core::domain::value_objects::Checksum,
    ) -> Result<Option<lattice_core::domain::entities::Document>> {
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

    async fn count_by_checksum(&self, checksum: &str) -> Result<u64> {
        let docs = self.documents.read();
        Ok(docs
            .values()
            .filter(|doc| doc.checksum().as_str() == checksum)
            .count() as u64)
    }

    async fn list_checksums(&self) -> Result<Vec<String>> {
        let docs = self.documents.read();
        let mut checksums: Vec<String> = docs
            .values()
            .map(|doc| doc.checksum().as_str().to_string())
            .collect();
        checksums.sort();
        checksums.dedup();
        Ok(checksums)
    }

    async fn find_checksum_by_id(&self, document_id: &str) -> Result<String> {
        self.documents
            .read()
            .get(document_id)
            .map(|doc| doc.checksum().as_str().to_string())
            .ok_or_else(|| AppError::NotFound(format!("Document not found: {document_id}")))
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
        use lattice_core::shared::types::ValidatedFilePath;
        use std::path::PathBuf;

        let validated_path = ValidatedFilePath::new(PathBuf::from(file_path))?;
        let checksum_vo =
            lattice_core::domain::value_objects::checksum::Checksum::new(checksum.to_string())?;
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
