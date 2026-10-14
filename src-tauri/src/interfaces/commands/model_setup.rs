//! Model Setup Commands
//!
//! Commands for first-run model setup and default model downloads.
//!
//! # Commands
//!
//! - `check_first_run_status_impl` - Check if first-run setup is needed
//! - `download_default_embedding_model_impl` - Download and activate default model
//! - `download_model_impl` - Download a catalog model by id
//!
//! # Gateway Pattern
//!
//! All commands are routed through the Gateway pattern which provides async dispatch.
//! Async functions store state on the heap (in Future objects), not the stack.

use crate::domain::download::DownloadOperationState;
use crate::domain::models::embedding_defaults::default_embedding_model;
use crate::features::initialization::use_cases::first_run_setup::CheckFirstRunStatusUseCase;
use crate::features::llm::dto::{DownloadModelRequestDto, DownloadModelResponseDto};
use crate::interfaces::di::Container;
use crate::shared::error::AppError;

#[derive(Debug, serde::Serialize)]
struct DownloadDefaultModelResponse {
    download_id: String,
    model_id: String,
    model_name: String,
    file_path: String,
    file_size_bytes: u64,
}

/// Check first-run status implementation.
///
/// Detects if this is the first run by checking for existing .onnx models.
/// Returns recommendation for default model if no models are found.
/// Called by gateway - async dispatch.
pub async fn check_first_run_status_impl(
    container: &Container,
) -> std::result::Result<String, String> {
    tracing::info!("Command: check_first_run_status - ENTRY");

    let repository = (*container.downloaded_model_repository()).clone();
    // Pass the system_info probe so the chat recommendation is sized to the
    // user's actual RAM, not a one-size-fits-all small-tier default.
    let use_case =
        CheckFirstRunStatusUseCase::with_system_info(repository, container.system_info());
    let response = use_case
        .execute()
        .await
        .map_err(|e| format!("First run check failed: {}", e))?;

    serde_json::to_string(&response).map_err(|e| format!("Serialization error: {}", e))
}

/// Download default embedding model implementation.
///
/// Downloads the model `default_embedding_model` picks for this machine, which
/// is the same one `check_first_run_status` offered.
/// Progress can be monitored via download events.
/// Called by gateway - async dispatch.
pub async fn download_default_embedding_model_impl(
    container: &Container,
) -> std::result::Result<String, String> {
    tracing::info!("Command: download_default_embedding_model - ENTRY");

    let default = default_embedding_model(
        crate::infrastructure::ml::compute_device::gpu_acceleration_available(),
    );
    let model_id = default.curated_id.to_string();
    let model_name = default.display_name.to_string();

    container
        .security_context()
        .rate_limiters()
        .indexing
        .check_rate_limit(&model_id)
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    let response = container
        .download_model_use_case()
        .execute(DownloadModelRequestDto {
            model_id: model_id.clone(),
        })
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    let file_size_bytes = match response.state {
        DownloadOperationState::DownloadStarted {
            total_size_bytes, ..
        }
        | DownloadOperationState::AlreadyDownloaded {
            total_size_bytes, ..
        } => total_size_bytes,
        DownloadOperationState::NetworkError { ref error_message }
        | DownloadOperationState::OperationFailed { ref error_message } => {
            return Err(format!("Download failed: {}", error_message));
        }
    };

    let payload = DownloadDefaultModelResponse {
        // Batch download snapshots are keyed by model_id.
        download_id: model_id.clone(),
        model_id: model_id.clone(),
        model_name: model_name.clone(),
        file_path: response
            .path
            .unwrap_or_else(|| container.models_path().to_string_lossy().to_string()),
        file_size_bytes,
    };

    serde_json::to_string(&payload).map_err(|e| format!("Serialization error: {}", e))
}

