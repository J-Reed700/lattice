//! The `DownloadManager` implementation: every operation callers can drive.
//!
//! Start, pause, resume, cancel, retry, delete and the read-only queries all
//! live here; the heavy lifting is delegated to the queue and task modules.

use super::state::{DownloadManagerService, StopReason};
use super::task::{send_control_event, send_terminal_event};
use super::types::{DownloadBatchItem, DownloadEvent, DownloadManager, DownloadRequest};
use crate::domain::download::{DownloadError, DownloadSession, DownloadState};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info};
use uuid::Uuid;

#[async_trait]
impl DownloadManager for DownloadManagerService {
    async fn start_download(&self, request: DownloadRequest) -> Result<String, DownloadError> {
        let _queue_guard = self.queue_gate.lock().await;
        let id = self.enqueue_download(request).await?;

        info!(download_id = %id, "→ Calling process_queue()...");
        self.process_queue().await?;
        info!(download_id = %id, "✓ Queue processed successfully");

        info!(download_id = %id, "✓ start_download() returning ID");
        Ok(id)
    }

    async fn start_download_batch(
        &self,
        items: Vec<DownloadBatchItem>,
    ) -> Result<Vec<String>, DownloadError> {
        let _queue_guard = self.queue_gate.lock().await;
        let mut ids = Vec::with_capacity(items.len());

        for item in items {
            let mut last_error = None;
            let mut registered_id = None;

            let candidate_count = item.requests.len();
            for (candidate_index, request) in item.requests.into_iter().enumerate() {
                match self.enqueue_download(request).await {
                    Ok(id) => {
                        registered_id = Some(id);
                        break;
                    }
                    Err(error) => {
                        let can_try_fallback = candidate_index + 1 < candidate_count
                            && matches!(error, DownloadError::HttpError { status: 404, .. });
                        last_error = Some(error);
                        if !can_try_fallback {
                            break;
                        }
                    }
                }
            }

            match registered_id {
                Some(id) => ids.push(id),
                None => {
                    for id in &ids {
                        self.repository.delete(id).await?;
                    }
                    {
                        let mut queue = self.download_queue.write().await;
                        queue.retain(|id| !ids.contains(id));
                    }
                    {
                        let mut tokens = self.auth_tokens.write().await;
                        tokens.retain(|id, _| !ids.contains(id));
                    }
                    return Err(last_error.unwrap_or_else(|| {
                        DownloadError::NetworkError(
                            "Download batch item has no candidate URLs".to_string(),
                        )
                    }));
                }
            }
        }

        self.process_queue().await?;
        Ok(ids)
    }

    async fn enqueue_download(&self, request: DownloadRequest) -> Result<String, DownloadError> {
        if self.shutdown.is_cancelled() {
            return Err(DownloadError::NetworkError(
                "Application is shutting down".into(),
            ));
        }
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

        self.supersede_sessions_for(&request.destination).await?;

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

        info!(download_id = %id, "✓ enqueue_download() returning ID");
        Ok(id)
    }

    async fn pause_download(&self, id: &str) -> Result<(), DownloadError> {
        let _queue_guard = self.queue_gate.lock().await;
        self.stop_transfer(id, StopReason::Pause).await;
        let mut session = self
            .repository
            .get(id)
            .await?
            .ok_or_else(|| DownloadError::SessionNotFound(id.to_string()))?;
        if session.state().is_terminal() {
            return Ok(());
        }
        session.pause()?;
        self.repository.update(&session).await?;
        self.auth_tokens.write().await.remove(id);
        drop(_queue_guard);
        send_control_event(
            &self.event_tx,
            &self.shutdown,
            DownloadEvent::Paused { id: id.to_string() },
        )
        .await;
        Ok(())
    }

    async fn resume_download(&self, id: &str) -> Result<(), DownloadError> {
        let _queue_guard = self.queue_gate.lock().await;
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

        drop(_queue_guard);
        send_control_event(
            &self.event_tx,
            &self.shutdown,
            DownloadEvent::Resumed { id: id.to_string() },
        )
        .await;

        Ok(())
    }

