//! First-run model setup and catalog model downloads.

use crate::features::initialization::use_cases::first_run_setup::CheckFirstRunStatusUseCase;
use crate::features::llm::dto::{DownloadModelRequestDto, DownloadModelResponseDto};
use crate::features::llm::llama_cpp::LlamaCppLlm;
use crate::features::settings::plugin::TestOllamaConnectionResponse;
use crate::interfaces::di::Container;
use lattice_core::domain::download::DownloadOperationState;
use lattice_core::domain::models::embedding_defaults::default_embedding_model;
use lattice_core::shared::error::AppError;
use lattice_core::shared::ipc::ApiError;

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
pub async fn check_first_run_status_impl(
    container: &Container,
) -> std::result::Result<String, String> {
    tracing::info!("Command: check_first_run_status - ENTRY");

    let repository = container.downloaded_models();
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

/// Downloads the default embedding model recommendation.
pub async fn download_default_embedding_model_impl(
    container: &Container,
) -> std::result::Result<String, String> {
    tracing::info!("Command: download_default_embedding_model - ENTRY");

    let default = default_embedding_model();
    let model_id = default.curated_id.to_string();
    let model_name = default.display_name.to_string();

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

/// Downloads an LLM model with progress tracking and integrity verification.
pub async fn download_model_impl(
    model_id: String,
    container: &Container,
) -> Result<DownloadModelResponseDto, AppError> {
    tracing::info!(model_id = %model_id, "Command: download_model - ENTRY");

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

#[tauri::command]
#[specta::specta]
pub async fn test_llama_cpp_connection(
    request: lattice_core::application::contracts::settings::LlamaCppSettingsDto,
) -> Result<TestOllamaConnectionResponse, ApiError> {
    let models = LlamaCppLlm::test_connection(&request)
        .await
        .map_err(ApiError::from)?;
    Ok(TestOllamaConnectionResponse {
        endpoint: "/v1/chat/completions".into(),
        models,
    })
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

        let repository = container.downloaded_models();
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

        assert!(models_path.exists() || models_path.to_string_lossy().contains(".lattice"));
    }
}
