use crate::features::download::events::model_download_events::*;
use crate::infrastructure::events::event_bus::EventBus;
use crate::infrastructure::persistence::repositories::downloaded_models::DownloadedModelRepository;
use chrono::Utc;
use lattice_core::application::ports::unit_of_work::ModelFileRepositoryPort;
use lattice_core::application::ports::UnitOfWorkFactory;
use lattice_core::domain::models::checkpoint::has_tokenizer;
use lattice_core::domain::models::downloaded::{DownloadedModel, ModelLocation};
use lattice_core::domain::value_objects::model_status::FileStatus;
use lattice_core::domain::value_objects::ArtifactIdentity;
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
        files: &[lattice_core::domain::entities::model_file::ModelFile],
    ) -> Option<&lattice_core::domain::entities::model_file::ModelFile> {
        files
            .iter()
            .find(|file| file.file_name.ends_with(".gguf"))
            // Safetensors LLM repos contain a `config.json`. Treat it as
            // the canonical primary file — the loader takes the parent
            // directory anyway, and config.json is the only file
            // guaranteed to be present in every safetensors layout
            // (single- or multi-shard).
            .or_else(|| files.iter().find(|file| file.file_name == "config.json"))
            .or_else(|| files.first())
    }

    /// The identity of an embedding GGUF whose download was checked against a
    /// pinned SHA-256: the transfer engine hashed the file and refused it on a
    /// mismatch, so the digest is the file's, and activation need not read
    /// 1.2 GB again. `None` when the download carried no digest.
    fn verified_gguf_identity(
        primary: &lattice_core::domain::entities::model_file::ModelFile,
    ) -> Option<ArtifactIdentity> {
        if !primary.file_name.ends_with(".gguf") {
            return None;
        }
        let digest = primary.checksum_sha256.as_deref()?.to_ascii_lowercase();
        ArtifactIdentity::parse(&format!("sha256:{digest}")).ok()
    }

    fn model_location_for_download(
        primary: &lattice_core::domain::entities::model_file::ModelFile,
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
                    // Slow consumer dropped events; the channel is still live. Logged
                    // loudly because a dropped download-complete is critical state.
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
                    lattice_core::domain::models::catalog_cache::ModelCatalogCache::instance()
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
                    Ok(None)
                        if !downloaded_model
                            .location()
                            .enclosing_dir()
                            .is_some_and(|dir| has_tokenizer(&dir)) =>
                    {
                        info!(
                            model_id = %event.model_id,
                            "Saga: embedding model has no tokenizer.json beside it; not auto-activated"
                        )
                    }
                    Ok(None) => {
                        // The completion write above has committed; any hashing
                        // happens off the runtime thread, outside it.
                        let verified = Self::verified_gguf_identity(main_file);
                        let activation = async {
                            let identity = match verified {
                                Some(identity) => Some(identity),
                                None => {
                                    crate::infrastructure::ml::artifact_identity::establish(
                                        &downloaded_model,
                                    )
                                    .await?
                                }
                            };
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

        // A file already on disk and recorded as complete stays that way: the
        // failure can belong to a different session for the same file.
        let already_complete = {
            let model_file_repo = uow.model_file_repository()?;
            model_file_repo
                .find_by_model_id(&event.model_id)
                .await?
                .iter()
                .any(|file| {
                    file.file_name == event.file_name
                        && matches!(file.status, FileStatus::Completed)
                })
        };
        if already_complete {
            warn!(
                "Saga: ignoring a failure for {}/{}, which has already completed",
                event.model_id, event.file_name
            );
            uow.rollback().await?;
            return Ok(());
        }

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
mod tests;
