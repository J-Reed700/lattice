//! # Delete Document Use Case
//!
//! Deletes a document and all its associated data from the system.
//!
//! This use case orchestrates:
//! 1. Document existence verification
//! 2. Removal of vector embeddings from search index
//! 3. Deletion of text chunks from repository
//! 4. Deletion of document record from repository
//!
//! ## Example
//!
//! ```rust,no_run
//! use vault_desktop::application::use_cases::indexing::delete_document::DeleteDocumentUseCase;
//!
//! # async fn example(use_case: DeleteDocumentUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let response = use_case.execute("doc-123".to_string()).await?;
//! println!("Document deleted: {}", response.message);
//! # Ok(())
//! # }
//! ```

use std::path::Path;
use std::sync::Arc;

use crate::application::ports::{
    ChunkRepositoryPort, DocumentRepositoryPort, FileStoragePort, VectorSearchPort,
};
use crate::domain::repositories::UnitOfWorkFactory;
use crate::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};

/// Request to delete a document.
///
/// Contains the document ID to be deleted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteDocumentRequestDto {
    /// ID of the document to delete
    pub document_id: String,
}

/// Response from deleting a document.
///
/// Contains the deletion status and confirmation message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteDocumentResponseDto {
    /// Deletion status (e.g., "deleted", "not_found")
    pub status: String,

    /// Confirmation or error message
    pub message: String,
}

/// Delete document use case.
///
/// Coordinates deletion of a document and all its associated data, including:
/// - Vector embeddings from search index
/// - Text chunks from repository
/// - Document record from repository
///
/// ## Dependencies
///
/// - `DocumentRepositoryPort`: Manages document records
/// - `ChunkRepositoryPort`: Manages text chunks
/// - `VectorSearchPort`: Manages vector embeddings
///
/// ## Business Rules
///
/// - Document must exist to be deleted
/// - All chunks are deleted in cascade
/// - Vector embeddings are removed for all chunks
/// - Database operations are atomic; vector index removal happens after commit
pub struct DeleteDocumentUseCase {
    document_repo: Arc<dyn DocumentRepositoryPort>,
    chunk_repo: Arc<dyn ChunkRepositoryPort>,
    vector_search: Arc<dyn VectorSearchPort>,
    uow_factory: Arc<dyn UnitOfWorkFactory>,
    file_storage: Arc<dyn FileStoragePort>,
}

impl DeleteDocumentUseCase {
    /// Create a new delete document use case.
    ///
    /// # Arguments
    ///
    /// * `document_repo` - Repository for document persistence
    /// * `chunk_repo` - Repository for chunk persistence
    /// * `vector_search` - Service for managing vector embeddings
    /// * `uow_factory` - Unit of work factory for transactional operations
    /// * `file_storage` - File storage for deleting physical files
    pub fn new(
        document_repo: Arc<dyn DocumentRepositoryPort>,
        chunk_repo: Arc<dyn ChunkRepositoryPort>,
        vector_search: Arc<dyn VectorSearchPort>,
        uow_factory: Arc<dyn UnitOfWorkFactory>,
        file_storage: Arc<dyn FileStoragePort>,
    ) -> Self {
        Self {
            document_repo,
            chunk_repo,
            vector_search,
            uow_factory,
            file_storage,
        }
    }

    /// Execute document deletion.
    ///
    /// # Arguments
    ///
    /// * `document_id` - ID of document to delete
    ///
    /// # Returns
    ///
    /// Response with deletion status and confirmation message
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Document not found (AppError::NotFound)
    /// - Chunk retrieval fails (AppError::Database)
    /// - Vector embedding removal fails (AppError::SearchFailed)
    /// - Chunk deletion fails (AppError::Database)
    /// - Document deletion fails (AppError::Database)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use vault_desktop::application::use_cases::indexing::delete_document::DeleteDocumentUseCase;
    /// # async fn example(use_case: DeleteDocumentUseCase) -> Result<(), Box<dyn std::error::Error>> {
    /// // Delete a document
    /// match use_case.execute("doc-123".to_string()).await {
    ///     Ok(response) => println!("Success: {}", response.message),
    ///     Err(e) => eprintln!("Failed: {}", e),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute(&self, document_id: String) -> Result<DeleteDocumentResponseDto> {
        // 0. Fetch the document's file path BEFORE deletion so we can clean up the physical file
        let file_path_str = self
            .document_repo
            .find_file_path_by_id(&document_id)
            .await
            .ok();

