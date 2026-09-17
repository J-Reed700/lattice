//! Mock implementations for testing
//!
//! This module provides mock implementations of service traits.

#[cfg(test)]
use super::trait_def::IndexStorageTrait;
#[cfg(test)]
use async_trait::async_trait;
#[cfg(test)]
use std::path::Path;
#[cfg(test)]
use std::sync::{Arc, RwLock};

#[cfg(test)]
type StoredChunk = (
    String,
    crate::features::indexing::engine::chunker::TextChunk,
    Vec<f32>,
);

#[cfg(test)]
/// Mock index storage for testing.
///
/// Provides in-memory storage for testing without requiring a database.
pub struct MockIndexStorage {
    documents: Arc<
        RwLock<
            std::collections::HashMap<
                String,
                crate::features::indexing::engine::storage::DocumentRecord,
            >,
        >,
    >,
    chunks: Arc<RwLock<Vec<StoredChunk>>>,
    indexed_count: Arc<RwLock<i64>>,
}

#[cfg(test)]
impl Default for MockIndexStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl MockIndexStorage {
    /// Create a new mock index storage.
    pub fn new() -> Self {
        Self {
            documents: Arc::new(RwLock::new(std::collections::HashMap::new())),
            chunks: Arc::new(RwLock::new(Vec::new())),
            indexed_count: Arc::new(RwLock::new(0)),
        }
    }

    /// Add a document to the mock storage.
    pub fn add_document(
        &self,
        path: &str,
        record: crate::features::indexing::engine::storage::DocumentRecord,
    ) {
        self.documents
            .write()
            .unwrap()
            .insert(path.to_string(), record);
    }

    /// Add chunks for a document.
    pub fn add_chunks(
        &self,
        doc_id: String,
        chunks: Vec<crate::features::indexing::engine::chunker::TextChunk>,
        embeddings: Vec<Vec<f32>>,
    ) {
        let mut chunks_guard = self.chunks.write().unwrap();
        for (chunk, embedding) in chunks.into_iter().zip(embeddings) {
            chunks_guard.push((doc_id.clone(), chunk, embedding));
        }
    }

    /// Clear all data.
    pub fn clear(&self) {
        self.documents.write().unwrap().clear();
        self.chunks.write().unwrap().clear();
        *self.indexed_count.write().unwrap() = 0;
    }

    /// Get document count.
    pub fn len(&self) -> usize {
        self.documents.read().unwrap().len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.documents.read().unwrap().is_empty()
    }
}

#[async_trait]
#[cfg(test)]
impl IndexStorageTrait for MockIndexStorage {
    async fn store_document(
        &self,
        path: &Path,
        mime_type: &str,
        chunks: Vec<crate::features::indexing::engine::chunker::TextChunk>,
        embeddings: Vec<Vec<f32>>,
    ) -> crate::features::indexing::engine::error::Result<String> {
        let doc_id = uuid::Uuid::new_v4().to_string();
        let path_str = path.to_string_lossy().to_string();

        let record = crate::features::indexing::engine::storage::DocumentRecord {
            id: doc_id.clone(),
            file_path: path_str.clone(),
            file_name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            file_type: None,
            mime_type: mime_type.to_string(),
            size_bytes: 0,
            checksum: "mock_checksum".to_string(),
            status: "indexed".to_string(),
            path: path_str.clone(),
            indexed_at: chrono::Utc::now().to_rfc3339(),
        };

        self.documents.write().unwrap().insert(path_str, record);

        self.add_chunks(doc_id.clone(), chunks, embeddings);

        *self.indexed_count.write().unwrap() += 1;

        Ok(doc_id)
    }

    async fn document_exists(
        &self,
        path: &Path,
    ) -> crate::features::indexing::engine::error::Result<bool> {
        let path_str = path.to_string_lossy().to_string();
        Ok(self.documents.read().unwrap().contains_key(&path_str))
    }

    async fn get_document_by_path(
        &self,
        path: &Path,
    ) -> crate::features::indexing::engine::error::Result<
        Option<crate::features::indexing::engine::storage::DocumentRecord>,
    > {
        let path_str = path.to_string_lossy().to_string();
        Ok(self.documents.read().unwrap().get(&path_str).cloned())
    }

    async fn needs_reindex(
        &self,
        path: &Path,
    ) -> crate::features::indexing::engine::error::Result<bool> {
        let _ = path;
        Ok(false)
    }

    async fn mark_document_status(
        &self,
        path: &Path,
        status: &str,
    ) -> crate::features::indexing::engine::error::Result<()> {
        let path_str = path.to_string_lossy().to_string();
        if let Some(doc) = self.documents.write().unwrap().get_mut(&path_str) {
            doc.status = status.to_string();
        }
        Ok(())
    }

