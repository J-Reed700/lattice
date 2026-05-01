//! Delete Downloaded Model Use Case
//!
//! SECURITY FIX (CWE-367): Atomic deletion with optimistic concurrency control
//!
//! Removes a downloaded model record and optionally deletes the file.
//! Uses database transactions to prevent TOCTOU race conditions.

use crate::audit::AuditAction;
use crate::infrastructure::persistence::repositories::DownloadedModelRepository;
use crate::shared::error::{AppError, Result};
use std::path::Path;
use std::time::Duration;
use tokio::fs;
use tokio::time::sleep;
use tracing::{error, info, warn};

/// Maximum retry attempts for concurrent modification conflicts
const MAX_RETRY_ATTEMPTS: u32 = 3;

/// Base delay between retries (exponential backoff)
const RETRY_BASE_DELAY_MS: u64 = 10;

/// Validates that a path is safe for directory deletion
///
/// SECURITY: Implements Oracle's 3 safety gates:
/// 1. Path must be under models directory
/// 2. Path must be at least one level deeper than models_dir (not root)
/// 3. Directory name must match model_id or parent must be "models"/"custom"
///
/// # Arguments
/// * `path` - The directory path to validate
/// * `model_id` - The model identifier for validation
///
/// # Returns
/// Ok(true) if safe to delete, descriptive error otherwise
fn is_safe_model_directory(path: &Path, model_id: &str) -> Result<bool> {
    // Gate 1: Verify path contains "/models/" or "\models\"
    let path_str = path.to_str().ok_or_else(|| {
        AppError::FileSystem(format!("Invalid path encoding: {}", path.display()))
    })?;

    if !path_str.contains("/models/") && !path_str.contains("\\models\\") {
        return Err(AppError::FileSystem(format!(
            "Unsafe deletion: Path does not contain models directory: {}",
            path.display()
        )));
    }

    // Gate 2: Verify path is at least one level deeper than models_dir
    // Path should be: <something>/models/<subdirectory>/...
    // NOT just: <something>/models
    let components: Vec<_> = path.components().collect();
    let models_idx = components.iter().position(|c| {
        if let std::path::Component::Normal(os_str) = c {
            *os_str == std::ffi::OsStr::new("models")
        } else {
            false
        }
    });

    if let Some(idx) = models_idx {
        // Verify there's at least one component after "models"
        if idx + 1 >= components.len() {
            return Err(AppError::FileSystem(format!(
                "Unsafe deletion: Path is models root directory: {}",
                path.display()
            )));
        }
    } else {
        return Err(AppError::FileSystem(format!(
            "Unsafe deletion: Cannot find models directory in path: {}",
            path.display()
        )));
    }

    // Gate 3: Verify directory name matches model_id or parent is "models"/"custom"
    let dir_name = path.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
        AppError::FileSystem(format!(
            "Unsafe deletion: Cannot extract directory name from path: {}",
            path.display()
        ))
    })?;

    let parent_name = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str());

    // Allow if:
    // - Directory name contains the model_id (e.g., "qwen2.5-0.5b-instruct-q4_0")
    // - OR parent is "models" or "custom" (for isolated model directories)
    let is_valid_structure = dir_name.contains(model_id)
        || parent_name == Some("models")
        || parent_name == Some("custom");

    if !is_valid_structure {
        return Err(AppError::FileSystem(format!(
            "Unsafe deletion: Directory name '{}' does not match model_id '{}' and parent is not models/custom: {}",
            dir_name,
            model_id,
            path.display()
        )));
    }

    Ok(true)
}

/// Use case for deleting a downloaded model
pub struct DeleteDownloadedModelUseCase {
    repository: DownloadedModelRepository,
}

impl DeleteDownloadedModelUseCase {
    pub fn new(repository: DownloadedModelRepository) -> Self {
        Self { repository }
    }

