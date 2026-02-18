use crate::domain::value_objects::model_status::{ModelStatus, FileStatus};
use crate::domain::events::model_download_events::*;
use crate::infrastructure::event_bus::EventBus;
use crate::infrastructure::persistence::repositories::model_file_repository::ModelFileRepository;
use crate::infrastructure::persistence::repositories::model_repository::ModelRepository;
use std::sync::Arc;
use tracing::{error, info, warn};

pub struct DatabaseHandler {
    event_bus: Arc<EventBus>,
    model_repo: Arc<dyn ModelRepository>,
    model_file_repo: Arc<ModelFileRepository>,
}

impl DatabaseHandler {
    pub fn new(
        event_bus: Arc<EventBus>,
        model_repo: Arc<dyn ModelRepository>,
        model_file_repo: Arc<ModelFileRepository>,
    ) -> Self {
        Self {
            event_bus,
            model_repo,
            model_file_repo,
        }
    }

    pub async fn start(&self) {
        let mut receiver = self.event_bus.subscribe();

        loop {
            match receiver.recv().await {
                Ok(event) => {
                    if let Err(e) = self.handle_event(event).await {
                        error!("DatabaseHandler error: {}", e);
                    }
                }
                Err(e) => {
                    warn!("DatabaseHandler receiver error: {}", e);
                }
            }
        }
    }

    async fn handle_event(
        &self,
        event: ModelDownloadEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match event {
            ModelDownloadEvent::ModelDownloadStarted(e) => {
                info!("DB: Model download started: {}", e.model_id);
                self.model_repo
                    .update_model_status(&e.model_id, ModelStatus::Downloading)
                    .await?;
            }
            ModelDownloadEvent::FileDownloadStarted(e) => {
                info!("DB: File download started: {} - {}", e.model_id, e.file_name);
                self.model_file_repo
                    .update_file_status(&e.file_id, FileStatus::Downloading)
                    .await?;
            }
            // NOTE: FileDownloadCompleted and FileDownloadFailed are now handled
            // exclusively by DownloadSaga to prevent race conditions.
            // Saga owns file status updates within atomic transactions.
            ModelDownloadEvent::FileDownloadCompleted(_) => {
                // Audit only - Saga handles the actual status update
            }
            ModelDownloadEvent::FileDownloadFailed(_) => {
                // Audit only - Saga handles the actual status update
            }
            ModelDownloadEvent::ModelDownloadCompleted(_) => {
                info!("DB: Model download completed (audit only)");
            }
            ModelDownloadEvent::ModelDownloadFailed(_) => {
                error!("DB: Model download failed (audit only)");
            }
            _ => {}
        }
        Ok(())
    }
}
