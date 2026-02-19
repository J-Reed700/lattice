//! Model Setup Commands
//!
//! Commands for first-run model setup and default model downloads.
//!
//! # Commands
//!
//! - `check_first_run_status_impl` - Check if first-run setup is needed
//! - `download_default_embedding_model_impl` - Download and activate default model
//!
//! # Gateway Pattern
//!
//! All commands are routed through the Gateway pattern which provides async dispatch.
//! Async functions store state on the heap (in Future objects), not the stack.

use crate::application::dtos::llm_dto::DownloadModelRequestDto;
use crate::application::use_cases::initialization::first_run_setup::CheckFirstRunStatusUseCase;
use crate::audit_success;
use crate::domain::download::DownloadOperationState;
use crate::domain::embedding_constants::DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME;
use crate::infrastructure::audit::{get_audit_logger, AuditAction};
use crate::interfaces::di::Container;

#[derive(Debug, serde::Serialize)]
struct DownloadDefaultModelResponse {
    download_id: String,
    model_id: String,
    model_name: String,
    file_path: String,
    file_size_bytes: u64,
}

// =============================================================================
// Gateway Impl Functions - Async implementations for gateway dispatch
// =============================================================================

/// Check first-run status implementation.
///
/// Detects if this is the first run by checking for existing .onnx models.
/// Returns recommendation for default model if no models are found.
/// Called by gateway - async dispatch.
pub async fn check_first_run_status_impl(
    container: &Container,
) -> std::result::Result<String, String> {
    tracing::info!("Command: check_first_run_status - ENTRY");

    let models_path = container.models_path();
    let use_case = CheckFirstRunStatusUseCase::new(models_path);
    let response = use_case
        .execute()
        .await
        .map_err(|e| format!("First run check failed: {}", e))?;

    let logger = get_audit_logger();
    if response.needs_setup {
        audit_success!(
            logger,
            AuditAction::SystemStartup,
            "first_run_check:setup_needed",
            "recommended_model" => response.recommended_model_id.as_deref().unwrap_or("none")
        )
        .await
        .ok();
    } else {
        audit_success!(
            logger,
            AuditAction::SystemStartup,
            "first_run_check:models_exist"
        )
        .await
        .ok();
    }

    serde_json::to_string(&response).map_err(|e| format!("Serialization error: {}", e))
}

/// Download default embedding model implementation.
///
/// Downloads the recommended default model (DEFAULT_EMBEDDING_MODEL_NAME) for first-run setup.
/// Progress can be monitored via download events.
/// Called by gateway - async dispatch.
pub async fn download_default_embedding_model_impl(
    container: &Container,
) -> std::result::Result<String, String> {
    tracing::info!("Command: download_default_embedding_model - ENTRY");

    let model_id = DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME.to_string();
    let model_name = DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME.to_string();

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

    let logger = get_audit_logger();
    audit_success!(
        logger,
        AuditAction::ModelDownloaded,
        &payload.model_id,
        "download_id" => &payload.download_id,
        "model_name" => &payload.model_name,
        "file_size_bytes" => &payload.file_size_bytes.to_string()
    )
    .await
    .ok();

    serde_json::to_string(&payload).map_err(|e| format!("Serialization error: {}", e))
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interfaces::di::Container;
    use tempfile::TempDir;

    /// Helper to create a test container with temp database
    async fn create_test_container() -> (Container, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Create empty DB file first
        std::fs::File::create(&db_path).unwrap();

        let db_url = format!("sqlite:{}", db_path.display());

        let pool = sqlx::SqlitePool::connect(&db_url).await.unwrap();

        // Run migrations
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        let security_context =
            std::sync::Arc::new(crate::infrastructure::security::SecurityContext::new());

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
        )
        .await
        .unwrap();

        (container, temp_dir)
    }

    #[tokio::test]
    #[ignore] // Requires writable filesystem, run manually with --ignored
    async fn test_check_first_run_status_command() {
        let (container, _temp_dir) = create_test_container().await;

        // Get models path from Container (proper DI)
        let models_path = container.models_path();

        // Create and execute use case directly (bypassing Tauri State wrapper)
        let use_case = CheckFirstRunStatusUseCase::new(models_path);
        let response = use_case.execute().await.unwrap();

        // Should need setup (no models in temp directory)
        assert!(response.needs_setup);
    }

    #[tokio::test]
    #[ignore] // Requires writable filesystem, run manually with --ignored
    async fn test_models_path_from_container() {
        let (container, temp_dir) = create_test_container().await;

        let models_path = container.models_path();

        // Should be a valid path
        assert!(models_path.to_string_lossy().contains("models"));

        // Verify fallback works (should use home dir)
        assert!(models_path.exists() || models_path.to_string_lossy().contains(".recall"));
    }
}