    async fn remove_document(
        &self,
        path: &Path,
    ) -> crate::features::indexing::engine::error::Result<()> {
        let path_str = path.to_string_lossy().to_string();
        self.documents.write().unwrap().remove(&path_str);

        self.chunks
            .write()
            .unwrap()
            .retain(|(_, chunk, _)| chunk.document_path != path_str);

        Ok(())
    }

    async fn store_file_metadata_only(
        &self,
        path: &Path,
        file_id: &str,
        mime_type: &str,
    ) -> crate::features::indexing::engine::error::Result<String> {
        let path_str = path.to_string_lossy().to_string();

        let record = crate::features::indexing::engine::storage::DocumentRecord {
            id: file_id.to_string(),
            file_path: path_str.clone(),
            file_name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            file_type: None,
            mime_type: mime_type.to_string(),
            size_bytes: 0,
            checksum: "mock_checksum".to_string(),
            status: "pending".to_string(),
            path: path_str.clone(),
            indexed_at: chrono::Utc::now().to_rfc3339(),
        };

        self.documents.write().unwrap().insert(path_str, record);

        Ok(file_id.to_string())
    }

    async fn get_indexed_count(&self) -> crate::features::indexing::engine::error::Result<i64> {
        Ok(*self.indexed_count.read().unwrap())
    }

    async fn get_total_chunks(&self) -> crate::features::indexing::engine::error::Result<i64> {
        Ok(self.chunks.read().unwrap().len() as i64)
    }

    async fn batch_store_documents(
        &self,
        documents: Vec<(
            std::path::PathBuf,
            String,
            Vec<crate::features::indexing::engine::chunker::TextChunk>,
            Vec<Vec<f32>>,
        )>,
    ) -> crate::features::indexing::engine::error::Result<Vec<String>> {
        let mut doc_ids = Vec::new();

        for (path, mime_type, chunks, embeddings) in documents {
            let doc_id = self
                .store_document(&path, &mime_type, chunks, embeddings)
                .await?;
            doc_ids.push(doc_id);
        }

        Ok(doc_ids)
    }

    async fn store_document_with_context(
        &self,
        path: &Path,
        mime_type: &str,
        chunks: Vec<crate::features::indexing::engine::chunker::ContextualizedChunk>,
        embeddings: Vec<Vec<f32>>,
    ) -> crate::features::indexing::engine::error::Result<String> {
        let doc_id = uuid::Uuid::new_v4().to_string();
        let path_str = path.to_string_lossy().to_string();

        let record = crate::features::indexing::engine::storage::DocumentRecord {
            id: doc_id.clone(),
            file_path: path_str.clone(),
            file_name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            file_type: None,
            mime_type: mime_type.to_string(),
            size_bytes: 0,
            checksum: "mock_checksum".to_string(),
            status: "indexed".to_string(),
            path: path_str.clone(),
            indexed_at: chrono::Utc::now().to_rfc3339(),
        };

        self.documents
            .write()
            .unwrap()
            .insert(path_str.clone(), record);

        let chunks: Vec<_> = chunks
            .into_iter()
            .map(|c| crate::features::indexing::engine::chunker::TextChunk {
                text: c.contextualized_content,
                start_idx: c.start_idx,
                end_idx: c.end_idx,
                token_count: c.token_count,
                document_path: path_str.clone(),
                chunk_index: c.chunk_index,
            })
            .collect();

        self.add_chunks(doc_id.clone(), chunks, embeddings);

        *self.indexed_count.write().unwrap() += 1;

        Ok(doc_id)
    }

    async fn store_document_with_context_and_file(
        &self,
        path: &Path,
        file_id: &str,
        mime_type: &str,
        chunks: Vec<crate::features::indexing::engine::chunker::ContextualizedChunk>,
        embeddings: Vec<Vec<f32>>,
    ) -> crate::features::indexing::engine::error::Result<String> {
        let path_str = path.to_string_lossy().to_string();

        let record = crate::features::indexing::engine::storage::DocumentRecord {
            id: file_id.to_string(),
            file_path: path_str.clone(),
            file_name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            file_type: None,
            mime_type: mime_type.to_string(),
            size_bytes: 0,
            checksum: "mock_checksum".to_string(),
            status: "indexed".to_string(),
            path: path_str.clone(),
            indexed_at: chrono::Utc::now().to_rfc3339(),
        };

        self.documents
            .write()
            .unwrap()
            .insert(path_str.clone(), record);

        let chunks: Vec<_> = chunks
            .into_iter()
            .map(|c| crate::features::indexing::engine::chunker::TextChunk {
                text: c.contextualized_content,
                start_idx: c.start_idx,
                end_idx: c.end_idx,
                token_count: c.token_count,
                document_path: path_str.clone(),
                chunk_index: c.chunk_index,
            })
            .collect();

        self.add_chunks(file_id.to_string(), chunks, embeddings);

        *self.indexed_count.write().unwrap() += 1;

        Ok(file_id.to_string())
    }
}
