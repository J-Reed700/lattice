//! Downloading a model that is one file.

use chrono::Utc;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::domain::download::DownloadOperationState;
use crate::domain::entities::model::Model;
use crate::domain::entities::model_file::ModelFile;
use crate::domain::value_objects::model_status::{FileStatus, ModelStatus};
use crate::features::llm::dto::DownloadModelResponseDto;
use crate::infrastructure::adapters::fs::model_paths::ModelPaths;
use crate::shared::error::AppError;

use super::{DownloadModelUseCase, DownloadRequest};

impl DownloadModelUseCase {
    /// Download a single-file model (legacy path for GGUF models)
    pub(super) async fn download_single_file_model(
        &self,
        model_id: &str,
        curated: &crate::features::model_management::domain::ModelMetadata,
        model_path: &std::path::Path,
        paths: &ModelPaths,
    ) -> Result<DownloadModelResponseDto, AppError> {
        let download_url = curated.download_url.clone().ok_or_else(|| {
            AppError::InvalidInput(format!("No download URL for model: {}", model_id))
        })?;
        let default_filename = curated.default_filename.clone().ok_or_else(|| {
            AppError::InvalidInput(format!("No filename specified for model: {}", model_id))
        })?;
        let model_name = &curated.name;

        // For HuggingFace models, build the resolve URL using the actual filename
        let model_file_url = if download_url.contains("huggingface.co") {
            format!("{}/resolve/main/{}", download_url, default_filename)
        } else {
            download_url
        };

        let destination = paths.manifest_file_path(&default_filename)?;

        debug!(
            url = %model_file_url,
            destination = %destination.display(),
            "Initiating single-file model download"
        );

        let auth_token = if curated.requires_auth {
            match self.credentials.get_api_key("huggingface_token").await {
                Ok(Some(token)) => {
                    info!(model_id = %model_id, "Model requires authentication, using HuggingFace token");
                    Some(token)
                }
                Ok(None) => {
                    return Err(AppError::InvalidInput(format!(
                        "Model '{}' requires HuggingFace authentication. Please set your HuggingFace token in Settings → HuggingFace.",
                        model_name
                    )));
                }
                Err(e) => {
                    warn!(error = ?e, model_id = %model_id, "Failed to retrieve HuggingFace token from secure storage");
                    return Err(AppError::Other(format!(
                        "Failed to retrieve HuggingFace token: {}. Please check your keychain settings.",
                        e
                    )));
                }
            }
        } else {
            None
        };

        // Inspect the selected file before persisting a download or fetching
        // gigabytes of weights. A GGUF extension alone says nothing about
        // whether the bundled engine recognizes its architecture.
        if curated.category == crate::features::model_management::domain::ModelCategory::LLM {
            crate::features::llm::engine::compatibility::check_download(
                &model_file_url,
                auth_token.as_deref(),
            )
            .await?;
        }

        // STEP 1: Create parent Model record FIRST (satisfies FK constraint)
        // Use explicit scope to ensure connections are released before download starts
        let mut uow = self.uow_factory.create().await?;
        let db_result = {
            let model_repo = uow.model_repository()?;

            let model = Model {
                id: Uuid::new_v4().to_string(),
                model_id: model_id.to_string(),
                name: model_name.clone(),
                description: Some(curated.description.clone()),
                base_path: model_path.to_string_lossy().to_string(),
                total_size_bytes: curated.total_size_bytes as i64,
                architecture: "GGUF".to_string(), // Default to GGUF format for LLM models
                model_type: match curated.category {
                    crate::features::model_management::domain::ModelCategory::LLM => {
                        "chat".to_string()
                    }
                    crate::features::model_management::domain::ModelCategory::Embedding => {
                        "embedding".to_string()
                    }
                    crate::features::model_management::domain::ModelCategory::OCR => {
                        "ocr".to_string()
                    }
                    crate::features::model_management::domain::ModelCategory::Transcription => {
                        // See ModelType::to_db_string: the models-table CHECK
                        // constraint has no `transcription` value yet.
                        "custom".to_string()
                    }
                },
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

            model_repo
                .create_model_with_files(&model, vec![])
                .await
                .map_err(|e| AppError::Database(format!("Failed to create model record: {}", e)))?;

            info!(
                model_id = %model_id,
                model_name = %model_name,
                "Created parent model record with Pending status"
            );
            Ok(())
        };

        match db_result {
            Ok(()) => {
                uow.commit().await.map_err(|e| {
                    AppError::Database(format!("Failed to commit model record transaction: {}", e))
                })?;
            }
            Err(err) => {
                if let Err(rollback_err) = uow.rollback().await {
                    return Err(AppError::Database(format!(
                        "Failed to create model record: {}; rollback failed: {}",
                        err, rollback_err
                    )));
                }
                return Err(err);
            }
        }

        let mut uow = self.uow_factory.create().await?;
        let db_result = {
            let model_file_repo = uow.model_file_repository()?;

            let model_file = ModelFile {
                id: Uuid::new_v4().to_string(),
                model_id: model_id.to_string(),
                file_name: default_filename.clone(),
                file_path: destination.to_string_lossy().to_string(),
                relative_path: default_filename.clone(),
                size_bytes: curated.total_size_bytes as i64,
                downloaded_bytes: 0,
                checksum_sha256: curated.files.first().and_then(|f| f.checksum.clone()),
                download_url: model_file_url.clone(),
                status: FileStatus::Pending,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                downloaded_at: None,
            };

            model_file_repo.create(&model_file).await.map_err(|e| {
                AppError::Database(format!("Failed to create model_file record: {}", e))
            })?;

            info!(
                model_id = %model_id,
                file_name = %default_filename,
                "Created model_file record with Pending status"
            );
            Ok(())
        };

        match db_result {
            Ok(()) => {
                uow.commit().await.map_err(|e| {
                    AppError::Database(format!("Failed to commit model_file transaction: {}", e))
                })?;
            }
            Err(err) => {
                if let Err(rollback_err) = uow.rollback().await {
                    return Err(AppError::Database(format!(
                        "Failed to create model_file record: {}; rollback failed: {}",
                        err, rollback_err
                    )));
                }
                return Err(err);
            }
        }

        // Yield to allow connection pool to reclaim connections
        tokio::task::yield_now().await;

        let download_request = DownloadRequest {
            url: model_file_url.clone(),
            destination: destination.clone(),
            checksum: None,
            auth_token,
            model_name: Some(model_name.clone()),
            model_id: Some(model_id.to_string()),
            model_file_name: Some(default_filename.clone()),
        };

        let download_id = match self.download_manager.start_download(download_request).await {
            Ok(id) => id,
            Err(e) => {
                // Detect network errors
                let error_msg = e.to_string();
                let mapped_error = if error_msg.contains("Network")
                    || error_msg.contains("connection")
                    || error_msg.contains("timeout")
                {
                    AppError::Network(format!("Network error: {}", error_msg))
                } else {
                    AppError::Network(format!("Failed to start download: {}", e))
                };

                // Mark model + file as failed before returning error
                let mut uow = self.uow_factory.create().await?;
                let update_result = {
                    let model_repo = uow.model_repository()?;
                    let model_file_repo = uow.model_file_repository()?;

                    model_repo.update_status_failed(model_id).await?;
                    model_file_repo
                        .update_file_status_by_model_and_name(
                            model_id,
                            &default_filename,
                            FileStatus::Failed,
                            None,
                        )
                        .await?;
                    Ok::<(), AppError>(())
                };

                match update_result {
                    Ok(()) => {
                        uow.commit().await?;
                    }
                    Err(err) => {
                        if let Err(rollback_err) = uow.rollback().await {
                            return Err(AppError::Database(format!(
                                "Failed to mark download failed: {}; rollback failed: {}",
                                err, rollback_err
                            )));
                        }
                        return Err(err);
                    }
                }

                return Err(mapped_error);
            }
        };

        info!(
            download_id = %download_id,
            model_id = %model_id,
            "Single-file download initiated, waiting for completion"
        );

        // Prefer the verified session size discovered by DownloadManager (HEAD/Range),
        // falling back to catalog metadata when unavailable.
        let total_size_bytes = self
            .download_manager
            .get_download_status(&download_id)
            .await
            .ok()
            .flatten()
            .and_then(|session| session.progress().total_bytes())
            .unwrap_or(curated.total_size_bytes);

        Ok(DownloadModelResponseDto {
            state: DownloadOperationState::DownloadStarted {
                total_size_bytes,
                files_to_download: 1,
            },
            path: Some(model_path.to_string_lossy().to_string()),
        })
    }
}