    async fn cancel_download(&self, id: &str) -> Result<(), DownloadError> {
        let _queue_guard = self.queue_gate.lock().await;
        self.stop_transfer(id, StopReason::Cancel).await;
        let mut session = self
            .repository
            .get(id)
            .await?
            .ok_or_else(|| DownloadError::SessionNotFound(id.to_string()))?;
        if session.state().is_terminal() {
            return Ok(());
        }
        session.cancel()?;
        self.repository.update(&session).await?;
        self.auth_tokens.write().await.remove(id);
        self.download_queue
            .write()
            .await
            .retain(|queued| queued != id);
        drop(_queue_guard);
        send_terminal_event(
            &self.event_tx,
            &self.shutdown,
            DownloadEvent::Cancelled { id: id.to_string() },
        )
        .await;
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
        let _queue_guard = self.queue_gate.lock().await;
        let mut session = self
            .repository
            .get(id)
            .await?
            .ok_or_else(|| DownloadError::SessionNotFound(id.to_string()))?;

        // Validate before touching the file: retrying a running session must
        // not delete the file underneath its active transfer.
        session.manual_retry()?;
        let destination = session.destination();
        if destination.exists() {
            self.file_cleanup.delete_file_best_effort(destination).await;
        }

        self.repository.update(&session).await?;

        // Queue for download (reuse existing queue logic)
        {
            let mut queue = self.download_queue.write().await;
            queue.push_back(id.to_string());
        }

        self.process_queue().await?;

        drop(_queue_guard);
        send_control_event(
            &self.event_tx,
            &self.shutdown,
            DownloadEvent::Resumed { id: id.to_string() },
        )
        .await;

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
        let _queue_guard = self.queue_gate.lock().await;
        self.process_queue().await
    }

    /// Subscribe to download events.
    ///
    /// Returns the shared receiver Arc. Caller is responsible for managing
    /// event forwarding in their own async context.
    ///
    /// # Returns
    /// Arc<RwLock<Option<Receiver<DownloadEvent>>>> - Shared receiver for download events
    fn subscribe_to_events(&self) -> Arc<RwLock<Option<mpsc::Receiver<DownloadEvent>>>> {
        Arc::clone(&self.event_rx)
    }
}

impl DownloadManagerService {
    /// Stop and join before another transfer may use this session or its slot.
    /// The task flushes its last progress before the caller changes state.
    async fn stop_transfer(&self, id: &str, reason: StopReason) {
        let download = self.active_downloads.write().await.remove(id);
        if let Some(mut download) = download {
            if let Some(tx) = download.cancel_tx.take() {
                let _ = tx.send(reason).await;
            }
            if let Some(task) = download.task_handle.take() {
                let _ = task.await;
            }
        }
    }

    /// A destination holds one file, so an earlier session for it that this
    /// manager is neither running nor holding in its queue is history: a
    /// finished or failed attempt, or one a previous launch left behind. Left
    /// in place it is reported beside the new session as a second row for the
    /// same file, and that row never moves. The partial file stays where it is;
    /// the new session resumes from its length.
    async fn supersede_sessions_for(
        &self,
        destination: &std::path::Path,
    ) -> Result<(), DownloadError> {
        let earlier: Vec<String> = self
            .repository
            .list()
            .await?
            .into_iter()
            .filter(|session| session.destination() == destination)
            .map(|session| session.id().to_string())
            .collect();
        for id in earlier {
            let running = self.active_downloads.read().await.contains_key(&id);
            let queued = self.download_queue.read().await.contains(&id);
            if running || queued {
                continue;
            }
            self.repository.delete(&id).await?;
            self.auth_tokens.write().await.remove(&id);
            info!(download_id = %id, "Superseded an earlier session for the same destination");
        }
        Ok(())
    }
}
