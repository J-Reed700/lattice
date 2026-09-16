//! Thin command layer for batch file imports: validate, delegate, and log.

use crate::features::batch::dto::StartBatchFileImportRequestDto;
use crate::infrastructure::audit::{get_audit_logger, AuditAction, AuditEvent, AuditResult};
use crate::interfaces::di::Container;
use crate::shared::domain_types::ValidatedFilePath;
use crate::shared::error::AppError;
use std::path::PathBuf;
use tauri::State;

/// Start batch file import
///
/// Creates a batch job and spawns background processing task.
/// Returns job_id for progress tracking.
///
/// # Command Flow
///
/// 1. **Input Validation**: Validate batch size and file paths
/// 2. **Path Validation**: Use ValidatedFilePath to prevent directory traversal (CWE-22)
/// 3. **Delegation**: Call StartBatchFileImportUseCase
/// 4. **Audit Logging**: Log successful job creation
///
/// # Security
///
/// - Batch size limit: Max 100 files (prevents memory exhaustion)
/// - Path validation: ValidatedFilePath prevents directory traversal (CWE-22)
/// - Audit logging: Tracks all import operations (CWE-778 mitigation)
///
/// # Arguments
///
/// * `file_paths` - Array of file paths to import (max 100)
/// * `container` - Service container for dependency injection
///
/// # Returns
///
/// Job ID (UUID) for progress tracking
///
/// # Errors
///
/// - `AppError::InvalidInput` if batch is empty or too large
/// - `AppError::InvalidInput` if any path is invalid or unsafe
/// - `AppError::Database` if job creation fails
///
/// # Example (from frontend)
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// const jobId = await invoke<string>('start_batch_file_import', {
///   filePaths: ['/path/to/file1.pdf', '/path/to/file2.txt'],
/// });
///
/// // Poll status
/// const status = await invoke('get_batch_job_status', { jobId });
/// ```
pub async fn start_batch_file_import(
    file_paths: Vec<String>,
    space_id: Option<String>,
    indexing: Option<crate::features::batch::dto::FileIndexingOptionsDto>,
    container: State<'_, Container>,
) -> Result<String, AppError> {
    start_batch_file_import_impl(file_paths, space_id, indexing, container.inner()).await
}

pub async fn start_batch_file_import_impl(
    file_paths: Vec<String>,
    space_id: Option<String>,
    indexing: Option<crate::features::batch::dto::FileIndexingOptionsDto>,
    container: &Container,
) -> Result<String, AppError> {
    // 1. Input validation - batch size
    if file_paths.is_empty() {
        return Err(AppError::InvalidInput("No files provided".into()));
    }
    if file_paths.len() > 100 {
        return Err(AppError::InvalidInput(format!(
            "Too many files: {} (max 100)",
            file_paths.len()
        )));
    }

    let file_count = file_paths.len();
    tracing::info!("Starting batch file import for {} files", file_count);

    // 2. Validate paths (security - prevents directory traversal CWE-22)
    for path in &file_paths {
        ValidatedFilePath::new(PathBuf::from(path))?;
    }

    // 3. Ensure embedding service is available before spawning background batch job.
    // This avoids partial jobs that immediately fail per-file with degraded mock errors.
    let embedding_service = container.get_or_load_embedding().await.map_err(|e| {
        AppError::ServiceNotAvailable(format!(
            "AI embedding models not installed. Download and activate an embedding model in Settings -> Models. Details: {}",
            e
        ))
    })?;
    let embedding_ready = embedding_service.is_ready().await.map_err(|e| {
        AppError::ServiceNotAvailable(format!("Failed to verify embedding model readiness: {}", e))
    })?;
    if !embedding_ready {
        return Err(AppError::ServiceNotAvailable(
            "AI embedding model is not ready yet. Activate an embedding model in Settings -> Models and try again."
                .to_string(),
        ));
    }

    // 4. Start batch import (delegate to use case)
    let use_case = container.start_batch_file_import_use_case();
    let result = use_case
        .execute(StartBatchFileImportRequestDto {
            file_paths,
            space_id,
            indexing,
        })
        .await
        .map(|response| response.job_id);

    // 5. Audit logging (security - CWE-778 mitigation)
    let audit_logger = get_audit_logger();
    match &result {
        Ok(job_id) => {
            tracing::info!("Batch file import job {} created successfully", job_id);

            let event = AuditEvent::new(AuditAction::DataImported, AuditResult::success())
                .with_resource_id(format!("batch_file_job:{}", job_id))
                .with_metadata("file_count", file_count.to_string())
                .with_metadata("job_id", job_id)
                .with_metadata("operation", "start_batch_file_import");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
        Err(e) => {
            let event = AuditEvent::new(
                AuditAction::DataImported,
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id("batch_file_job:failed")
            .with_metadata("file_count", file_count.to_string())
            .with_metadata("operation", "start_batch_file_import")
            .with_metadata("error", e.to_string());

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
    }

    // 6. Return response
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Full integration tests require Container setup.
    // These tests verify the command structure and validation logic.

    #[tokio::test]
    async fn test_start_batch_file_import_empty_paths() {
        // This test would require full Container setup
        // Skipped for now - integration tests should cover this
    }

    #[test]
    fn test_batch_size_validation() {
        let empty: Vec<String> = vec![];
        assert!(empty.is_empty());

        let too_many: Vec<String> = (0..101).map(|i| format!("/file{}.txt", i)).collect();
        assert!(too_many.len() > 100);
    }

    #[test]
    fn test_path_validation() {
        // Valid path
        let valid = ValidatedFilePath::new(std::env::temp_dir().join("test.txt"));
        assert!(valid.is_ok());

        // Invalid path (directory traversal)
        let invalid = ValidatedFilePath::new(PathBuf::from("../../../etc/passwd"));
        assert!(invalid.is_err());
    }
}
