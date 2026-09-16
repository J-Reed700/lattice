//! Mock implementations for testing
//!
//! This module provides mock implementations of service traits.

#[cfg(test)]
use super::trait_def::{IndexStorageTrait, IndexingServiceTrait};
#[cfg(test)]
use async_trait::async_trait;
#[cfg(test)]
use std::path::Path;
#[cfg(test)]
use std::sync::{Arc, RwLock};

#[cfg(test)]
/// Mock implementation of IndexingServiceTrait for testing
///
/// Simulates indexing operations without actual file processing.
/// Useful for testing UI components and progress tracking logic.
pub struct MockIndexingService {
    indexed_files: Arc<RwLock<Vec<std::path::PathBuf>>>,
    progress: Arc<RwLock<crate::features::indexing::engine::progress::IndexProgress>>,
    is_indexing: Arc<std::sync::atomic::AtomicBool>,
    progress_tx:
        tokio::sync::broadcast::Sender<crate::features::indexing::engine::progress::IndexProgress>,
}

#[cfg(test)]
impl MockIndexingService {
    /// Create a new mock indexing service
    pub fn new() -> Self {
        let (progress_tx, _) = tokio::sync::broadcast::channel(100);
        Self {
            indexed_files: Arc::new(RwLock::new(Vec::new())),
            progress: Arc::new(RwLock::new(
                crate::features::indexing::engine::progress::IndexProgress::new(),
            )),
            is_indexing: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            progress_tx,
        }
    }

    /// Set the progress state for testing
    pub fn set_progress(
        &self,
        progress: crate::features::indexing::engine::progress::IndexProgress,
    ) {
        *self.progress.write().unwrap() = progress.clone();
        let _ = self.progress_tx.send(progress);
    }

    /// Get list of indexed files
    pub fn get_indexed_files(&self) -> Vec<std::path::PathBuf> {
        self.indexed_files.read().unwrap().clone()
    }

    /// Check if currently indexing
    pub fn is_indexing(&self) -> bool {
        self.is_indexing.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Simulate indexing completion
    pub fn complete_indexing(&self) {
        let mut progress = self.progress.write().unwrap();
        progress.complete();
        self.is_indexing
            .store(false, std::sync::atomic::Ordering::SeqCst);
        let _ = self.progress_tx.send(progress.clone());
    }
}

#[cfg(test)]
impl Default for MockIndexingService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
#[cfg(test)]
impl IndexingServiceTrait for MockIndexingService {
    async fn index_file(
        &self,
        path: std::path::PathBuf,
    ) -> crate::features::indexing::engine::error::Result<()> {
        self.indexed_files.write().unwrap().push(path.clone());
        self.is_indexing
            .store(true, std::sync::atomic::Ordering::SeqCst);

        let mut progress = self.progress.write().unwrap();
        progress.increment_processed();
        let _ = self.progress_tx.send(progress.clone());

        Ok(())
    }

    async fn index_folder(
        &self,
        path: std::path::PathBuf,
        _recursive: bool,
    ) -> crate::features::indexing::engine::error::Result<()> {
        self.indexed_files.write().unwrap().push(path);
        self.is_indexing
            .store(true, std::sync::atomic::Ordering::SeqCst);

        let mut progress = self.progress.write().unwrap();
        progress.total_files = 5; // Mock: assume 5 files
        progress.status = crate::features::indexing::engine::progress::IndexStatus::Processing;
        let _ = self.progress_tx.send(progress.clone());

        Ok(())
    }

    async fn reindex_file(
        &self,
        path: std::path::PathBuf,
    ) -> crate::features::indexing::engine::error::Result<()> {
        self.index_file(path).await
    }

    async fn remove_file(
        &self,
        path: std::path::PathBuf,
    ) -> crate::features::indexing::engine::error::Result<()> {
        let mut files = self.indexed_files.write().unwrap();
        files.retain(|p| p != &path);
        Ok(())
    }

    async fn cancel_all(&self) -> crate::features::indexing::engine::error::Result<()> {
        self.is_indexing
            .store(false, std::sync::atomic::Ordering::SeqCst);
        let mut progress = self.progress.write().unwrap();
        progress.cancel();
        let _ = self.progress_tx.send(progress.clone());
        Ok(())
    }

    async fn get_progress(&self) -> crate::features::indexing::engine::progress::IndexProgress {
        self.progress.read().unwrap().clone()
    }

    async fn subscribe_progress(
        &self,
    ) -> tokio::sync::broadcast::Receiver<crate::features::indexing::engine::progress::IndexProgress>
    {
        self.progress_tx.subscribe()
    }

    async fn pause_indexing(&self) -> crate::features::indexing::engine::error::Result<()> {
        Ok(())
    }

    async fn resume_indexing(&self) -> crate::features::indexing::engine::error::Result<()> {
        Ok(())
    }
}

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
