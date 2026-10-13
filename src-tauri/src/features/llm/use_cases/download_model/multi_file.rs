//! Downloading a model that spans several files as one batch.

use chrono::Utc;
use tracing::{info, warn};
use uuid::Uuid;

use crate::domain::download::DownloadOperationState;
use crate::domain::entities::model::Model;
use crate::domain::entities::model_file::ModelFile;
use crate::domain::value_objects::model_status::{FileStatus, ModelStatus};
use crate::features::llm::dto::DownloadModelResponseDto;
use crate::infrastructure::adapters::fs::model_paths::ModelPaths;
use crate::shared::error::AppError;

use super::{DownloadBatchItem, DownloadModelUseCase, DownloadRequest};

impl DownloadModelUseCase {
    /// Download a multi-file model (e.g., BGE-M3 with multiple ONNX files)
    pub(super) async fn download_multi_file_model(
        &self,
        model_id: &str,
        curated: &crate::features::model_management::domain::ModelMetadata,
        model_path: &std::path::Path,
        paths: &ModelPaths,
    ) -> Result<DownloadModelResponseDto, AppError> {
        let model_name = &curated.name;
        let mut download_ids = Vec::new();

        // MODULE 3: Enhanced logging & verification at START
        info!(
            model_id = %model_id,
            file_count = curated.files.len(),
            files = ?curated.files.iter().map(|f| &f.filename).collect::<Vec<_>>(),
            total_size_gb = curated.total_size_bytes as f64 / 1_000_000_000.0,
            "Starting multi-file model download"
        );

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
                "Created parent model record with Pending status for multi-file model"
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

            for file in &curated.files {
                let destination = paths.manifest_file_path(&file.filename)?;

                let model_file = ModelFile {
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
                };

                model_file_repo.create(&model_file).await.map_err(|e| {
                    AppError::Database(format!("Failed to create model_file record: {}", e))
                })?;

                info!(
                    model_id = %model_id,
                    file_name = %file.filename,
                    "Created model_file record with Pending status"
                );
            }

            info!(
                model_id = %model_id,
                file_count = curated.files.len(),
                "Pre-created all model_file records before download"
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
                        "Failed to create model_file records: {}; rollback failed: {}",
                        err, rollback_err
                    )));
                }
                return Err(err);
            }
        }

        // Yield to allow connection pool to reclaim connections
        tokio::task::yield_now().await;

        let auth_token = if curated.requires_auth {
            match self.credentials.get_api_key("huggingface_token").await {
                Ok(Some(token)) => {
                    info!(model_id = %model_id, "Model requires authentication, using HuggingFace token for all files");
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

        let mut batch_items = Vec::with_capacity(curated.files.len());
        for (index, file) in curated.files.iter().enumerate() {
            let destination = paths.manifest_file_path(&file.filename)?;

            // MODULE 4: Enhanced download status tracking
            info!(
                file = %file.filename,
                file_num = index + 1,
                total_files = curated.files.len(),
                size_mb = file.size_bytes as f64 / 1_000_000.0,
                url = %file.url,
                "Starting file download"
            );

            let checksum = if let Some(checksum_str) = &file.checksum {
                Some(
                    crate::domain::download::Checksum::new(
                        crate::domain::download::ChecksumAlgorithm::Sha256,
                        checksum_str.clone(),
                    )
                    .map_err(|e| {
                        AppError::InvalidInput(format!(
                            "Invalid checksum for {}: {}",
                            file.filename, e
                        ))
                    })?,
                )
            } else {
                None
            };

            let mut attempt_urls = vec![file.url.clone()];
            if let Some(fallback_url) =
                Self::fallback_url_for_onnx_metadata(&file.filename, &file.url)
            {
                if fallback_url != file.url {
                    attempt_urls.push(fallback_url);
                }
            }

            let requests = attempt_urls
                .into_iter()
                .map(|attempt_url| DownloadRequest {
                    url: attempt_url.clone(),
                    destination: destination.clone(),
                    checksum: checksum.clone(),
                    auth_token: auth_token.clone(),
                    model_name: Some(model_name.clone()),
                    model_id: Some(model_id.to_string()),
                    model_file_name: Some(file.filename.clone()),
                })
                .collect();
            batch_items.push(DownloadBatchItem { requests });
        }

        let ids = match self
            .download_manager
            .start_download_batch(batch_items)
            .await
        {
            Ok(ids) => ids,
            Err(error) => {
                let mut uow = self.uow_factory.create().await?;
                let update_result = {
                    let model_repo = uow.model_repository()?;
                    let model_file_repo = uow.model_file_repository()?;

                    model_repo.update_status_failed(model_id).await?;
                    for file in &curated.files {
                        model_file_repo
                            .update_file_status_by_model_and_name(
                                model_id,
                                &file.filename,
                                FileStatus::Failed,
                                Some(file.size_bytes as i64),
                            )
                            .await?;
                    }
                    Ok::<(), AppError>(())
                };

                match update_result {
                    Ok(()) => uow.commit().await?,
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

                return Err(AppError::Network(format!(
                    "Failed to prepare model download: {}",
                    error
                )));
            }
        };

        download_ids.extend(
            ids.into_iter()
                .zip(curated.files.iter().map(|file| file.filename.clone())),
        );

        info!(
            model_id = %model_id,
            download_count = download_ids.len(),
            "All downloads initiated, returning immediately"
        );

        // Sum concrete session sizes from DownloadManager to avoid surfacing stale/estimated
        // catalog sizes in UI while download starts.
        let mut detected_total_size_bytes = 0u64;
        for (download_id, _) in &download_ids {
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
            curated.total_size_bytes
        };

        Ok(DownloadModelResponseDto {
            state: DownloadOperationState::DownloadStarted {
                total_size_bytes,
                files_to_download: curated.files.len(),
            },
            path: Some(model_path.to_string_lossy().to_string()),
        })
    }
}
