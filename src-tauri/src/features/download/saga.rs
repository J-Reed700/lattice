use crate::application::ports::unit_of_work::ModelFileRepositoryPort;
use crate::application::ports::UnitOfWorkFactory;
use crate::domain::downloaded_model::{DownloadedModel, ModelLocation};
use crate::domain::value_objects::model_status::FileStatus;
use crate::features::download::downloaded_model_repository::DownloadedModelRepository;
use crate::features::download::events::model_download_events::*;
use crate::infrastructure::event_bus::EventBus;
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
    fn select_primary_model_file(
        files: &[crate::domain::entities::model_file::ModelFile],
    ) -> Option<&crate::domain::entities::model_file::ModelFile> {
        files
            .iter()
            .find(|file| file.file_name == "model.onnx" || file.file_name.ends_with("/model.onnx"))
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

    fn model_location_for_download(
        primary: &crate::domain::entities::model_file::ModelFile,
    ) -> Option<ModelLocation> {
        let path = PathBuf::from(&primary.file_path);
        if primary.file_name == "config.json" {
            path.parent().map(|p| ModelLocation::LocalDirectory {
                path: p.to_path_buf(),
            })
        } else {
            Some(ModelLocation::LocalFile { path })
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

    /// Run the saga until either the event bus closes or `cancel` fires.
    /// The biased select makes shutdown win when both branches are ready.
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

        let _should_complete_model = match tx_result {
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

            // Sort files to select a deterministic path for multi-file models.
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

            let location = Self::model_location_for_download(main_file).ok_or_else(|| {
                format!(
                    "Primary file has no parent directory for model {}",
                    event.model_id
                )
            })?;

            let downloaded_model = DownloadedModel::new_with_catalog(
                Uuid::new_v4().to_string(),
                model.model_name().to_string(),
                event.model_id.clone(),
                location,
                total_size as i64,
                model.architecture.clone(),
                model.metadata.clone(),
                |identifier| {
                    crate::features::model_management::catalog_cache::ModelCatalogCache::instance()
                        .lookup(identifier)
                },
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

            // First-download-wins: only auto-activate when the slot is
            // empty so a deliberate user assignment is never stolen.
            if downloaded_model.is_embedding_model() {
                match self
                    .downloaded_model_repo
                    .get_active_embedding_model()
                    .await
                {
                    Ok(None) => {
                        // The completion write above has committed; hashing the
                        // artifacts happens off the runtime thread, outside it.
                        let activation = async {
                            let identity =
                                crate::features::embedding::artifact_identity::establish(
                                    &downloaded_model,
                                )
                                .await?;
                            self.downloaded_model_repo
                                .set_active_embedding_model(&event.model_id, identity.as_ref())
                                .await
                        };
                        if let Err(e) = activation.await {
                            warn!(
                                model_id = %event.model_id,
                                error = %e,
                                "Saga: failed to auto-activate empty embedding slot"
                            );
                        } else {
                            info!(
                                model_id = %event.model_id,
                                "Saga: auto-activated newly-downloaded embedding model (slot was empty)"
                            );
                        }
                    }
                    Ok(Some(_)) => {}
                    Err(e) => warn!(
                        error = %e,
                        "Saga: failed to read active embedding slot for auto-activate"
                    ),
                }
            } else if downloaded_model.is_chat_model() {
                match self.downloaded_model_repo.get_active_chat_model().await {
                    Ok(None) => {
                        if let Err(e) = self
                            .downloaded_model_repo
                            .set_active_chat_model(&event.model_id)
                            .await
                        {
                            warn!(
                                model_id = %event.model_id,
                                error = %e,
                                "Saga: failed to auto-activate empty chat slot"
                            );
                        } else {
                            info!(
                                model_id = %event.model_id,
                                "Saga: auto-activated newly-downloaded chat model (slot was empty)"
                            );
                        }
                    }
                    Ok(Some(_)) => {}
                    Err(e) => warn!(
                        error = %e,
                        "Saga: failed to read active chat slot for auto-activate"
                    ),
                }
            }

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
    use crate::domain::downloaded_model::ModelLocation;
    use crate::domain::entities::model_file::ModelFile;
    use crate::domain::value_objects::model_status::FileStatus;
    use chrono::Utc;
    use std::path::PathBuf;

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
    fn model_location_returns_directory_for_config_json() {
        let primary = make_file("config.json", "/tmp/gemma-2-9b-it/config.json");
        let location = DownloadSaga::model_location_for_download(&primary).unwrap();
        assert_eq!(
            location,
            ModelLocation::LocalDirectory {
                path: PathBuf::from("/tmp/gemma-2-9b-it")
            }
        );
    }

    #[tokio::test]
    async fn first_embedding_download_is_activated_with_its_identity() {
        use crate::features::download::downloaded_model_repository::DownloadedModelRepository;
        use crate::features::download::events::model_download_events::FileDownloadCompletedEvent;
        use crate::features::embedding::artifact_identity;
        use crate::infrastructure::event_bus::EventBus;
        use crate::infrastructure::persistence::repositories::model_file::SqliteModelFileRepository;
        use crate::infrastructure::persistence::repositories::unit_of_work::SqliteUnitOfWorkFactory;
        use std::sync::Arc;

        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .expect("create in-memory pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("run migrations");

        let dir = tempfile::tempdir().expect("create model directory");
        let files = [
            ("config.json", &b"{}"[..]),
            ("tokenizer.json", &b"{}"[..]),
            ("model.safetensors", &b"weights"[..]),
        ];
        sqlx::query(
            "INSERT INTO models (id, model_name, model_id, base_path, model_type, architecture, status)
             VALUES ('row-1', 'test-embed-model', 'test-embed-model', ?1, 'embedding', 'bert', 'downloading')",
        )
        .bind(dir.path().to_string_lossy().into_owned())
        .execute(&pool)
        .await
        .expect("insert model row");
        for (name, contents) in files {
            let path = dir.path().join(name);
            std::fs::write(&path, contents).expect("write model file");
            // config.json is the file whose completion finishes the download.
            let status = if name == "config.json" {
                "downloading"
            } else {
                "completed"
            };
            sqlx::query(
                "INSERT INTO model_files
                     (id, model_id, file_name, file_path, relative_path, size_bytes, download_url, status)
                 VALUES (?1, 'test-embed-model', ?2, ?3, ?2, ?4, 'https://example.com', ?5)",
            )
            .bind(format!("file-{name}"))
            .bind(name)
            .bind(path.to_string_lossy().into_owned())
            .bind(contents.len() as i64)
            .bind(status)
            .execute(&pool)
            .await
            .expect("insert model file row");
        }

        let downloaded_models = Arc::new(DownloadedModelRepository::new(pool.clone()));
        let saga = DownloadSaga::new(
            Arc::new(EventBus::new()),
            Arc::new(SqliteModelFileRepository::new(pool.clone())),
            Arc::new(SqliteUnitOfWorkFactory::new(pool.clone())),
            Arc::clone(&downloaded_models),
        );

        saga.handle_file_completed(FileDownloadCompletedEvent {
            model_id: "test-embed-model".to_string(),
            file_id: "file-config.json".to_string(),
            file_name: "config.json".to_string(),
            file_size: 2,
            timestamp: Utc::now(),
        })
        .await
        .expect("handle final file completion");

        let active = downloaded_models
            .get_active_embedding_model()
            .await
            .expect("read active embedding model")
            .expect("the empty embedding slot is filled by the first download");
        assert_eq!(active.model_id(), "test-embed-model");
        assert_eq!(
            active.embedding_artifact_identity(),
            Some(&artifact_identity::compute(dir.path()).expect("compute identity")),
            "a local embedding model is activated with the identity of its files"
        );
    }

    #[test]
    fn model_location_returns_local_file_for_gguf() {
        // GGUF stays a single-file path under LocalFile.
        let primary = make_file("phi.gguf", "/tmp/phi/phi.gguf");
        let location = DownloadSaga::model_location_for_download(&primary).unwrap();
        assert_eq!(
            location,
            ModelLocation::LocalFile {
                path: PathBuf::from("/tmp/phi/phi.gguf")
            }
        );
    }
}
