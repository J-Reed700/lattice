//! Index storage module for managing documents, chunks, and embeddings.
//!
//! This module provides a modular interface for storing and retrieving indexed
//! documents with their text chunks and vector embeddings.
//!
//! # Architecture
//!
//! The module is split into focused sub-modules:
//! - `types`: Data structures (DocumentRecord)
//! - `documents`: Document CRUD operations
//! - `chunks`: Text chunk and embedding storage
//! - `context`: Contextualized chunk storage
//! - `checksum`: File checksum utilities
//! - `stats`: Statistics queries
//!
//! # Example
//!
//! ```no_run
//! use sqlx::SqlitePool;
//! use crate::infrastructure::indexing::storage::IndexStorage;
//!
//! # async fn example(pool: SqlitePool) -> Result<(), Box<dyn std::error::Error>> {
//! let storage = IndexStorage::new(pool);
//! let exists = storage.document_exists(Path::new("doc.pdf")).await?;
//! # Ok(())
//! # }
//! ```

mod checksum;
mod chunks;
mod context;
mod documents;
mod stats;
mod types;

// Re-export public types
pub use types::DocumentRecord;

use crate::features::indexing::IndexStorageTrait;
use crate::infrastructure::indexing::chunker::{ContextualizedChunk, TextChunk};
use crate::infrastructure::indexing::error::Result;
use async_trait::async_trait;
use sqlx::{Sqlite, SqlitePool, Transaction};
use std::path::{Path, PathBuf};

/// Main storage interface for indexed documents.
///
/// Provides methods for storing, retrieving, and managing documents
/// with their associated text chunks and embeddings.
pub struct IndexStorage {
    pub pool: SqlitePool,
}

impl IndexStorage {
    /// Create a new storage instance.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    // Document operations

    /// Store a document with text chunks and embeddings.
    pub async fn store_document(
        &self,
        path: &Path,
        mime_type: &str,
        chunks: Vec<TextChunk>,
        embeddings: Vec<Vec<f32>>,
    ) -> Result<String> {
        chunks::store_document(&self.pool, path, mime_type, chunks, embeddings).await
    }

    /// Check if a document exists by path.
    pub async fn document_exists(&self, path: &Path) -> Result<bool> {
        documents::document_exists(&self.pool, path).await
    }

    /// Get document record by path.
    pub async fn get_document_by_path(&self, path: &Path) -> Result<Option<DocumentRecord>> {
        documents::get_document_by_path(&self.pool, path).await
    }

    /// Check if document needs reindexing.
    pub async fn needs_reindex(&self, path: &Path) -> Result<bool> {
        documents::needs_reindex(&self.pool, path).await
    }

    /// Update document status.
    pub async fn mark_document_status(&self, path: &Path, status: &str) -> Result<()> {
        documents::mark_document_status(&self.pool, path, status).await
    }

    /// Remove a document.
    pub async fn remove_document(&self, path: &Path) -> Result<()> {
        documents::remove_document(&self.pool, path).await
    }

    /// Store file metadata only.
    pub async fn store_file_metadata_only(
        &self,
        path: &Path,
        file_id: &str,
        mime_type: &str,
    ) -> Result<String> {
        documents::store_file_metadata_only(&self.pool, path, file_id, mime_type).await
    }

    // Statistics

    /// Get count of indexed documents.
    pub async fn get_indexed_count(&self) -> Result<i64> {
        stats::get_indexed_count(&self.pool).await
    }

    /// Get total number of chunks.
    pub async fn get_total_chunks(&self) -> Result<i64> {
        stats::get_total_chunks(&self.pool).await
    }

    // Batch operations

    /// Store multiple documents in batch.
    pub async fn batch_store_documents(
        &self,
        documents: chunks::ChunkedDocuments,
    ) -> Result<Vec<String>> {
        chunks::batch_store_documents(&self.pool, documents).await
    }

    // Contextualized storage

    /// Store document with contextualized chunks.
    pub async fn store_document_with_context(
        &self,
        path: &Path,
        mime_type: &str,
        chunks: Vec<ContextualizedChunk>,
        embeddings: Vec<Vec<f32>>,
    ) -> Result<String> {
        context::store_document_with_context(&self.pool, path, mime_type, chunks, embeddings).await
    }

    /// Store document with context and file ID.
    pub async fn store_document_with_context_and_file(
        &self,
        path: &Path,
        file_id: &str,
        mime_type: &str,
        chunks: Vec<ContextualizedChunk>,
        embeddings: Vec<Vec<f32>>,
    ) -> Result<String> {
        context::store_document_with_context_and_file(
            &self.pool, path, file_id, mime_type, chunks, embeddings,
        )
        .await
    }

    /// Store document with context using external transaction.
    pub async fn store_document_with_context_and_file_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        path: &Path,
        file_id: &str,
        mime_type: &str,
        chunks: Vec<ContextualizedChunk>,
        embeddings: Vec<Vec<f32>>,
    ) -> Result<String> {
        context::store_document_with_context_and_file_tx(
            tx, path, file_id, mime_type, chunks, embeddings,
        )
        .await
    }

    // Private helpers exposed for backwards compatibility

    async fn calculate_checksum(&self, path: &Path) -> Result<String> {
        checksum::calculate_checksum(path).await
    }
}

// ============================================================================
// IndexStorageTrait Implementation
// ============================================================================

#[async_trait]
impl IndexStorageTrait for IndexStorage {
    async fn store_document(
        &self,
        path: &Path,
        mime_type: &str,
        chunks: Vec<TextChunk>,
        embeddings: Vec<Vec<f32>>,
    ) -> Result<String> {
        self.store_document(path, mime_type, chunks, embeddings)
            .await
    }

    async fn document_exists(&self, path: &Path) -> Result<bool> {
        self.document_exists(path).await
    }

    async fn get_document_by_path(&self, path: &Path) -> Result<Option<DocumentRecord>> {
        self.get_document_by_path(path).await
    }

    async fn needs_reindex(&self, path: &Path) -> Result<bool> {
        self.needs_reindex(path).await
    }

    async fn mark_document_status(&self, path: &Path, status: &str) -> Result<()> {
        self.mark_document_status(path, status).await
    }

    async fn remove_document(&self, path: &Path) -> Result<()> {
        self.remove_document(path).await
    }

    async fn store_file_metadata_only(
        &self,
        path: &Path,
        file_id: &str,
        mime_type: &str,
    ) -> Result<String> {
        self.store_file_metadata_only(path, file_id, mime_type)
            .await
    }

    async fn get_indexed_count(&self) -> Result<i64> {
        self.get_indexed_count().await
    }

    async fn get_total_chunks(&self) -> Result<i64> {
        self.get_total_chunks().await
    }

    async fn batch_store_documents(
        &self,
        documents: Vec<(PathBuf, String, Vec<TextChunk>, Vec<Vec<f32>>)>,
    ) -> Result<Vec<String>> {
        self.batch_store_documents(documents).await
    }

    async fn store_document_with_context(
        &self,
        path: &Path,
        mime_type: &str,
        chunks: Vec<ContextualizedChunk>,
        embeddings: Vec<Vec<f32>>,
    ) -> Result<String> {
        self.store_document_with_context(path, mime_type, chunks, embeddings)
            .await
    }

    async fn store_document_with_context_and_file(
        &self,
        path: &Path,
        file_id: &str,
        mime_type: &str,
        chunks: Vec<ContextualizedChunk>,
        embeddings: Vec<Vec<f32>>,
    ) -> Result<String> {
        self.store_document_with_context_and_file(path, file_id, mime_type, chunks, embeddings)
            .await
    }
}
