use crate::domain::downloaded_model::DownloadedModel;
use crate::domain::events::model_download_events::*;
use crate::domain::repositories::unit_of_work::ModelFileRepositoryPort;
use crate::domain::repositories::UnitOfWorkFactory;
use crate::domain::value_objects::model_status::FileStatus;
use crate::infrastructure::event_bus::EventBus;
use crate::infrastructure::persistence::repositories::downloaded_model_repository::DownloadedModelRepository;
use crate::persistence::repositories::model_file::SqliteModelFileRepository;
use chrono::Utc;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info, warn};
use uuid::Uuid;

pub struct DownloadSaga {
    event_bus: Arc<EventBus<ModelDownloadEvent>>,
    model_file_repo: Arc<dyn ModelFileRepositoryPort>,
    uow_factory: Arc<dyn UnitOfWorkFactory>,
    downloaded_model_repo: Arc<DownloadedModelRepository>,
}

impl DownloadSaga {
    fn select_primary_model_file<'a>(
        files: &'a [crate::domain::entities::model_file::ModelFile],
    ) -> Option<&'a crate::domain::entities::model_file::ModelFile> {
        files
            .iter()
            .find(|file| {
                file.file_name == "model.onnx" || file.file_name.ends_with("/model.onnx")
            })
            .or_else(|| {
                files.iter().find(|file| {
                    file.file_name.ends_with(".onnx") && !file.file_name.ends_with(".onnx_data")
                })
            })
            .or_else(|| files.iter().find(|file| file.file_name.ends_with(".gguf")))
            .or_else(|| files.first())
    }

    pub fn new(
        event_bus: Arc<EventBus<ModelDownloadEvent>>,
        model_file_repo: Arc<dyn ModelFileRepositoryPort>,
        uow_factory: Arc<dyn UnitOfWorkFactory>,
        downloaded_model_repo: Arc<DownloadedModelRepository>,
    ) -> Self {
        Self {
            event_bus,
            model_file_repo,
            uow_factory,
            downloaded_model_repo,
        }
    }

    pub async fn start(&self) {
        let mut receiver = self.event_bus.subscribe();

        loop {
            match receiver.recv().await {
                Ok(event) => {
                    if let Err(e) = self.handle_event(event).await {
                        error!("Saga error handling event: {}", e);
                    }
                }
                Err(e) => {
                    warn!("Saga event receiver error: {}", e);
                }
            }
        }
    }

    async fn handle_event(
        &self,
        event: ModelDownloadEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match event {
            ModelDownloadEvent::ModelDownloadRequested(e) => {
                self.handle_model_requested(e).await?;
            }
            ModelDownloadEvent::FileDownloadCompleted(e) => {
                self.handle_file_completed(e).await?;
            }
            ModelDownloadEvent::FileDownloadFailed(e) => {
                self.handle_file_failed(e).await?;
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_model_requested(
        &self,
        event: ModelDownloadRequestedEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Saga: Model download requested for {}", event.model_id);
        Ok(())
    }

    async fn handle_file_completed(
        &self,
        event: FileDownloadCompletedEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!(
            "Saga: File completed {}/{}",
            event.model_id, event.file_name
        );

        let mut uow = self.uow_factory.create().await?;
        let file_size = event.file_size as i64;

        // CRITICAL: Check completion WITHIN transaction to avoid isolation bug
        let tx_result: Result<bool, Box<dyn std::error::Error + Send + Sync>> = {
            let model_file_repo = uow.model_file_repository()?;
            model_file_repo
                .update_file_status_by_model_and_name(
                    &event.model_id,
                    &event.file_name,
                    FileStatus::Completed,
                    Some(file_size),
                )
                .await?;

            let total_files = model_file_repo.count_total_files(&event.model_id).await?;
            let completed_files = model_file_repo
                .count_completed_files(&event.model_id)
                .await?;

            info!(
                "Saga: Model {} progress: {}/{} files",
                event.model_id, completed_files, total_files
            );

            let is_complete = completed_files == total_files;
            if is_complete {
                info!(
                    "Saga: All files completed for {}, marking as completed",
                    event.model_id
                );
            }

            // After first scope drops, update model status if needed
            if is_complete {
                let model_repo = uow.model_repository()?;
                model_repo.update_status_completed(&event.model_id).await?;
            }

            Ok(is_complete)
        };

        let should_complete_model = match tx_result {
            Ok(is_complete) => is_complete,
            Err(err) => {
                if let Err(rollback_err) = uow.rollback().await {
                    warn!(
                        "Saga: rollback failed after file completion transaction error for {}: {}",
                        event.model_id, rollback_err
                    );
                }
                return Err(err);
            }
        };

        uow.commit().await?;

        let total_files = self
            .model_file_repo
            .count_total_files(&event.model_id)
            .await?;
        let completed_files = self
            .model_file_repo
            .count_completed_files(&event.model_id)
            .await?;

        if completed_files == total_files {
            // Create a new UnitOfWork to get model  info
            let mut uow2 = self.uow_factory.create().await?;
            let model_result: Result<_, Box<dyn std::error::Error + Send + Sync>> = {
                let model_repo = uow2.model_repository()?;
                let model = model_repo
                    .find_by_model_id(&event.model_id)
                    .await?
                    .ok_or_else(|| format!("Model not found: {}", event.model_id))?;
                Ok(model)
            };
            if let Err(e) = uow2.rollback().await {
                warn!(
                    "Saga: Failed to close read-only transaction after completion lookup for {}: {}",
                    event.model_id, e
                );
            }
            let model = model_result?;

            let mut files = self
                .model_file_repo
                .find_by_model_id(&event.model_id)
                .await?;

            // Oracle: Sort files to ensure deterministic file path selection for multi-file models
            files.sort_by(|a, b| a.file_path.cmp(&b.file_path));

            let total_size: u64 = files.iter().map(|f| f.size_bytes as u64).sum();

            // CRITICAL: Create and save DownloadedModel BEFORE emitting completion event
            // This ensures the database record exists when the frontend queries after receiving the event
            info!(
                "Saga: Creating DownloadedModel record for {} before emitting completion event",
                event.model_id
            );

            let main_file = Self::select_primary_model_file(&files)
                .ok_or_else(|| format!("No files found for model {}", event.model_id))?;

            let file_path = PathBuf::from(&main_file.file_path);

            let downloaded_model = DownloadedModel::new(
                Uuid::new_v4().to_string(),
                model.model_name().to_string(),
                event.model_id.clone(),
                file_path,
                total_size as i64,
                model.architecture.clone(),
                model.metadata.clone(),
            )
            .map_err(|e| format!("Failed to create DownloadedModel: {}", e))?;

            // Save to database - this MUST succeed before we emit the event
            self.downloaded_model_repo
                .save(&downloaded_model)
                .await
                .map_err(|e| format!("Failed to save DownloadedModel: {}", e))?;

            info!(
                "Saga: DownloadedModel saved successfully for {}, now emitting completion event",
                event.model_id
            );

            // ONLY after successful save, emit the completion event
            let completion_event = ModelDownloadCompletedEvent {
                model_id: event.model_id.clone(),
                model_name: model.model_name().to_string(),
                total_size,
                file_count: files.len(),
                timestamp: Utc::now(),
            };

            if let Err(e) = self
                .event_bus
                .publish(ModelDownloadEvent::ModelDownloadCompleted(completion_event))
            {
                error!(
                    "CRITICAL: Failed to publish ModelDownloadCompleted event for model '{}'. \
                    No active subscribers (EventBridge likely disconnected). Error: {:?}",
                    event.model_id, e
                );
            }
        }

        Ok(())
    }

    async fn handle_file_failed(
        &self,
        event: FileDownloadFailedEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        error!(
            "Saga: File failed for model {}: {} - {}",
            event.model_id, event.file_name, event.error
        );

        let mut uow = self.uow_factory.create().await?;

        let tx_result: Result<(), Box<dyn std::error::Error + Send + Sync>> = {
            let model_file_repo = uow.model_file_repository()?;
            model_file_repo
                .update_file_status_by_model_and_name(
                    &event.model_id,
                    &event.file_name,
                    FileStatus::Failed,
                    None,
                )
                .await?;

            let model_repo = uow.model_repository()?;
            model_repo.update_status_failed(&event.model_id).await?;
            Ok(())
        };

        if let Err(err) = tx_result {
            if let Err(rollback_err) = uow.rollback().await {
                warn!(
                    "Saga: rollback failed after file failure transaction error for {}: {}",
                    event.model_id, rollback_err
                );
            }
            return Err(err);
        }

        uow.commit().await?;

        // Create a new UnitOfWork to get model info
        let mut uow2 = self.uow_factory.create().await?;
        let model_result: Result<_, Box<dyn std::error::Error + Send + Sync>> = {
            let model_repo = uow2.model_repository()?;
            let model = model_repo
                .find_by_model_id(&event.model_id)
                .await?
                .ok_or_else(|| format!("Model not found: {}", event.model_id))?;
            Ok(model)
        };
        if let Err(e) = uow2.rollback().await {
            warn!(
                "Saga: Failed to close read-only transaction after failure lookup for {}: {}",
                event.model_id, e
            );
        }
        let model = model_result?;

        let failure_event = ModelDownloadFailedEvent {
            model_id: event.model_id.clone(),
            model_name: model.model_name().to_string(),
            error: format!("File download failed: {}", event.error),
            timestamp: Utc::now(),
        };

        if let Err(e) = self
            .event_bus
            .publish(ModelDownloadEvent::ModelDownloadFailed(failure_event))
        {
            error!(
                "CRITICAL: Failed to publish ModelDownloadFailed event for model '{}'. \
                No active subscribers. Error: {:?}",
                event.model_id, e
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::DownloadSaga;
    use crate::domain::entities::model_file::ModelFile;
    use crate::domain::value_objects::model_status::FileStatus;
    use chrono::Utc;

    fn make_file(file_name: &str, file_path: &str) -> ModelFile {
        ModelFile {
            id: "id".to_string(),
            model_id: "model".to_string(),
            file_name: file_name.to_string(),
            file_path: file_path.to_string(),
            relative_path: file_name.to_string(),
            size_bytes: 0,
            downloaded_bytes: 0,
            checksum_sha256: None,
            download_url: "https://example.com".to_string(),
            status: FileStatus::Completed,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            downloaded_at: None,
        }
    }

    #[test]
    fn select_primary_prefers_model_onnx() {
        let files = vec![
            make_file("config.json", "/tmp/config.json"),
            make_file("model.onnx", "/tmp/model.onnx"),
            make_file("tokenizer.json", "/tmp/tokenizer.json"),
        ];

        let selected = DownloadSaga::select_primary_model_file(&files).expect("file selected");
        assert_eq!(selected.file_name, "model.onnx");
    }

    #[test]
    fn select_primary_prefers_onnx_in_subdirectory() {
        let files = vec![
            make_file("config.json", "/tmp/config.json"),
            make_file("onnx/model.onnx", "/tmp/onnx/model.onnx"),
            make_file("tokenizer.json", "/tmp/tokenizer.json"),
        ];

        let selected = DownloadSaga::select_primary_model_file(&files).expect("file selected");
        assert_eq!(selected.file_name, "onnx/model.onnx");
    }

    #[test]
    fn select_primary_falls_back_to_gguf() {
        let files = vec![
            make_file("config.json", "/tmp/config.json"),
            make_file("phi.gguf", "/tmp/phi.gguf"),
        ];

        let selected = DownloadSaga::select_primary_model_file(&files).expect("file selected");
        assert_eq!(selected.file_name, "phi.gguf");
    }
}
