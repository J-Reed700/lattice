//! The `DownloadManager` implementation: every operation callers can drive.
//!
//! Start, pause, resume, cancel, retry, delete and the read-only queries all
//! live here; the heavy lifting is delegated to the queue and task modules.

use super::state::{DownloadManagerService, StopReason};
use super::task::{send_control_event, send_terminal_event};
use crate::infrastructure::adapters::fs::model_paths::ModelPaths;
use async_trait::async_trait;
use lattice_core::application::ports::download_manager::{
    is_huggingface_url, DownloadBatchItem, DownloadEvent, DownloadManager, DownloadRequest,
};
use lattice_core::application::services::credentials::CredentialKey;
use lattice_core::domain::download::{DownloadError, DownloadSession, DownloadState};
use lattice_core::shared::path_safety::reject_parent_components;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};
use uuid::Uuid;

#[async_trait]
impl DownloadManager for DownloadManagerService {
    async fn start_download(&self, request: DownloadRequest) -> Result<String, DownloadError> {
        // Sizing the file is network I/O: it happens before the gate, so a
        // slow server cannot stall pause, cancel, resume or queue promotion.
        let prepared = self.prepare_download(request).await?;
        let _queue_guard = self.queue_gate.lock().await;
        let id = self
            .register_downloads(vec![prepared])
            .await?
            .pop()
            .ok_or_else(|| DownloadError::NetworkError("No download was registered".to_string()))?;

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
        let mut prepared = Vec::with_capacity(items.len());

        for item in items {
            let mut last_error = None;
            let mut ready = None;

            let candidate_count = item.requests.len();
            for (candidate_index, mut request) in item.requests.into_iter().enumerate() {
                // The 404 fallback below keys on the prepare-time HEAD, so a
                // request with alternatives must be sized by one.
                if candidate_count > 1 {
                    request.expected_bytes = None;
                }
                match self.prepare_download(request).await {
                    Ok(download) => {
                        ready = Some(download);
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

            match ready {
                Some(download) => prepared.push(download),
                None => {
                    return Err(last_error.unwrap_or_else(|| {
                        DownloadError::NetworkError(
                            "Download batch item has no candidate URLs".to_string(),
                        )
                    }));
                }
            }
        }

        // Every item is prepared (and sized) before the gate is taken; the
        // gate then covers only registration and promotion, so the batch
        // still becomes observable all at once.
        let _queue_guard = self.queue_gate.lock().await;
        let ids = self.register_downloads(prepared).await?;
        self.process_queue().await?;
        Ok(ids)
    }

    async fn enqueue_download(&self, request: DownloadRequest) -> Result<String, DownloadError> {
        let prepared = self.prepare_download(request).await?;
        self.register_downloads(vec![prepared])
            .await?
            .pop()
            .ok_or_else(|| DownloadError::NetworkError("No download was registered".to_string()))
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
        self.download_queue
            .write()
            .await
            .retain(|queued| queued != id);
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
        self.refuse_if_destination_live(&session).await?;
        let partial_bytes = self.valid_partial_bytes(&session).await;
        if partial_bytes != session.progress().bytes_downloaded() {
            session.update_progress(partial_bytes, 0.0);
        }
        self.repository.update(&session).await?;

        if let Some(token) = self.resolve_auth_token(session.url(), None).await {
            self.auth_tokens.write().await.insert(id.to_string(), token);
        }

        {
            let mut queue = self.download_queue.write().await;
            if !queue.iter().any(|queued| queued == id) {
                queue.push_back(id.to_string());
            }
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
        self.discard_working_file(&session).await;
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
        self.refuse_if_destination_live(&session).await?;
        // Preserve valid `.part` files even when `checksum` is `None` (e.g.,
        // custom HuggingFace GGUFs): `HttpDownloadEngine` uses HTTP Range
        // (`206 Partial Content` vs `200 OK` reset) and validates final length.
        let partial_bytes = self.valid_partial_bytes(&session).await;
        session.update_progress(partial_bytes, 0.0);

        self.repository.update(&session).await?;

        if let Some(token) = self.resolve_auth_token(session.url(), None).await {
            self.auth_tokens.write().await.insert(id.to_string(), token);
        }

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
        let mut queue_guard = None;

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

            // From here the gate is held to the end: a start of the same file
            // must not register and begin resuming this `.part` between the
            // discard and the row delete, then lose it.
            queue_guard = Some(self.queue_gate.lock().await);

            // A Failed (or Cancelled) row no longer owns anything but its
            // `.part`; a Completed one owns the finished file, which stays.
            if matches!(
                session.state(),
                DownloadState::Failed | DownloadState::Cancelled
            ) {
                self.discard_working_file(&session).await;
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

        drop(queue_guard);
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
        let _queue_guard = self.queue_gate.lock().await;
        let pending_ids: Vec<String> = self
            .repository
            .list_by_state(&DownloadState::Pending)
            .await?
            .into_iter()
            .filter(|session| session.model_id() == Some(model_id))
            .map(|session| session.id().to_string())
            .collect();
        let deleted = self.repository.delete_pending_by_model(model_id).await?;
        if !pending_ids.is_empty() {
            {
                let mut tokens = self.auth_tokens.write().await;
                for id in &pending_ids {
                    tokens.remove(id);
                }
            }
            {
                let mut queue = self.download_queue.write().await;
                queue.retain(|queued| !pending_ids.iter().any(|id| id == queued));
            }
        }
        Ok(deleted)
    }

    async fn process_pending_queue(&self) -> Result<(), DownloadError> {
        let _queue_guard = self.queue_gate.lock().await;
        self.process_queue().await
    }

    /// Subscribe to download events.
    ///
    /// Returns the shared receiver Arc. Caller is responsible for managing
    /// event forwarding in their own async context.
    fn subscribe_to_events(&self) -> Arc<RwLock<Option<mpsc::Receiver<DownloadEvent>>>> {
        Arc::clone(&self.event_rx)
    }
}

/// A download checked and sized, ready to save.
struct PreparedDownload {
    session: DownloadSession,
    auth_token: Option<String>,
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

    /// Checks a request and sizes the file, building the session without saving it.
    async fn prepare_download(
        &self,
        request: DownloadRequest,
    ) -> Result<PreparedDownload, DownloadError> {
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

        reject_parent_components(&request.destination).map_err(|reason| {
            DownloadError::InvalidDestination(format!("Destination path {reason}"))
        })?;

        let cache_root = dirs::cache_dir().map(|p| p.join("lattice"));
        let dot_cache_root = ModelPaths::cache_root().ok();
        // Model files arrive confined, so canonicalized (on Windows with the
        // `\\?\` prefix), while the roots are plain paths: each root is matched
        // in both forms, or no model could ever download on Windows.
        let under = |root: &std::path::Path| {
            request.destination.starts_with(root)
                || root
                    .canonicalize()
                    .is_ok_and(|canonical| request.destination.starts_with(canonical))
        };
        let within_allowed_root = under(self.allowed_root.as_path());
        let within_cache_root = cache_root.as_deref().is_some_and(under);
        let within_dot_cache_root = dot_cache_root.as_deref().is_some_and(under);

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

        let auth_token = self
            .resolve_auth_token(&request.url, request.auth_token)
            .await;

        let file_size = match request.expected_bytes {
            Some(bytes) => Some(bytes),
            None => {
                let (file_size, _final_url) = self
                    .engine
                    .get_file_size(&request.url, auth_token.as_deref())
                    .await
                    .inspect_err(|error| {
                        // The user sees a plain sentence; keep the cause.
                        warn!(url = %request.url, %error, "Could not size the download");
                    })?;
                info!(url = %request.url, file_size = ?file_size, "✓ File size retrieved");
                file_size
            }
        };

        let id = Uuid::new_v4().to_string();
        let mut session = DownloadSession::new(
            id.clone(),
            request.url.clone(),
            request.destination.clone(),
            file_size,
            request.checksum,
        )?;
        info!(download_id = %id, url = %request.url, "✓ DownloadSession created");

        match (request.model_name.clone(), request.model_id.clone()) {
            (Some(model_name), Some(model_id)) => {
                session = session.with_model_metadata(model_name, model_id);
                info!(download_id = %id, "✓ Model metadata attached to session");
            }
            // A name needs no model id to be shown: a vision projector or a
            // reranker belongs to no model record.
            (Some(model_name), None) => session = session.with_model_name(model_name),
            _ => {}
        }
        if let Some(model_file_name) = request.model_file_name {
            session = session.with_model_file_name(model_file_name);
        }

        Ok(PreparedDownload {
            session,
            auth_token,
        })
    }

    /// Saves prepared downloads, and deletes the sessions they supersede, in one
    /// unit of work; then queues them in order.
    ///
    /// A destination holds one file, so an item whose destination already has a
    /// live session (running or queued), or repeats an earlier item of the same
    /// batch, is not registered: its slot in the returned ids is that session's
    /// id. Callers hold `queue_gate`.
    async fn register_downloads(
        &self,
        prepared: Vec<PreparedDownload>,
    ) -> Result<Vec<String>, DownloadError> {
        let existing = self.repository.list().await?;
        let mut superseded = Vec::new();
        let mut ids = Vec::with_capacity(prepared.len());
        let mut registering: Vec<PreparedDownload> = Vec::new();
        for download in prepared {
            let destination = download.session.destination();
            if let Some(twin) = registering
                .iter()
                .find(|earlier| earlier.session.destination() == destination)
            {
                ids.push(twin.session.id().to_string());
                continue;
            }
            let history = self.destination_history(destination, None, &existing).await;
            if let Some(live) = history.live {
                info!(
                    download_id = %live,
                    "A download for this destination is already live; not registering another"
                );
                ids.push(live);
                continue;
            }
            superseded.extend(history.superseded);
            ids.push(download.session.id().to_string());
            registering.push(download);
        }
        if registering.is_empty() {
            return Ok(ids);
        }
        for download in &mut registering {
            // Preserve valid `.part` files across re-downloads even without a
            // checksum; discard empty or oversized `.part` files so they start clean.
            let partial_bytes = self.valid_partial_bytes(&download.session).await;
            if partial_bytes > 0 {
                download.session.update_progress(partial_bytes, 0.0);
            }
        }

        let sessions: Vec<DownloadSession> = registering
            .iter()
            .map(|download| download.session.clone())
            .collect();

        // Give pool time to release connections from prior operations
        tokio::task::yield_now().await;

        self.repository.register(&superseded, &sessions).await?;
        for id in &superseded {
            self.auth_tokens.write().await.remove(id);
            info!(download_id = %id, "Superseded an earlier session for the same destination");
        }

        for download in registering {
            let id = download.session.id().to_string();
            if let Some(token) = download.auth_token {
                self.auth_tokens.write().await.insert(id.clone(), token);
            }
            self.download_queue.write().await.push_back(id.clone());
            info!(download_id = %id, "✓ Added to download queue");
        }
        Ok(ids)
    }

    /// What the sessions in `sessions` say about one destination, apart from
    /// `except`: the one this manager is running or holding in its queue, if
    /// any, and the rest, which are history. History is a finished or failed
    /// attempt, or one a previous launch left behind; left in place it is
    /// reported beside a new session as a second row for the same file, and
    /// that row never moves. A live session is never superseded: two tasks
    /// would write one file. Superseding keeps the `.part`, so the new session
    /// resumes the old one's bytes.
    async fn destination_history(
        &self,
        destination: &std::path::Path,
        except: Option<&str>,
        sessions: &[DownloadSession],
    ) -> DestinationHistory {
        let mut history = DestinationHistory::default();
        for session in sessions {
            if session.destination() != destination || Some(session.id()) == except {
                continue;
            }
            // A finished task lingers in the active table for a moment after
            // its row is written: only a non-terminal session is live.
            if session.state().is_terminal() {
                history.superseded.push(session.id().to_string());
                continue;
            }
            let id = session.id().to_string();
            let running = self.active_downloads.read().await.contains_key(&id);
            let queued = self.download_queue.read().await.contains(&id);
            if running || queued {
                history.live.get_or_insert(id);
            } else {
                history.superseded.push(id);
            }
        }
        history
    }

    /// Refuses to bring a session back to life while another session for its
    /// destination is running or queued. Callers hold `queue_gate`.
    async fn refuse_if_destination_live(
        &self,
        session: &DownloadSession,
    ) -> Result<(), DownloadError> {
        let sessions = self.repository.list().await?;
        let history = self
            .destination_history(session.destination(), Some(session.id()), &sessions)
            .await;
        match history.live {
            Some(_) => Err(DownloadError::DestinationBusy(
                session.destination().display().to_string(),
            )),
            None => Ok(()),
        }
    }

    /// Deletes a session's `.part` file, unless another live session for the
    /// same destination is writing it.
    async fn discard_working_file(&self, session: &DownloadSession) {
        if let Ok(sessions) = self.repository.list().await {
            let history = self
                .destination_history(session.destination(), Some(session.id()), &sessions)
                .await;
            if history.live.is_some() {
                return;
            }
        }
        self.file_cleanup
            .delete_file_best_effort(&session.working_path())
            .await;
    }

    /// Returns the existing `.part` file length if it is valid for resuming
    /// (`0 < len <= total_bytes`, or `len > 0` when `total_bytes` is unknown).
    /// Empty or oversized `.part` files are removed so the transfer starts clean.
    async fn valid_partial_bytes(&self, session: &DownloadSession) -> u64 {
        let part = session.working_path();
        match tokio::fs::metadata(&part).await {
            Ok(meta) if meta.is_file() => {
                let len = meta.len();
                let oversized = session
                    .progress()
                    .total_bytes()
                    .is_some_and(|total| len > total);
                if len == 0 || oversized {
                    self.file_cleanup.delete_file_best_effort(&part).await;
                    0
                } else {
                    len
                }
            }
            _ => 0,
        }
    }

    /// Returns `explicit` when provided, or reloads the stored HuggingFace
    /// token from `CredentialsPort` for HuggingFace hosts so pause/resume,
    /// retry, and restarts authenticate without persisting tokens to SQLite.
    pub(super) async fn resolve_auth_token(
        &self,
        url: &str,
        explicit: Option<String>,
    ) -> Option<String> {
        if let Some(token) = explicit.filter(|token| !token.trim().is_empty()) {
            return Some(token);
        }
        if !is_huggingface_url(url) {
            return None;
        }
        let credentials = self.credentials.as_ref()?;
        match credentials
            .get_api_key(CredentialKey::HuggingFaceToken.as_str())
            .await
        {
            Ok(Some(token)) if !token.trim().is_empty() => Some(token),
            Ok(_) => None,
            Err(error) => {
                warn!(%error, "Failed to read HuggingFace token from credentials store");
                None
            }
        }
    }
}

#[derive(Default)]
struct DestinationHistory {
    live: Option<String>,
    superseded: Vec<String>,
}
