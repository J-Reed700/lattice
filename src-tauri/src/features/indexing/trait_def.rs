//! Service trait definitions
//!
//! This module defines trait interfaces for dependency injection.

use async_trait::async_trait;
use std::path::Path;

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
