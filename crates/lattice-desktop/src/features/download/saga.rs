use crate::features::download::events::model_download_events::*;
use crate::infrastructure::events::event_bus::EventBus;
use crate::infrastructure::persistence::repositories::downloaded_models::{
    self, DownloadedModelRepository,
};
use crate::infrastructure::persistence::repositories::model::ops as model_ops;
use crate::infrastructure::persistence::repositories::model_file::ops as model_file_ops;
use chrono::Utc;
use lattice_core::application::contracts::settings::SettingsCategory;
use lattice_core::application::ports::SettingsRepositoryPort;
use lattice_core::domain::entities::model::Model;
use lattice_core::domain::entities::model_file::ModelFile;
use lattice_core::domain::models::checkpoint::has_tokenizer;
use lattice_core::domain::models::downloaded::{DownloadedModel, ModelLocation};
use lattice_core::domain::models::model_ref::ModelRef;
use lattice_core::domain::value_objects::model_status::FileStatus;
use lattice_core::domain::value_objects::ArtifactIdentity;
use lattice_core::shared::persistence::Database;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn, Instrument};
use uuid::Uuid;

pub struct DownloadSaga {
    event_bus: Arc<EventBus<ModelDownloadEvent>>,
    database: Database,
    downloaded_model_repo: Arc<DownloadedModelRepository>,
    settings: Option<Arc<dyn SettingsRepositoryPort>>,
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
        database: Database,
        downloaded_model_repo: Arc<DownloadedModelRepository>,
    ) -> Self {
        Self {
            event_bus,
            database,
            downloaded_model_repo,
            settings: None,
        }
    }

    /// Lets a first-run chat activation also set the Learning model.
    pub fn with_settings(mut self, settings: Arc<dyn SettingsRepositoryPort>) -> Self {
        self.settings = Some(settings);
        self
    }

    /// Makes `model_id` the Learning model when none is set; one the user
    /// chose is never replaced.
    async fn default_learning_model(&self, model_id: &str) {
        let Some(settings) = &self.settings else {
            return;
        };
        let current = match settings.get_all().await {
            Ok(current) => current,
            Err(e) => {
                warn!(error = %e, "Saga: could not read settings to default the Learning model");
                return;
            }
        };
        if current.llm.learning_model.is_some() {
            return;
        }
        let model = ModelRef::Local {
            model_id: model_id.to_string(),
        };
        let Ok(value) = serde_json::to_value(&model) else {
            return;
        };
        let updates = HashMap::from([("learningModel".to_string(), value)]);
        match settings.update(Some(SettingsCategory::Llm), updates).await {
            Ok(_) => info!(
                model_id,
                "Saga: set the Learning model to the first chat model"
            ),
            Err(e) => warn!(error = %e, "Saga: could not set the Learning model"),
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

    /// Records a finished file and, when it was the last one, the model's
    /// completion and its downloaded-model record, in one unit of work.
    /// `None` while other files are still downloading.
    async fn record_file_completed(
        &self,
        event: &FileDownloadCompletedEvent,
    ) -> Result<Option<CompletedModel>, Box<dyn std::error::Error + Send + Sync>> {
        let mut tx = self.database.begin_write().await?;
        let file_size = event.file_size as i64;
        model_file_ops::update_file_status_by_model_and_name(
            &mut tx,
            &event.model_id,
            &event.file_name,
            FileStatus::Completed,
            Some(file_size),
        )
        .await?;

        let total_files = model_file_ops::count_total_files(&mut tx, &event.model_id).await?;
        let completed_files =
            model_file_ops::count_completed_files(&mut tx, &event.model_id).await?;
        info!(
            "Saga: Model {} progress: {}/{} files",
            event.model_id, completed_files, total_files
        );
        if completed_files != total_files {
            tx.commit().await?;
            return Ok(None);
        }
        info!(
            "Saga: All files completed for {}, marking as completed",
            event.model_id
        );

        model_ops::update_status_completed(&mut tx, &event.model_id).await?;
        let model = model_ops::find_by_model_id(&mut tx, &event.model_id)
            .await?
            .ok_or_else(|| format!("Model not found: {}", event.model_id))?;
        let mut files = model_file_ops::find_by_model_id(&mut tx, &event.model_id).await?;
        // Sort files to select a deterministic path for multi-file models.
        files.sort_by(|a, b| a.file_path.cmp(&b.file_path));
        let total_size: u64 = files.iter().map(|f| f.size_bytes as u64).sum();

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

        // The record must exist before the completion event goes out, and it
        // lands with the completion so neither can stand without the other.
        downloaded_models::save_in(&mut tx, &downloaded_model)
            .await
            .map_err(|e| format!("Failed to save DownloadedModel: {}", e))?;
        tx.commit().await?;

        Ok(Some(CompletedModel {
            model,
            files,
            total_size,
            downloaded_model,
        }))
    }

    async fn handle_file_completed(
        &self,
        event: FileDownloadCompletedEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!(
            "Saga: File completed {}/{}",
            event.model_id, event.file_name
        );

        let Some(completed) = self.record_file_completed(&event).await? else {
            return Ok(());
        };
        let CompletedModel {
            model,
            files,
            total_size,
            downloaded_model,
        } = completed;
        info!(
            "Saga: DownloadedModel saved successfully for {}, now emitting completion event",
            event.model_id
        );

        // Activation is its own step: a failure leaves the slot empty or on
        // the previous model, both of which the app handles.
        self.activate_if_slot_empty(&downloaded_model, Self::select_primary_model_file(&files))
            .await;

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

        Ok(())
    }

    /// First-download-wins: only auto-activate when the slot is empty so a
    /// deliberate user assignment is never stolen. The hashing an embedding
    /// model needs runs outside any unit of work, and the slot is checked
    /// again in the write that fills it.
    async fn activate_if_slot_empty(
        &self,
        downloaded_model: &DownloadedModel,
        main_file: Option<&ModelFile>,
    ) {
        let model_id = downloaded_model.model_id();
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
                        model_id = %model_id,
                        "Saga: embedding model has no tokenizer.json beside it; not auto-activated"
                    )
                }
                Ok(None) => {
                    let verified = main_file.and_then(Self::verified_gguf_identity);
                    let activation = async {
                        let identity = match verified {
                            Some(identity) => Some(identity),
                            None => {
                                crate::infrastructure::ml::artifact_identity::establish(
                                    downloaded_model,
                                )
                                .await?
                            }
                        };
                        self.downloaded_model_repo
                            .activate_embedding_if_empty(model_id, identity.as_ref())
                            .await
                    };
                    match activation.await {
                        Ok(true) => info!(
                            model_id = %model_id,
                            "Saga: auto-activated newly-downloaded embedding model (slot was empty)"
                        ),
                        Ok(false) => info!(
                            model_id = %model_id,
                            "Saga: embedding slot was filled meanwhile; not auto-activated"
                        ),
                        Err(e) => warn!(
                            model_id = %model_id,
                            error = %e,
                            "Saga: failed to auto-activate empty embedding slot"
                        ),
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
                Ok(None) => match self
                    .downloaded_model_repo
                    .activate_chat_if_empty(model_id)
                    .await
                {
                    Ok(true) => {
                        info!(
                            model_id = %model_id,
                            "Saga: auto-activated newly-downloaded chat model (slot was empty)"
                        );
                        self.default_learning_model(model_id).await;
                    }
                    Ok(false) => info!(
                        model_id = %model_id,
                        "Saga: chat slot was filled meanwhile; not auto-activated"
                    ),
                    Err(e) => warn!(
                        model_id = %model_id,
                        error = %e,
                        "Saga: failed to auto-activate empty chat slot"
                    ),
                },
                Ok(Some(_)) => {}
                Err(e) => warn!(
                    error = %e,
                    "Saga: failed to read active chat slot for auto-activate"
                ),
            }
        }
    }

    /// Records a failed file and the model's failure in one unit of work,
    /// and returns the model. `None` when the file had already completed.
    async fn record_file_failed(
        &self,
        event: &FileDownloadFailedEvent,
    ) -> Result<Option<Model>, Box<dyn std::error::Error + Send + Sync>> {
        let mut tx = self.database.begin_write().await?;

        // A file already on disk and recorded as complete stays that way: the
        // failure can belong to a different session for the same file.
        let already_complete = model_file_ops::find_by_model_id(&mut tx, &event.model_id)
            .await?
            .iter()
            .any(|file| {
                file.file_name == event.file_name && matches!(file.status, FileStatus::Completed)
            });
        if already_complete {
            tx.rollback().await?;
            return Ok(None);
        }

        model_file_ops::update_file_status_by_model_and_name(
            &mut tx,
            &event.model_id,
            &event.file_name,
            FileStatus::Failed,
            None,
        )
        .await?;
        model_ops::update_status_failed(&mut tx, &event.model_id).await?;
        let model = model_ops::find_by_model_id(&mut tx, &event.model_id)
            .await?
            .ok_or_else(|| format!("Model not found: {}", event.model_id))?;
        tx.commit().await?;
        Ok(Some(model))
    }

    async fn handle_file_failed(
        &self,
        event: FileDownloadFailedEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        error!(
            "Saga: File failed for model {}: {} - {}",
            event.model_id, event.file_name, event.error
        );

        let Some(model) = self.record_file_failed(&event).await? else {
            warn!(
                "Saga: ignoring a failure for {}/{}, which has already completed",
                event.model_id, event.file_name
            );
            return Ok(());
        };

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

/// What the unit of work that completes a model hands to the steps after it.
struct CompletedModel {
    model: Model,
    files: Vec<ModelFile>,
    total_size: u64,
    downloaded_model: DownloadedModel,
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod write_tests;
