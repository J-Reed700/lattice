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
//!
//! ## Example
//!
//! ```rust,no_run
//! use vault_desktop::application::use_cases::indexing::rename_document::RenameDocumentUseCase;
//!
//! # async fn example(use_case: RenameDocumentUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let response = use_case.execute("doc-123".to_string(), "New Name.txt".to_string()).await?;
//! println!("Document renamed: {}", response.message);
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use crate::application::ports::DocumentRepositoryPort;
use crate::shared::error::{AppError, Result};
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
    ///
    /// # Arguments
    ///
    /// * `document_repo` - Repository for document persistence
    pub fn new(document_repo: Arc<dyn DocumentRepositoryPort>) -> Self {
        Self { document_repo }
    }

    /// Execute document rename.
    ///
    /// # Arguments
    ///
    /// * `document_id` - ID of document to rename
    /// * `new_name` - New display name for the document
    ///
    /// # Returns
    ///
    /// Response with rename status and confirmation message
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Document not found (AppError::NotFound)
    /// - New name is invalid (AppError::InvalidInput)
    /// - Document update fails (AppError::Database)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use vault_desktop::application::use_cases::indexing::rename_document::RenameDocumentUseCase;
    /// # async fn example(use_case: RenameDocumentUseCase) -> Result<(), Box<dyn std::error::Error>> {
    /// // Rename a document
    /// match use_case.execute("doc-123".to_string(), "Updated Name.txt".to_string()).await {
    ///     Ok(response) => println!("Success: {}", response.message),
    ///     Err(e) => eprintln!("Failed: {}", e),
    /// }
    /// # Ok(())
    /// # }
    /// ```
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

        // 4. Update file_name field by reconstructing the document
        // Clone/copy values from the document to reconstruct with new name
        let updated_document = crate::domain::entities::Document::with_id(
            document.id().clone(),
            document.validated_file_path().clone(),
            new_name.clone(),
            document.file_type().map(|s| s.to_string()),
            document.mime_type().to_string(),
            document.size_bytes(),
            *document.modified_at(),
            *document.indexed_at(),
            document.checksum().clone(),
            document.status(), // DocumentStatus is Copy
            document.error_message().map(|s| s.to_string()),
            document.language().clone(),
            document.category().clone(),
            document.quality_score(),
            document.access_count(),
            document.last_accessed_at().copied(),
            document.word_count(),
            document.content().to_string(),
        );

        // 5. Save updated document
        self.document_repo.save(&updated_document).await?;

        // 6. Build success response
        Ok(RenameDocumentResponseDto {
            status: "renamed".to_string(),
            message: format!("Document renamed from '{}' to '{}'", old_name, new_name),
            new_name,
        })
    }

    /// Validate the new document name.
    ///
    /// # Arguments
    ///
    /// * `name` - The proposed new name
    ///
    /// # Returns
    ///
    /// Trimmed, validated name
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

        // Check not empty
        if trimmed.is_empty() {
            return Err(AppError::InvalidInput(
                "Document name cannot be empty".to_string(),
            ));
        }

        // Check length
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

        // Check for null bytes and control characters
        if trimmed.contains('\0') || trimmed.chars().any(|c| c.is_control()) {
            return Err(AppError::InvalidInput(
                "Document name cannot contain control characters".to_string(),
            ));
        }

        Ok(trimmed.to_string())
    }
}

// ============================================================================
// Tests
// ============================================================================