        let mut uow = self.uow_factory.create().await?;
        let db_result = {
            let chunk_repo = uow.chunk_repository()?;
            let document_repo = uow.document_repository()?;

            // 1. Verify document exists (transaction-scoped)
            let exists = document_repo.document_exists(&document_id).await?;
            if !exists {
                return Err(AppError::NotFound(format!(
                    "Document not found: {}",
                    document_id
                )));
            }

            // 2. Get all chunks for the document to track how many we're deleting
            let chunks = chunk_repo.find_by_document(&document_id).await?;

            // 3. Delete chunks and document within the transaction
            chunk_repo.delete_by_document(&document_id).await?;
            DocumentRepositoryPort::delete(&*document_repo, &document_id).await?;
            Ok::<_, AppError>(chunks)
        };

        let chunks = match db_result {
            Ok(chunks) => {
                uow.commit().await?;
                chunks
            }
            Err(err) => {
                if let Err(rollback_err) = uow.rollback().await {
                    return Err(AppError::Database(format!(
                        "Delete failed: {}; rollback failed: {}",
                        err, rollback_err
                    )));
                }
                return Err(err);
            }
        };

        let chunks_count = chunks.len();

        // 4. Remove vector embeddings AFTER DB commit
        // If this fails, surface error to user (DB state is already committed)
        // Key format matches EmbeddingRepository::save/save_batch: "emb_{chunk_id}"
        for chunk in &chunks {
            let embedding_key = format!("emb_{}", chunk.id());
            if let Err(e) = self.vector_search.remove_embedding(&embedding_key) {
                tracing::error!(
                    document_id = %document_id,
                    chunk_id = %chunk.id(),
                    error = %e,
                    "Failed to remove vector embedding after delete commit"
                );
                return Err(AppError::Other(format!(
                    "Failed to remove vector embedding for chunk {}: {}",
                    chunk.id(),
                    e
                )));
            }
        }

        // 5. Delete the physical file from content-addressed storage AFTER DB commit
        if let Some(path_str) = file_path_str {
            let file_path = Path::new(&path_str);
            if self.file_storage.exists(file_path).await {
                if let Err(e) = self.file_storage.delete_file(file_path).await {
                    tracing::warn!(
                        document_id = %document_id,
                        file_path = %path_str,
                        error = %e,
                        "Failed to delete physical file after document deletion (non-fatal)"
                    );
                } else {
                    tracing::info!(
                        document_id = %document_id,
                        file_path = %path_str,
                        "Deleted physical file from library"
                    );
                    // Also try to remove the parent directory if it's now empty
                    // (content-addressed storage uses {hash}/ directories)
                    if let Some(parent) = file_path.parent() {
                        if let Ok(mut entries) = tokio::fs::read_dir(parent).await {
                            let mut is_empty = true;
                            if entries.next_entry().await.ok().flatten().is_some() {
                                is_empty = false;
                            }
                            if is_empty {
                                let _ = tokio::fs::remove_dir(parent).await;
                            }
                        }
                    }
                }
            }
        }

        // 6. Build success response
        Ok(DeleteDocumentResponseDto {
            status: "deleted".to_string(),
            message: format!(
                "Document {} and {} associated chunks deleted successfully",
                document_id, chunks_count
            ),
        })
    }
}

// ============================================================================
// Tests
// ============================================================================
