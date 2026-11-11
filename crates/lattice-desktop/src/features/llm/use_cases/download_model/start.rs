//! Starting a model download: the pending rows, then one batch of transfers.

use chrono::Utc;
use tracing::{info, warn};
use uuid::Uuid;

use crate::features::llm::dto::DownloadModelResponseDto;
use crate::features::llm::engine::compatibility::check_download;
use crate::infrastructure::adapters::fs::model_paths::ModelPaths;
use crate::infrastructure::persistence::repositories::models_table_type::models_table_type;
use lattice_core::application::services::credentials::CredentialKey;
use lattice_core::domain::download::{Checksum, ChecksumAlgorithm, DownloadOperationState};
use lattice_core::domain::entities::model::Model;
use lattice_core::domain::entities::model_file::ModelFile;
use lattice_core::domain::models::selection::{ModelCategory, ModelMetadata};
use lattice_core::domain::value_objects::model_status::{FileStatus, ModelStatus};
use lattice_core::shared::error::AppError;

use super::plan::PlannedFile;
use super::{DownloadBatchItem, DownloadModelUseCase, DownloadRequest};

impl DownloadModelUseCase {
    async fn huggingface_token(
        &self,
        model_id: &str,
        meta: &ModelMetadata,
    ) -> Result<Option<String>, AppError> {
        if !meta.requires_auth {
            return Ok(None);
        }
        match self.credentials.get_api_key(CredentialKey::HuggingFaceToken.as_str()).await {
            Ok(Some(token)) => {
                info!(model_id = %model_id, "Model requires authentication, using HuggingFace token");
                Ok(Some(token))
            }
            Ok(None) => Err(AppError::InvalidInput(format!(
                "Model '{}' requires HuggingFace authentication. Please set your HuggingFace token in Settings → HuggingFace.",
                meta.name
            ))),
            Err(e) => {
                warn!(error = ?e, model_id = %model_id, "Failed to retrieve HuggingFace token from secure storage");
                Err(AppError::Other(format!(
                    "Failed to retrieve HuggingFace token: {}. Please check your keychain settings.",
                    e
                )))
            }
        }
    }

    /// Records the model and its files as pending, then starts them as one batch.
    pub(super) async fn download_planned(
        &self,
        model_id: &str,
        meta: &ModelMetadata,
        model_path: &std::path::Path,
        paths: &ModelPaths,
        plan: &[PlannedFile],
    ) -> Result<DownloadModelResponseDto, AppError> {
        info!(
            model_id = %model_id,
            file_count = plan.len(),
            files = ?plan.iter().map(|file| &file.filename).collect::<Vec<_>>(),
            "Starting model download"
        );

        let auth_token = self.huggingface_token(model_id, meta).await?;

        // A GGUF extension says nothing about whether the bundled engine knows
        // its architecture, so read the header before persisting or fetching.
        if matches!(meta.category, ModelCategory::LLM | ModelCategory::Embedding) {
            for file in plan
                .iter()
                .filter(|file| file.filename.to_ascii_lowercase().ends_with(".gguf"))
            {
                check_download(&file.url, auth_token.as_deref()).await?;
            }
        }

        let model = Model {
            id: Uuid::new_v4().to_string(),
            model_id: model_id.to_string(),
            name: meta.name.clone(),
            description: Some(meta.description.clone()),
            base_path: model_path.to_string_lossy().to_string(),
            total_size_bytes: meta.total_size_bytes as i64,
            architecture: "GGUF".to_string(),
            model_type: models_table_type(meta.category).to_string(),
            status: ModelStatus::Pending,
            files: vec![],
            is_active_for_chat: false,
            is_active_for_embedding: false,
            use_count: 0,
            last_used_at: None,
            metadata: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            downloaded_at: None,
        };

        let mut model_files = Vec::with_capacity(plan.len());
        let mut batch_items = Vec::with_capacity(plan.len());
        for file in plan {
            let destination = paths.manifest_file_path(&file.filename)?;
            model_files.push(ModelFile {
                id: Uuid::new_v4().to_string(),
                model_id: model_id.to_string(),
                file_name: file.filename.clone(),
                file_path: destination.to_string_lossy().to_string(),
                relative_path: file.filename.clone(),
                size_bytes: file.size_bytes as i64,
                downloaded_bytes: 0,
                checksum_sha256: file.checksum.clone(),
                download_url: file.url.clone(),
                status: FileStatus::Pending,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                downloaded_at: None,
            });

            let checksum = match &file.checksum {
                Some(value) => Some(
                    Checksum::new(ChecksumAlgorithm::Sha256, value.clone()).map_err(|e| {
                        AppError::InvalidInput(format!(
                            "Invalid checksum for {}: {}",
                            file.filename, e
                        ))
                    })?,
                ),
                None => None,
            };
            batch_items.push(DownloadBatchItem {
                requests: vec![DownloadRequest {
                    url: file.url.clone(),
                    destination,
                    checksum,
                    auth_token: auth_token.clone(),
                    model_name: Some(meta.name.clone()),
                    model_id: Some(model_id.to_string()),
                    model_file_name: Some(file.filename.clone()),
                }],
            });
        }

        self.record_pending(&model, &model_files).await?;

        // Yield to allow connection pool to reclaim connections
        tokio::task::yield_now().await;

        let ids = match self
            .download_manager
            .start_download_batch(batch_items)
            .await
        {
            Ok(ids) => ids,
            Err(error) => {
                let failed_files: Vec<(&str, Option<i64>)> = plan
                    .iter()
                    .map(|file| (file.filename.as_str(), None))
                    .collect();
                self.record_failed(model_id, &failed_files).await?;
                return Err(AppError::Network(format!(
                    "Failed to prepare model download: {}",
                    error
                )));
            }
        };

        info!(
            model_id = %model_id,
            download_count = ids.len(),
            "Downloads started"
        );

        // Prefer the sizes the download manager measured over catalog estimates.
        let mut detected_total_size_bytes = 0u64;
        for download_id in &ids {
            if let Ok(Some(session)) = self.download_manager.get_download_status(download_id).await
            {
                if let Some(total_bytes) = session.progress().total_bytes() {
                    detected_total_size_bytes =
                        detected_total_size_bytes.saturating_add(total_bytes);
                }
            }
        }
        let total_size_bytes = if detected_total_size_bytes > 0 {
            detected_total_size_bytes
        } else {
            meta.total_size_bytes
        };

        Ok(DownloadModelResponseDto {
            state: DownloadOperationState::DownloadStarted {
                total_size_bytes,
                files_to_download: plan.len(),
            },
            path: Some(model_path.to_string_lossy().to_string()),
        })
    }
}