/// Downloads an LLM model with progress tracking and integrity verification
///
/// Initiates download of a model from remote storage to local filesystem. Provides
/// real-time progress updates via events, validates downloaded file integrity, and
/// handles network errors gracefully with retry logic. This is a resource-intensive
/// operation protected by rate limiting to prevent abuse and ensure system stability.
///
/// # Arguments
///
/// * `model_id` - Unique identifier of the model to download (e.g., "llama-3-8b")
/// * `container` - Service container with use cases and security context
///
/// # Returns
///
/// * `Ok(DownloadModelResponseDto)` - Download completion status with path
/// * `Err(AppError)` - If rate limited, download fails, or verification fails
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Too many download requests (CWE-770 mitigation)
/// * `AppError::InvalidInput` - Model ID invalid or not in catalog
/// * `AppError::Other` - Network error, storage full, or integrity check failed
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
/// import { listen } from '@tauri-apps/api/event';
///
/// interface DownloadProgress {
///   modelId: string;
///   bytesDownloaded: number;
///   totalBytes: number;
///   percentComplete: number;
///   downloadSpeed: number;      // Bytes per second
///   estimatedTimeRemaining: number; // Seconds
/// }
///
/// interface DownloadComplete {
///   modelId: string;
///   path: string;
///   sizeBytes: number;
///   downloadTimeSeconds: number;
/// }
///
/// // Listen for progress updates using the unified download:progress event
/// const unlisten = await listen<DownloadStateSnapshot>('download:progress', (event) => {
///   const snapshot = event.payload;
///   if (snapshot.kind === 'single') {
///     console.log(`Downloading ${snapshot.filename}: ${snapshot.percentage}%`);
///     console.log(`Speed: ${(snapshot.bytesPerSecond / 1024 / 1024).toFixed(2)} MB/s`);
///     console.log(`ETA: ${snapshot.etaSeconds}s`);
///   }
/// });
///
/// try {
///   // Start download
///   const result = await invoke<DownloadComplete>('download_model', {
///     modelId: 'llama-3-8b'
///   });
///
///   console.log(`Download complete: ${result.path}`);
///   console.log(`Downloaded in ${result.downloadTimeSeconds}s`);
///   console.log(`Size: ${(result.sizeBytes / 1024 / 1024 / 1024).toFixed(2)} GB`);
/// } catch (error) {
///   console.error('Download failed:', error);
/// } finally {
///   unlisten();
/// }
/// ```
///
/// # Download Process
///
/// **Workflow**:
/// 1. **Rate Limit Check**: Prevents excessive downloads (CWE-770)
/// 2. **Model Validation**: Verify model ID exists in catalog
/// 3. **Storage Check**: Ensure sufficient disk space
/// 4. **Network Download**: Stream model file with progress events
/// 5. **Integrity Verification**: Validate checksum after download
///
/// **Progress Events**:
/// - Emitted every 1 second or 10MB, whichever comes first
/// - Event name: `download:progress` (unified download event system)
/// - Payload: DownloadStateSnapshot with discriminated union (kind: 'single' | 'batch')
/// - Includes: bytes downloaded, total size, percentage, speed, ETA
///
/// **Error Handling**:
/// - **Network Failure**: Automatic retry up to 3 times with exponential backoff
/// - **Storage Full**: Clean error message, suggests freeing disk space
/// - **Integrity Failure**: Deletes corrupted file, suggests retry
/// - **Rate Limited**: Suggests waiting before retrying
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Max 5 downloads per hour to prevent DoS
/// - **Integrity Verification**: SHA-256 checksum validation
/// - **Path Validation**: Downloads only to safe, validated paths (CWE-22)
/// - **Size Limits**: Rejects downloads exceeding maximum size (100GB)
///
/// # Performance
///
/// **Download Speeds** (typical):
/// - **Small Models** (<5GB): 2-5 minutes on 100Mbps connection
/// - **Medium Models** (5-15GB): 5-15 minutes on 100Mbps connection
/// - **Large Models** (15-50GB): 15-60 minutes on 100Mbps connection
/// - **Ultra Models** (>50GB): 1+ hours on 100Mbps connection
///
/// **Resource Usage**:
/// - **Memory**: ~100-200MB for download buffer
/// - **Disk I/O**: Sequential writes, minimal system impact
/// - **Network**: Sustained download, may affect other network operations
///
/// # Architecture
///
/// Thin controller with cross-cutting concerns:
/// 1. **Rate Limiting**: Applied before execution
/// 2. **Use Case Execution**: DownloadModelUseCase handles business logic
///
/// # Command Flow
///
/// 1. Check rate limit (prevent abuse)
/// 2. Get use case from container
/// 3. Create request DTO with model ID
/// 4. Execute download (validate, download, verify)
/// 5. Return DownloadModelResponseDto with path
///
/// # Note
///
/// Downloads can be cancelled using a separate `cancel_download` command. Progress
/// is saved, allowing resumption on retry (if supported by download source).
pub async fn download_model_impl(
    model_id: String,
    container: &Container,
) -> Result<DownloadModelResponseDto, AppError> {
    tracing::info!(model_id = %model_id, "Command: download_model - ENTRY");

    // Rate limiting (resource-intensive operation)
    container
        .security_context()
        .rate_limiters()
        .indexing
        .check_rate_limit(&model_id)
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    let use_case = container.download_model_use_case();
    let request = DownloadModelRequestDto {
        model_id: model_id.clone(),
    };
    let response = use_case.execute(request).await.map_err(|e| {
        tracing::error!(model_id = %model_id, error = %e, "Command: download_model - ERROR");
        e
    })?;

    tracing::info!(model_id = %model_id, "Command: download_model - EXIT");
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interfaces::di::Container;
    use tempfile::TempDir;

    /// Helper to create a test container with temp database
    async fn create_test_container() -> (Container, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        std::fs::File::create(&db_path).unwrap();

        let db_url = format!("sqlite:{}", db_path.display());

        let pool = sqlx::SqlitePool::connect(&db_url).await.unwrap();

        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        let db_conn = std::sync::Arc::new(
            crate::infrastructure::persistence::database::DatabaseConnection::new(db_path.clone())
                .await
                .unwrap(),
        );

        let container = Container::new(
            pool,
            db_conn,
            None, // No embedding model for tests
            "http://localhost:11434",
            "llama3.1:8b",
            temp_dir.path().to_path_buf(),
            std::sync::Arc::default(),
        )
        .await
        .unwrap();

        (container, temp_dir)
    }

    #[tokio::test]
    #[ignore = "Requires writable filesystem, run manually with --ignored"]
    async fn test_check_first_run_status_command() {
        let (container, _temp_dir) = create_test_container().await;

        // Resolve the repository from the container (proper DI) and run the use case.
        let repository = (*container.downloaded_model_repository()).clone();
        let use_case = CheckFirstRunStatusUseCase::new(repository);
        let response = use_case.execute().await.unwrap();

        assert!(response.needs_setup);
    }

    #[tokio::test]
    #[ignore = "Requires writable filesystem, run manually with --ignored"]
    async fn test_models_path_from_container() {
        let (container, _temp_dir) = create_test_container().await;

        let models_path = container.models_path();

        assert!(models_path.to_string_lossy().contains("models"));

        // Verify fallback works (should use home dir)
        assert!(models_path.exists() || models_path.to_string_lossy().contains(".lattice"));
    }
}