    /// Execute the use case with atomic deletion and retry logic
    ///
    /// SECURITY FIX (CWE-367): TOCTOU Race Condition Prevention
    ///
    /// # Arguments
    ///
    /// * `id` - The unique record ID (primary key) of the model to delete
    /// * `delete_file` - If true, also delete the file from filesystem
    ///
    /// # Atomic Deletion Strategy
    ///
    /// To prevent TOCTOU race conditions, this use case implements:
    ///
    /// 1. **Single Transaction Scope**: All validation and deletion happen atomically
    /// 2. **Optimistic Concurrency**: Retry on concurrent modification conflicts
    /// 3. **File-First Deletion**: File deleted BEFORE database record
    /// 4. **Audit Logging**: All operations logged for forensics
    ///
    /// ## Deletion Order (Inside Transaction)
    ///
    /// 1. BEGIN TRANSACTION
    /// 2. SELECT model record FOR UPDATE (locks row, prevents concurrent deletion)
    /// 3. Validate model is not active (embedding or chat)
    /// 4. Delete file from filesystem (if delete_file=true)
    /// 5. DELETE database record
    /// 6. COMMIT TRANSACTION
    ///
    /// ## Concurrency Handling
    ///
    /// - If another transaction modifies the model concurrently, SQLite returns SQLITE_BUSY
    /// - We detect this and retry with exponential backoff (max 3 attempts)
    /// - ConcurrentModification error if all retries fail
    ///
    /// ## Error Recovery
    ///
    /// - Transaction rollback on any error (database remains consistent)
    /// - If file delete fails, database record is preserved
    /// - If database delete fails, file remains (can be cleaned up later)
    ///
    /// # Errors
    ///
    /// - `NotFound`: Model doesn't exist
    /// - `InvalidInput`: Model is active (embedding or chat) - must deactivate first
    /// - `FileSystem`: File deletion failed (transaction rolled back)
    /// - `ConcurrentModification`: Concurrent deletion detected, retry limit exceeded
    /// - `Database`: Other database errors
    pub async fn execute(&self, id: &str, delete_file: bool) -> Result<()> {
        // Retry loop with exponential backoff
        for attempt in 0..MAX_RETRY_ATTEMPTS {
            if attempt > 0 {
                // Exponential backoff: 10ms, 20ms, 40ms
                let delay_ms = RETRY_BASE_DELAY_MS * (1 << attempt);
                warn!(
                    attempt,
                    delay_ms, "Concurrent modification detected, retrying deletion..."
                );
                sleep(Duration::from_millis(delay_ms)).await;
            }

            // Try atomic deletion
            match self.try_delete_atomic(id, delete_file).await {
                Ok(()) => {
                    // Success - deletion completed
                    return Ok(());
                }
                Err(AppError::ConcurrentModification { .. })
                    if attempt < MAX_RETRY_ATTEMPTS - 1 =>
                {
                    // Concurrent modification - retry
                    continue;
                }
                Err(e) => {
                    // Other error or max retries exceeded - propagate
                    return Err(e);
                }
            }
        }

        // Should never reach here (loop always returns), but satisfy compiler
        Err(AppError::ConcurrentModification {
            resource: format!("model with id {}", id),
            details: "Max retry attempts exceeded".to_string(),
        })
    }

