use std::sync::Arc;

use lattice_core::application::ports::DocumentRepositoryPort;
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};

/// Request to rename a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameDocumentRequestDto {
    /// ID of the document to rename.
    pub document_id: String,
    /// New name for the document.
    pub new_name: String,
}

/// Response from renaming a document.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct RenameDocumentResponseDto {
    /// Rename status.
    pub status: String,
    /// Confirmation message.
    pub message: String,
    /// The new name applied.
    pub new_name: String,
}

/// Updates the display name (`file_name` field) of a document in the database metadata.
/// This does not rename the actual file on disk.
pub struct RenameDocumentUseCase {
    document_repo: Arc<dyn DocumentRepositoryPort>,
}

impl RenameDocumentUseCase {
    pub fn new(document_repo: Arc<dyn DocumentRepositoryPort>) -> Self {
        Self { document_repo }
    }

    pub async fn execute(
        &self,
        document_id: String,
        new_name: String,
    ) -> Result<RenameDocumentResponseDto> {
        let new_name = self.validate_name(&new_name)?;

        let document = self
            .document_repo
            .find_by_id(&document_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Document not found: {}", document_id)))?;

        let old_name = document.file_name().to_string();

        // Apply the rename as a metadata-only update.
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

        Ok(RenameDocumentResponseDto {
            status: "renamed".to_string(),
            message: format!("Document renamed from '{}' to '{}'", old_name, new_name),
            new_name,
        })
    }

    fn validate_name(&self, name: &str) -> Result<String> {
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
