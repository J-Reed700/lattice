//! The `DownloadManager` implementation: every operation callers can drive.
//!
//! Start, pause, resume, cancel, retry, delete and the read-only queries all
//! live here; the heavy lifting is delegated to the queue and task modules.

use super::state::{DownloadManagerService, StopReason};
use super::types::{DownloadEvent, DownloadManager, DownloadRequest};
use crate::domain::download::{DownloadError, DownloadSession, DownloadState};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};
use uuid::Uuid;

#[async_trait]
impl DownloadManager for DownloadManagerService {
    async fn start_download(&self, request: DownloadRequest) -> Result<String, DownloadError> {
        if request.destination.as_os_str().is_empty() {
            return Err(DownloadError::InvalidDestination(
                "Destination path cannot be empty".to_string(),
            ));
        }

        if request
            .destination
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(DownloadError::InvalidDestination(
                "Path traversal detected in destination path".to_string(),
            ));
        }

        let cache_root = dirs::cache_dir().map(|p| p.join("lattice"));
        let dot_cache_root = std::env::var("HOME")
            .ok()
            .map(std::path::PathBuf::from)
            .map(|p| p.join(".cache").join("lattice"));
        let within_allowed_root = request.destination.starts_with(&self.allowed_root);
        let within_cache_root = cache_root
            .as_ref()
            .is_some_and(|root| request.destination.starts_with(root));
        let within_dot_cache_root = dot_cache_root
            .as_ref()
            .is_some_and(|root| request.destination.starts_with(root));

        if !within_allowed_root && !within_cache_root && !within_dot_cache_root {
            let mut roots = vec![self.allowed_root.display().to_string()];
            if let Some(root) = &cache_root {
                roots.push(root.display().to_string());
            }
            if let Some(root) = &dot_cache_root {
                roots.push(root.display().to_string());
            }
            return Err(DownloadError::InvalidDestination(format!(
                "Destination path must be within allowed roots: {}",
                roots.join(", ")
            )));
        }

        let (file_size, _final_url) = self.engine.get_file_size(&request.url).await?;
        info!(url = %request.url, file_size = ?file_size, "✓ File size retrieved");

        let id = Uuid::new_v4().to_string();
        let mut session = DownloadSession::new(
            id.clone(),
            request.url.clone(),
            request.destination.clone(),
            file_size,
            request.checksum,
        )?;
        info!(download_id = %id, url = %request.url, "✓ DownloadSession created");

        if let (Some(model_name), Some(model_id)) =
            (request.model_name.clone(), request.model_id.clone())
        {
            session = session.with_model_metadata(model_name, model_id);
            info!(download_id = %id, "✓ Model metadata attached to session");
        }
        if let Some(model_file_name) = request.model_file_name {
            session = session.with_model_file_name(model_file_name);
        }

        info!(download_id = %id, "→ Calling repository.create()...");

        // Give pool time to release connections from prior operations
        tokio::task::yield_now().await;

        self.repository.create(&session).await?;
        info!(download_id = %id, "✓ Download session created in database");

        if let Some(token) = request.auth_token {
            let mut tokens = self.auth_tokens.write().await;
            tokens.insert(id.clone(), token);
            info!(download_id = %id, "✓ Auth token stored");
        }

        {
            let mut queue = self.download_queue.write().await;
            queue.push_back(id.clone());
            info!(download_id = %id, "✓ Added to download queue");
        }

        info!(download_id = %id, "→ Calling process_queue()...");
        self.process_queue().await?;
        info!(download_id = %id, "✓ Queue processed successfully");