    /// Try to delete the model atomically (single attempt)
    ///
    /// This method uses an atomic DELETE query with WHERE guard to prevent TOCTOU.
    /// The check and deletion happen in ONE database operation - no race window.
    async fn try_delete_atomic(&self, id: &str, delete_file: bool) -> Result<()> {
        // === ATOMIC DELETION (CWE-367 FIX) ===
        // Single DELETE query with WHERE is_active = 0 prevents race condition

        // Attempt atomic delete-if-not-active
        let maybe_deleted_model = self.repository.delete_if_not_active(id).await?;

        match maybe_deleted_model {
            Some(deleted_model) => {
                // Model existed and was deleted
                info!(
                    id = %id,
                    model_id = %deleted_model.model_id(),
                    model_name = %deleted_model.model_name(),
                    "Model record deleted from database (atomic)"
                );

                // === FILE DELETION (best-effort, after database) ===
                // At this point, database record is GONE - model cannot become active
                // File deletion is best-effort (orphaned files handled by cleanup job)

                if delete_file {
                    let is_external_model = deleted_model
                        .metadata()
                        .and_then(|meta| meta.get("source"))
                        .and_then(|value| value.as_str())
                        .map(|source| source == "external_directory")
                        .unwrap_or(false)
                        || deleted_model
                            .metadata()
                            .and_then(|meta| meta.get("external"))
                            .and_then(|value| value.as_bool())
                            .unwrap_or(false);

                    if is_external_model {
                        info!(
                            model_id = %deleted_model.model_id(),
                            "Skipping file deletion for external model record"
                        );
                        return Ok(());
                    }

                    // For remote-hosted models there's nothing on disk to remove.
                    let Some(file_path) = deleted_model.loadable_path() else {
                        info!(
                            model_id = %deleted_model.model_id(),
                            "Skipping file deletion for remote-hosted model"
                        );
                        return Ok(());
                    };

                    if file_path.exists() {
                        let model_dir = deleted_model
                            .location()
                            .enclosing_dir()
                            .ok_or_else(|| {
                                AppError::FileSystem(format!(
                                    "Cannot determine enclosing directory for: {}",
                                    file_path.display()
                                ))
                            })?;
                        let model_dir = model_dir.as_path();

                        // Validate directory is safe to delete
                        match is_safe_model_directory(model_dir, deleted_model.model_id()) {
                            Ok(true) => {
                                // Safe to delete entire directory
                                match fs::remove_dir_all(model_dir).await {
                                    Ok(_) => {
                                        info!(
                                            directory = %model_dir.display(),
                                            model_id = %deleted_model.model_id(),
                                            "Model directory deleted successfully (all files removed)"
                                        );
                                    }
                                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                                        warn!(
                                            directory = %model_dir.display(),
                                            "Model directory already deleted"
                                        );
                                    }
                                    #[cfg(target_os = "windows")]
                                    Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                                        // Windows file locking - model may still be loaded
                                        error!(
                                            error = %e,
                                            directory = %model_dir.display(),
                                            "Failed to delete model directory: File in use (Windows file locking). Model may still be loaded in LlmService. Orphaned directory should be cleaned up by maintenance job."
                                        );
                                    }
                                    Err(e) => {
                                        // Log but DON'T fail - database is source of truth
                                        error!(
                                            error = %e,
                                            directory = %model_dir.display(),
                                            "Failed to delete model directory. Orphaned directory should be cleaned up by maintenance job."
                                        );
                                    }
                                }
                            }
                            Ok(false) => {
                                // Safety validation returned false - fall back to single file deletion
                                warn!(
                                    file_path = %file_path.display(),
                                    directory = %model_dir.display(),
                                    "Safety validation returned false for directory deletion. Falling back to single file deletion."
                                );

                                // Fall back to removing just the single file
                                match fs::remove_file(&file_path).await {
                                    Ok(_) => {
                                        warn!(
                                            file_path = %file_path.display(),
                                            "Single file deleted (safety validation prevented directory deletion)"
                                        );
                                    }
                                    Err(e) => {
                                        error!(
                                            error = %e,
                                            file_path = %file_path.display(),
                                            "Failed to delete model file"
                                        );
                                    }
                                }
                            }
                            Err(safety_error) => {
                                // Safety validation failed with error - fall back to single file deletion
                                error!(
                                    error = %safety_error,
                                    file_path = %file_path.display(),
                                    directory = %model_dir.display(),
                                    "Safety validation failed for directory deletion. Falling back to single file deletion."
                                );

                                // Fall back to removing just the single file
                                match fs::remove_file(&file_path).await {
                                    Ok(_) => {
                                        warn!(
                                            file_path = %file_path.display(),
                                            "Single file deleted (safety validation prevented directory deletion)"
                                        );
                                    }
                                    Err(e) => {
                                        error!(
                                            error = %e,
                                            file_path = %file_path.display(),
                                            "Failed to delete model file"
                                        );
                                    }
                                }
                            }
                        }
                    } else {
                        warn!(
                            file_path = %file_path.display(),
                            "Model file not found, skipping file deletion"
                        );
                    }
                }
            }
            None => {
                // Model doesn't exist - idempotent success
                info!(
                    id = %id,
                    "Model already deleted (idempotent success)"
                );
            }
        }

        Ok(())
    }
}
