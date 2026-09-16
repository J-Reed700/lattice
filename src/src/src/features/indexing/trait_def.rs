//! Service trait definitions
//!
//! This module defines trait interfaces for dependency injection.

use async_trait::async_trait;
use std::path::Path;

#[async_trait]
pub trait IndexingServiceTrait: Send + Sync {
    /// Index a single file
    ///
    /// Queues the file for indexing. The actual processing happens asynchronously.
    ///
    /// # Arguments
    /// * `path` - Path to the file to index
    ///
    /// # Returns
    /// Ok(()) if successfully queued
    ///
    /// # Errors
    /// - `IndexingError::QueueFull` if the indexing queue is full
    /// - `IndexingError::FileNotFound` if the file doesn't exist
    ///
    /// # Example
    /// ```rust
    /// service.index_file(PathBuf::from("/path/to/file.txt")).await?;
    /// ```
    async fn index_file(
        &self,
        path: std::path::PathBuf,
    ) -> crate::features::indexing::engine::error::Result<()>;

    /// Index all files in a folder
    ///
    /// Scans the folder and queues all indexable files for processing.
    ///
    /// # Arguments
    /// * `path` - Path to the folder to index
    /// * `recursive` - Whether to recursively index subdirectories
    ///
    /// # Returns
    /// Ok(()) if successfully queued
    ///
    /// # Example
    /// ```rust
    /// // Index folder recursively
    /// service.index_folder(PathBuf::from("/docs"), true).await?;
    /// ```
    async fn index_folder(
        &self,
        path: std::path::PathBuf,
        recursive: bool,
    ) -> crate::features::indexing::engine::error::Result<()>;

    /// Reindex an existing file
    ///
    /// Forces reindexing of a file even if it hasn't changed.
    ///
    /// # Arguments
    /// * `path` - Path to the file to reindex
    ///
    /// # Returns
    /// Ok(()) if successfully queued
    async fn reindex_file(
        &self,
        path: std::path::PathBuf,
    ) -> crate::features::indexing::engine::error::Result<()>;

    /// Remove a file from the index
    ///
    /// Deletes all indexed chunks and metadata for the file.
    ///
    /// # Arguments
    /// * `path` - Path to the file to remove
    ///
    /// # Returns
    /// Ok(()) if successfully queued
    async fn remove_file(
        &self,
        path: std::path::PathBuf,
    ) -> crate::features::indexing::engine::error::Result<()>;

    /// Cancel all ongoing indexing operations
    ///
    /// Stops the indexing actor and clears the queue.
    ///
    /// # Returns
    /// Ok(()) if cancellation was successful
    async fn cancel_all(&self) -> crate::features::indexing::engine::error::Result<()>;

    /// Get current indexing progress
    ///
    /// Returns a snapshot of the current indexing state.
    ///
    /// # Returns
    /// IndexProgress struct with total, processed, and failed counts
    ///
    /// # Example
    /// ```rust
    /// let progress = service.get_progress().await;
    /// println!("Progress: {}/{}", progress.processed, progress.total_files);
    /// ```
    async fn get_progress(&self) -> crate::features::indexing::engine::progress::IndexProgress;

    /// Subscribe to progress updates
    ///
    /// Returns a broadcast receiver that receives progress updates in real-time.
    ///
    /// # Returns
    /// Broadcast receiver for IndexProgress updates
    ///
    /// # Example
    /// ```rust
    /// let mut rx = service.subscribe_progress().await;
    /// while let Ok(progress) = rx.recv().await {
    ///     println!("Updated: {:.1}%", progress.percentage);
    /// }
    /// ```
    async fn subscribe_progress(
        &self,
    ) -> tokio::sync::broadcast::Receiver<crate::features::indexing::engine::progress::IndexProgress>;

    async fn pause_indexing(&self) -> crate::features::indexing::engine::error::Result<()>;

    async fn resume_indexing(&self) -> crate::features::indexing::engine::error::Result<()>;
}

/// Mock implementation of IndexingServiceTrait for testing
///
/// Simulates indexing operations without actual file processing.
/// Useful for testing UI components and progress tracking logic.

#[async_trait]
pub trait IndexStorageTrait: Send + Sync {
    /// Store a document with text chunks and embeddings.
    ///
    /// # Arguments
    /// * `path` - Path to the document
    /// * `mime_type` - MIME type of the document
    /// * `chunks` - Text chunks
    /// * `embeddings` - Vector embeddings for each chunk
    ///
    /// # Returns
    /// Document ID
    async fn store_document(
        &self,
        path: &Path,
        mime_type: &str,
        chunks: Vec<crate::features::indexing::engine::chunker::TextChunk>,
        embeddings: Vec<Vec<f32>>,
    ) -> crate::features::indexing::engine::error::Result<String>;

    /// Check if a document exists by path.
    async fn document_exists(
        &self,
        path: &Path,
    ) -> crate::features::indexing::engine::error::Result<bool>;

    /// Get document record by path.
    async fn get_document_by_path(
        &self,
        path: &Path,
    ) -> crate::features::indexing::engine::error::Result<
        Option<crate::features::indexing::engine::storage::DocumentRecord>,
    >;

    /// Check if document needs reindexing.
    async fn needs_reindex(
        &self,
        path: &Path,
    ) -> crate::features::indexing::engine::error::Result<bool>;

    /// Update document status.
    async fn mark_document_status(
        &self,
        path: &Path,
        status: &str,
    ) -> crate::features::indexing::engine::error::Result<()>;

    /// Remove a document.
    async fn remove_document(
        &self,
        path: &Path,
    ) -> crate::features::indexing::engine::error::Result<()>;

    /// Store file metadata only.
    async fn store_file_metadata_only(
        &self,
        path: &Path,
        file_id: &str,
        mime_type: &str,
    ) -> crate::features::indexing::engine::error::Result<String>;

    /// Get count of indexed documents.
    async fn get_indexed_count(&self) -> crate::features::indexing::engine::error::Result<i64>;

    /// Get total number of chunks.
    async fn get_total_chunks(&self) -> crate::features::indexing::engine::error::Result<i64>;

    /// Store multiple documents in batch.
    async fn batch_store_documents(
        &self,
        documents: Vec<(
            std::path::PathBuf,
            String,
            Vec<crate::features::indexing::engine::chunker::TextChunk>,
            Vec<Vec<f32>>,
        )>,
    ) -> crate::features::indexing::engine::error::Result<Vec<String>>;

    /// Store document with contextualized chunks.
    async fn store_document_with_context_for_model(
        &self,
        path: &std::path::Path,
        mime_type: &str,
        chunks: Vec<crate::features::indexing::engine::chunker::ContextualizedChunk>,
        embeddings: Vec<Vec<f32>>,
        _model_identity: &str,
    ) -> crate::shared::error::Result<String> {
        self.store_document_with_context(path, mime_type, chunks, embeddings)
            .await
    }

    async fn store_document_with_context(
        &self,
        path: &Path,
        mime_type: &str,
        chunks: Vec<crate::features::indexing::engine::chunker::ContextualizedChunk>,
        embeddings: Vec<Vec<f32>>,
    ) -> crate::features::indexing::engine::error::Result<String>;

    /// Store document with context and file ID.
    async fn store_document_with_context_and_file(
        &self,
        path: &Path,
        file_id: &str,
        mime_type: &str,
        chunks: Vec<crate::features::indexing::engine::chunker::ContextualizedChunk>,
        embeddings: Vec<Vec<f32>>,
    ) -> crate::features::indexing::engine::error::Result<String>;
}
