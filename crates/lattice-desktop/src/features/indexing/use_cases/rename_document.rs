//! # Rename Document Use Case
//!
//! Renames a document by updating its file_name field in the database.
//!
//! This use case orchestrates:
//! 1. Input validation (new_name is not empty, no invalid characters)
//! 2. Document existence verification
//! 3. File name update in document entity
//! 4. Persistence of updated document
//!
//! ## Note
//!
//! This operation only updates the database record. It does NOT rename
//! the actual file on disk. The file_name field is metadata for display
//! purposes, while file_path remains the source of truth for the actual file.

use std::sync::Arc;

use lattice_core::application::ports::DocumentRepositoryPort;
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};

/// Request to rename a document.
///
/// Contains the document ID and the new name to apply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameDocumentRequestDto {
    /// ID of the document to rename
    pub document_id: String,

    /// New name for the document
    pub new_name: String,
}

/// Response from renaming a document.
///
/// Contains the rename status and confirmation message.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct RenameDocumentResponseDto {
    /// Rename status (e.g., "renamed", "not_found")
    pub status: String,

    /// Confirmation or error message
    pub message: String,

    /// The new name that was applied
    pub new_name: String,
}

/// Rename document use case.
///
/// Updates the display name (file_name field) of a document in the database.
/// This does NOT rename the actual file on disk - only the metadata.
///
/// ## Dependencies
///
/// - `DocumentRepositoryPort`: Manages document records
///
/// ## Business Rules
///
/// - Document must exist to be renamed
/// - New name cannot be empty or whitespace-only
/// - New name cannot contain path separator characters (/, \)
/// - New name should be reasonable length (1-255 characters)
/// - Operation is atomic - either succeeds or fails completely
///
/// ## Validation Rules
///
/// The new name is validated to ensure:
/// - Not empty after trimming whitespace
/// - Does not contain path separators (prevents path traversal)
/// - Length is between 1 and 255 characters
/// - Does not contain null bytes or other control characters
pub struct RenameDocumentUseCase {
    document_repo: Arc<dyn DocumentRepositoryPort>,
}

impl RenameDocumentUseCase {
    /// Create a new rename document use case.
    pub fn new(document_repo: Arc<dyn DocumentRepositoryPort>) -> Self {
        Self { document_repo }
    }

    /// Execute document rename.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Document not found (AppError::NotFound)
    /// - New name is invalid (AppError::InvalidInput)
    /// - Document update fails (AppError::Database)
    pub async fn execute(
        &self,
        document_id: String,
        new_name: String,
    ) -> Result<RenameDocumentResponseDto> {
        // 1. Validate new_name
        let new_name = self.validate_name(&new_name)?;

        // 2. Find document by ID
        let document = self
            .document_repo
            .find_by_id(&document_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Document not found: {}", document_id)))?;

        // 3. Store old name for response message
        let old_name = document.file_name().to_string();

        // 4. Apply the rename as a metadata-only update.
        // This used to rebuild the aggregate with `Document::with_id` and call
        // `save()`. `with_id` produces a document with `chunks: Vec::new()`,
        // and the aggregate save deletes every existing chunk before inserting
        // that empty set — so renaming a file destroyed its chunks, cascaded
        // its embeddings away, and dropped the document out of search. A
        // rename touches one column; it must not travel through a path that
        // rewrites children.
        //
        // The chunks' stored context prefix (`embedding_input::context_prefix`)
        // names the file, and their dense vectors and FTS rows were built from
        // it, so both keep the old name until the document is next reindexed.
        // The re-embed backfill cannot pick this up: it keys on
        // `contextualized_content`, vectors in `text_embeddings` are restored
        // without a hash check, and it only runs at startup when the index is
        // short of chunks. Refreshing the prefix here would mean rewriting every
        // chunk row, dropping its vectors from the live index and re-embedding —
        // a reindex without the extraction, which this use case has no ports for.
        self.document_repo.rename(&document_id, &new_name).await?;

        // 6. Build success response
        Ok(RenameDocumentResponseDto {
            status: "renamed".to_string(),
            message: format!("Document renamed from '{}' to '{}'", old_name, new_name),
            new_name,
        })
    }

    /// Validate the new document name.
    ///
    /// # Errors
    ///
    /// Returns `AppError::InvalidInput` if:
    /// - Name is empty or whitespace-only
    /// - Name contains path separators (/, \)
    /// - Name exceeds 255 characters
    /// - Name contains null bytes or control characters
    fn validate_name(&self, name: &str) -> Result<String> {
        // Trim whitespace
        let trimmed = name.trim();

        if trimmed.is_empty() {
            return Err(AppError::InvalidInput(
                "Document name cannot be empty".to_string(),
            ));
        }

        if trimmed.len() > 255 {
            return Err(AppError::InvalidInput(
                "Document name cannot exceed 255 characters".to_string(),
            ));
        }

        // Check for path separators (security: prevent path traversal)
        if trimmed.contains('/') || trimmed.contains('\\') {
            return Err(AppError::InvalidInput(
                "Document name cannot contain path separators (/ or \\)".to_string(),
            ));
        }

        if trimmed.contains('\0') || trimmed.chars().any(|c| c.is_control()) {
            return Err(AppError::InvalidInput(
                "Document name cannot contain control characters".to_string(),
            ));
        }

        Ok(trimmed.to_string())
    }
}