        info!(download_id = %id, "✓ start_download() returning ID");
        Ok(id)
    }

    async fn pause_download(&self, id: &str) -> Result<(), DownloadError> {
        let mut session = self
            .repository
            .get(id)
            .await?
            .ok_or_else(|| DownloadError::SessionNotFound(id.to_string()))?;

        session.pause()?;
        self.repository.update(&session).await?;

        // Remove the entry here rather than relying solely on the task's own
        // cleanup. `process_queue` gates on `active.len() >= max_concurrent`,
        // so leaving a paused session occupying a slot stalls the queue: with
        // a limit of 2, pausing two files meant no queued or new download ever
        // started again. Removing here also closes the window between the
        // pause returning and the task getting scheduled.
        let stop_tx = {
            let mut active = self.active_downloads.write().await;
            active
                .remove(id)
                .and_then(|mut download| download.cancel_tx.take())
        };

        if let Some(stop_tx) = stop_tx {
            if let Err(e) = stop_tx.send(StopReason::Pause).await {
                warn!(download_id = %id, error = %e, "Failed to send pause signal to download task");
            }
        }

        // The task may still be mid-flight; make sure its token is gone too.
        {
            let mut tokens = self.auth_tokens.write().await;
            tokens.remove(id);
        }

        if let Err(e) = self
            .event_tx
            .send(DownloadEvent::Paused { id: id.to_string() })
        {
            warn!(download_id = %id, error = %e, "Failed to send Paused event");
        }

        Ok(())
    }

    async fn resume_download(&self, id: &str) -> Result<(), DownloadError> {
        let mut session = self
            .repository
            .get(id)
            .await?
            .ok_or_else(|| DownloadError::SessionNotFound(id.to_string()))?;

        session.resume()?;
        self.repository.update(&session).await?;

        {
            let mut queue = self.download_queue.write().await;
            queue.push_back(id.to_string());
        }

        self.process_queue().await?;

        if let Err(e) = self
            .event_tx
            .send(DownloadEvent::Resumed { id: id.to_string() })
        {
            warn!(download_id = %id, error = %e, "Failed to send Resumed event");
        }

        Ok(())
    }

    async fn cancel_download(&self, id: &str) -> Result<(), DownloadError> {
        let mut session = self
            .repository
            .get(id)
            .await?
            .ok_or_else(|| DownloadError::SessionNotFound(id.to_string()))?;

        session.cancel()?;
        self.repository.update(&session).await?;

        let mut active = self.active_downloads.write().await;
        if let Some(download) = active.remove(id) {
            if let Some(cancel_tx) = download.cancel_tx {
                if let Err(e) = cancel_tx.send(StopReason::Cancel).await {
                    warn!(download_id = %id, error = %e, "Failed to send cancel signal to download task");
                }
            }
        }

        if let Err(e) = self
            .event_tx
            .send(DownloadEvent::Cancelled { id: id.to_string() })
        {
            warn!(download_id = %id, error = %e, "Failed to send Cancelled event");
        }

        Ok(())
    }

    async fn get_download_status(
        &self,
        id: &str,
    ) -> Result<Option<DownloadSession>, DownloadError> {
        self.repository.get(id).await
    }

    async fn list_downloads(&self) -> Result<Vec<DownloadSession>, DownloadError> {
        self.repository.list().await
    }

    async fn list_active_downloads(&self) -> Result<Vec<DownloadSession>, DownloadError> {
        self.repository.list_active().await
    }

    async fn retry_download(&self, id: &str) -> Result<(), DownloadError> {
        let mut session = self
            .repository
            .get(id)
            .await?
            .ok_or_else(|| DownloadError::SessionNotFound(id.to_string()))?;

        let destination = session.destination();
        if destination.exists() {
            self.file_cleanup.delete_file_best_effort(destination).await;
        }

        // Manual retry resets retry counter to allow user override of automatic retry limits
        session.manual_retry()?;
        self.repository.update(&session).await?;

        // Queue for download (reuse existing queue logic)
        {
            let mut queue = self.download_queue.write().await;
            queue.push_back(id.to_string());
        }

        self.process_queue().await?;

        if let Err(e) = self
            .event_tx
            .send(DownloadEvent::Resumed { id: id.to_string() })
        {
            warn!(download_id = %id, error = %e, "Failed to send Resumed event for retry");
        }

        Ok(())
    }

    async fn delete_download(&self, id: &str) -> Result<(), DownloadError> {
        let session = self.repository.get(id).await?;

        if let Some(session) = session {
            // If download is active, cancel it first.
            // If it disappears between lookups, treat as already deleted.
            if !session.state().is_terminal() {
                match self.cancel_download(id).await {
                    Ok(_) => {}
                    Err(DownloadError::SessionNotFound(_)) => {
                        debug!(download_id = %id, "Download disappeared during delete; treating as removed");
                    }
                    Err(e) => return Err(e),
                }
            }

            self.repository.delete(id).await?;
        } else {
            // Idempotent behavior: removing an already-missing session is a success.
            debug!(download_id = %id, "Download not found during delete; treating as already removed");
        }

        {
            let mut active = self.active_downloads.write().await;
            active.remove(id);
        }

        {
            let mut tokens = self.auth_tokens.write().await;
            tokens.remove(id);
        }

        {
            let mut queue = self.download_queue.write().await;
            queue.retain(|download_id| download_id != id);
        }

        Ok(())
    }

    async fn clear_completed_downloads(&self) -> Result<usize, DownloadError> {
        let sessions = self.repository.list().await?;
        let mut deleted_count = 0;

        for session in sessions {
            if session.state() == &DownloadState::Completed {
                self.delete_download(session.id()).await?;
                deleted_count += 1;
            }
        }

        Ok(deleted_count)
    }

    async fn delete_pending_by_model(&self, model_id: &str) -> Result<u64, DownloadError> {
        self.repository.delete_pending_by_model(model_id).await
    }

    async fn process_pending_queue(&self) -> Result<(), DownloadError> {
        self.process_queue().await
    }

    /// Subscribe to download events.
    ///
    /// Returns the shared receiver Arc. Caller is responsible for managing
    /// event forwarding in their own async context.
    ///
    /// # Returns
    /// Arc<RwLock<Option<UnboundedReceiver<DownloadEvent>>>> - Shared receiver for download events
    fn subscribe_to_events(&self) -> Arc<RwLock<Option<mpsc::UnboundedReceiver<DownloadEvent>>>> {
        Arc::clone(&self.event_rx)
    }
}
