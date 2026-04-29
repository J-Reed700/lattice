use crate::domain::downloaded_model::{DownloadedModel, ModelBackend};
use crate::domain::events::model_download_events::*;
use crate::domain::repositories::unit_of_work::ModelFileRepositoryPort;
use crate::domain::repositories::UnitOfWorkFactory;
use crate::domain::value_objects::model_status::FileStatus;
use crate::infrastructure::event_bus::EventBus;
use crate::features::download::downloaded_model_repository::DownloadedModelRepository;
use crate::persistence::repositories::model_file::SqliteModelFileRepository;
use chrono::Utc;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn, Instrument};
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
            // Safetensors LLM repos contain a `config.json`. Treat it as
            // the canonical primary file — the loader takes the parent
            // directory anyway, and config.json is the only file
            // guaranteed to be present in every safetensors layout
            // (single- or multi-shard).
            .or_else(|| files.iter().find(|file| file.file_name == "config.json"))
            .or_else(|| files.first())
    }

    /// Resolve the path to store on `DownloadedModel.file_path`. For
    /// single-file formats (GGUF, ONNX) this is the file path itself.
    /// For safetensors LLM/multimodal layouts (config.json + shards),
    /// the loader expects the *directory* — return the parent of
    /// `config.json`. Returns `None` if the primary file has no parent
    /// (shouldn't happen for valid downloads).
    fn primary_path_for_downloaded_model(
        primary: &crate::domain::entities::model_file::ModelFile,
    ) -> Option<PathBuf> {
        let path = PathBuf::from(&primary.file_path);
        if primary.file_name == "config.json" {
            path.parent().map(|p| p.to_path_buf())
        } else {
            Some(path)
        }
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

    /// Run the saga until either the event bus closes or `cancel`
    /// fires. See ConversationSummarySaga::start for the rationale on
    /// the select! biased shape.
    pub async fn start(&self, cancel: CancellationToken) {
        let mut receiver = self.event_bus.subscribe();

        loop {
            tokio::select! {
                biased;

                _ = cancel.cancelled() => {
                    info!("DownloadSaga cancelled; subscriber exiting");
                    return;
                }

                recv = receiver.recv() => match recv {
                    Ok(envelope) => {
                        // Instrument under the publish-time span so the
                        // saga's DB writes appear in the same trace as
                        // the originating download manager event.
                        let span = envelope.span.clone();
                        let payload = envelope.payload;
                        let result = async {
                            self.handle_event(payload).await
                        }
                        .instrument(span)
                        .await;
                        if let Err(e) = result {
                            error!("Saga error handling event: {}", e);
                        }
                    }
                    // Slow consumer dropped events. Channel still live.
                    // Eviction risk for state events is now mitigated
                    // (see P1 fix that filters Progress out at publish
                    // time) but we still log loudly because in-flight
                    // download-complete is critical state.
                    Err(RecvError::Lagged(n)) => {
                        warn!(
                            dropped = n,
                            "DownloadSaga lagged behind producer; events dropped — model state may be inconsistent"
                        );
                    }
                    // Producer side closed. MUST break or spin a CPU
                    // core at 100%.
                    Err(RecvError::Closed) => {
                        info!("DownloadSaga event bus closed; subscriber exiting");
                        return;
                    }
                },
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

        // CRITICAL: Check completion WITHIN transaction to avoid isolation bug.
        // Wrap in async block so `?` short-circuits to tx_result, not out of
        // the function — otherwise we'd skip the rollback arm below and drop
        // `uow` with an open transaction.
        let tx_result: Result<bool, Box<dyn std::error::Error + Send + Sync>> = async {
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
        }
        .await;

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

            let file_path = Self::primary_path_for_downloaded_model(main_file).ok_or_else(
                || format!("Primary file has no parent for model {}", event.model_id),
            )?;

            let downloaded_model = DownloadedModel::new(
                Uuid::new_v4().to_string(),
                model.model_name().to_string(),
                event.model_id.clone(),
                file_path,
                total_size as i64,
                model.architecture.clone(),
                model.metadata.clone(),
                ModelBackend::Local,
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

        // Wrap in async block so `?` short-circuits to tx_result, not out of
        // the function — otherwise we'd skip the rollback arm below and drop
        // `uow` with an open transaction.
        let tx_result: Result<(), Box<dyn std::error::Error + Send + Sync>> = async {
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
        }
        .await;

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

    #[test]
    fn select_primary_picks_config_for_safetensors_layout() {
        // No GGUF/ONNX present; HF safetensors LLM layout. config.json
        // should win over a random shard so the downstream
        // `primary_path_for_downloaded_model` helper can derive the
        // directory path.
        let files = vec![
            make_file(
                "model-00001-of-00004.safetensors",
                "/tmp/m/model-00001-of-00004.safetensors",
            ),
            make_file(
                "model-00002-of-00004.safetensors",
                "/tmp/m/model-00002-of-00004.safetensors",
            ),
            make_file("config.json", "/tmp/m/config.json"),
            make_file("tokenizer.json", "/tmp/m/tokenizer.json"),
        ];

        let selected = DownloadSaga::select_primary_model_file(&files).expect("file selected");
        assert_eq!(selected.file_name, "config.json");
    }

    #[test]
    fn primary_path_returns_directory_for_config_json() {
        // For safetensors, DownloadedModel.file_path should point at
        // the model directory (parent of config.json), because that's
        // what mistralrs::ModelBuilder takes as input.
        let primary = make_file("config.json", "/tmp/gemma-2-9b-it/config.json");
        let path = DownloadSaga::primary_path_for_downloaded_model(&primary).unwrap();
        assert_eq!(path.to_string_lossy(), "/tmp/gemma-2-9b-it");
    }

    #[test]
    fn primary_path_returns_file_path_for_gguf() {
        // GGUF stays a single-file path.
        let primary = make_file("phi.gguf", "/tmp/phi/phi.gguf");
        let path = DownloadSaga::primary_path_for_downloaded_model(&primary).unwrap();
        assert_eq!(path.to_string_lossy(), "/tmp/phi/phi.gguf");
    }
}
