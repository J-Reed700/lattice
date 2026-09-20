use crate::domain::download_snapshot::{BatchSnapshot, DownloadStateSnapshot, SingleFileSnapshot};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, RwLock};
use tracing::Instrument;

/// Download event with state snapshot payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadEvent {
    #[serde(flatten)]
    pub snapshot: DownloadStateSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadFailedEvent {
    pub id: String,
    pub error: String,
}

impl DownloadEvent {
    /// Create event from single-file snapshot
    pub fn from_single(snapshot: SingleFileSnapshot) -> Self {
        Self {
            snapshot: DownloadStateSnapshot::Single(snapshot),
        }
    }

    /// Create event from batch snapshot
    pub fn from_batch(snapshot: BatchSnapshot) -> Self {
        Self {
            snapshot: DownloadStateSnapshot::Batch(snapshot),
        }
    }
}

pub struct DownloadEventEmitter {
    app_handle: AppHandle,
}

impl DownloadEventEmitter {
    pub fn new(app_handle: AppHandle) -> Self {
        Self { app_handle }
    }

    pub async fn emit(&self, event: DownloadEvent) -> Result<(), String> {
        self.app_handle
            .emit("download:progress", event)
            .map_err(|e| format!("Failed to emit event: {}", e))
    }

    pub async fn emit_batch(&self, events: Vec<DownloadEvent>) -> Result<(), String> {
        for event in events {
            self.emit(event).await?;
        }
        Ok(())
    }

    pub async fn emit_failed(&self, event: DownloadFailedEvent) -> Result<(), String> {
        self.app_handle
            .emit("download:failed", event)
            .map_err(|e| format!("Failed to emit failed event: {}", e))
    }
}

pub struct DownloadEventBridge {
    emitter: Arc<DownloadEventEmitter>,
    download_manager: Arc<dyn crate::features::download::manager::DownloadManager>,
    repository: Arc<dyn crate::features::download::download_repository::DownloadRepository>,
    event_rx_arc: Arc<
        RwLock<Option<mpsc::UnboundedReceiver<crate::features::download::manager::DownloadEvent>>>,
    >,
    // reason: holds the repository handle supplied by DI wiring in
    // infrastructure/setup/app.rs; dropping it would change the public `new` signature.
    #[allow(dead_code)]
    downloaded_model_repository: Option<
        Arc<crate::features::download::downloaded_model_repository::DownloadedModelRepository>,
    >,
    event_bus: Option<
        Arc<
            crate::infrastructure::event_bus::EventBus<
                crate::features::download::events::model_download_events::ModelDownloadEvent,
            >,
        >,
    >,
}

impl DownloadEventBridge {
    pub fn new(
        app_handle: AppHandle,
        download_manager: Arc<dyn crate::features::download::manager::DownloadManager>,
        repository: Arc<dyn crate::features::download::download_repository::DownloadRepository>,
        event_rx_arc: Arc<
            RwLock<
                Option<mpsc::UnboundedReceiver<crate::features::download::manager::DownloadEvent>>,
            >,
        >,
        downloaded_model_repository: Option<
            Arc<crate::features::download::downloaded_model_repository::DownloadedModelRepository>,
        >,
        event_bus: Option<
            Arc<
                crate::infrastructure::event_bus::EventBus<
                    crate::features::download::events::model_download_events::ModelDownloadEvent,
                >,
            >,
        >,
    ) -> Self {
        let emitter = Arc::new(DownloadEventEmitter::new(app_handle));

        Self {
            emitter,
            download_manager,
            repository,
            event_rx_arc,
            downloaded_model_repository,
            event_bus,
        }
    }

    pub async fn start(self, cancel: tokio_util::sync::CancellationToken) {
        // Take ownership of the receiver from the Arc<RwLock<Option<...>>>
        let mut event_rx = {
            let mut rx_opt = self.event_rx_arc.write().await;
            match rx_opt.take() {
                Some(rx) => rx,
                None => {
                    tracing::warn!("Download event receiver already taken, bridge exiting");
                    return;
                }
            }
        };

        // Subscribe to domain events from EventBus if available.
        let mut domain_subscriber = self.event_bus.as_ref().map(|bus| bus.subscribe());

        // Run two concurrent event loops:
        // STREAM A: Infrastructure events from DownloadManager (mpsc).
        // STREAM B: Domain events from EventBus (broadcast).
        // Why this is structured carefully:
        // The previous version used `Some(Ok(domain_event)) = async { ... }`
        // as the select! pattern. That had two latent bugs:
        //  1. If `domain_subscriber` was None (no event_bus configured),
        //     the async block resolved to None, the pattern failed, the
        //     `else` arm fired and silently killed the entire bridge —
        //     including STREAM A.
        //  2. If recv() returned `Err(RecvError::Lagged)` (slow consumer
        //     dropped events), the pattern Some(Ok(_)) failed, again
        //     hitting the else arm and killing the bridge forever. UI
        //     would stop receiving download updates after the first
        //     burst that overflowed the broadcast channel.
        // Fix: STREAM B's async block always yields a `Result<T, RecvError>`
        // (or pends forever when there's no subscriber). The arm body
        // matches the Result explicitly. The select! pattern itself
        // cannot fail-and-break-the-bridge.
        loop {
            tokio::select! {
                biased;
                // Finish any in-flight handler before observing shutdown here.
                _ = cancel.cancelled() => break,
                // STREAM A: file-level mpsc events from DownloadManager.
                maybe_event = event_rx.recv() => {
                    match maybe_event {
                        Some(manager_event) => {
                            self.handle_infrastructure_event(manager_event).await;
                        }
                        None => {
                            // mpsc Sender side has dropped — the manager
                            // is gone, nothing more will arrive on STREAM A.
                            // Domain stream may still be live, but in
                            // practice the app shuts down at this point.
                            tracing::info!(
                                "Download manager event stream closed; bridge exiting"
                            );
                            break;
                        }
                    }
                }

                // STREAM B: broadcast domain events. The async block always
                // resolves to a Result (or blocks forever when no subscriber).
                domain_recv = async {
                    match &mut domain_subscriber {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    match domain_recv {
                        Ok(envelope) => {
                            // Instrument under the publish-time span so
                            // any tracing inside handle_domain_event
                            // (and the Tauri emit it triggers) joins
                            // the originating trace.
                            let span = envelope.span.clone();
                            let payload = envelope.payload;
                            async {
                                self.handle_domain_event(payload).await;
                            }
                            .instrument(span)
                            .await;
                        }
                        // Slow consumer dropped events. Critical events
                        // like ModelDownloadCompleted may be among them —
                        // log loudly. Tracked as a P1 follow-up: split
                        // high-volume Progress events off this bus so
                        // critical state events never get evicted.
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(
                                dropped = n,
                                "DownloadEventBridge lagged behind producer on domain bus; \
                                 events dropped — frontend may miss state transitions"
                            );
                        }
                        // Domain bus closed — STREAM B is gone for good
                        // but STREAM A may still have work. Stop polling
                        // STREAM B by clearing the subscriber; the async
                        // block will then `pending()` forever instead.
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            tracing::info!(
                                "Domain event bus closed; bridge will continue forwarding \
                                 manager events only"
                            );
                            domain_subscriber = None;
                        }
                    }
                }
            }
        }

        tracing::info!("Download event bridge stopped");
    }

    /// Handle infrastructure events from DownloadManager (file-level events)
    /// Converts to snapshot and emits
    async fn handle_infrastructure_event(
        &self,
        manager_event: crate::features::download::manager::DownloadEvent,
    ) {
        let is_terminal_event = matches!(
            manager_event,
            crate::features::download::manager::DownloadEvent::Completed { .. }
                | crate::features::download::manager::DownloadEvent::Failed { .. }
                | crate::features::download::manager::DownloadEvent::Cancelled { .. }
        );

        // Bridge manager events into domain download events so DownloadSaga can
        // update model_files/model status atomically.
        if let Err(e) = self.publish_domain_event(&manager_event).await {
            tracing::warn!("Failed to publish domain download event: {}", e);
        }

        if let crate::features::download::manager::DownloadEvent::Failed { id, error } =
            &manager_event
        {
            if let Err(e) = self
                .emitter
                .emit_failed(DownloadFailedEvent {
                    id: id.clone(),
                    error: error.clone(),
                })
                .await
            {
                tracing::error!("Failed to emit download:failed event: {}", e);
            }
        }

        match self.convert_event(manager_event).await {
            Ok(snapshot_event) => {
                if let Err(e) = self.emitter.emit(snapshot_event).await {
                    tracing::error!("Failed to emit download snapshot: {}", e);
                }
            }
            Err(e) => {
                tracing::error!("Failed to convert event to snapshot: {}", e);
            }
        }

        if is_terminal_event {
            if let Err(e) = self.download_manager.process_pending_queue().await {
                tracing::warn!(
                    "Failed to continue download queue after terminal event: {}",
                    e
                );
            }
        }
    }

    async fn publish_domain_event(
        &self,
        manager_event: &crate::features::download::manager::DownloadEvent,
    ) -> Result<(), String> {
        use crate::features::download::events::model_download_events::{
            FileDownloadCompletedEvent, FileDownloadFailedEvent, FileDownloadStartedEvent,
            ModelDownloadEvent,
        };
        use crate::features::download::manager::DownloadEvent as ME;

        let Some(event_bus) = &self.event_bus else {
            return Ok(());
        };

        // STATE-ONLY POLICY: do not republish high-volume Progress
        // events to the domain bus. The bus is reserved for low-rate
        // state transitions (Started / Completed / Failed) so a slow
        // subscriber cannot evict a critical state event from the
        // 1000-cap broadcast channel.
        // Progress is still surfaced to the UI — `handle_infrastructure_event`
        // converts manager events directly into snapshots and emits
        // them on the `download:progress` Tauri channel without going
        // through the bus.
        // Paused/Resumed are also bus-irrelevant; the saga doesn't act
        // on them and the UI is driven by snapshots.
        let session_id = match manager_event {
            ME::Started { id }
            | ME::Completed { id }
            | ME::Failed { id, .. }
            | ME::Cancelled { id } => id,
            ME::Progress { .. } | ME::Paused { .. } | ME::Resumed { .. } => return Ok(()),
        };

        let Some(session) = self
            .repository
            .get(session_id)
            .await
            .map_err(|e| format!("Failed to load download session {}: {}", session_id, e))?
        else {
            return Ok(());
        };

        let Some(model_id) = session.model_id().map(|s| s.to_string()) else {
            return Ok(());
        };

        let file_name = model_file_name_for_session(&session);

        let total_bytes = session
            .progress()
            .total_bytes()
            .unwrap_or_else(|| session.progress().bytes_downloaded());

        // Progress / Paused / Resumed already returned early above —
        // they are not bus-eligible.
        let event = match manager_event {
            ME::Started { .. } => {
                ModelDownloadEvent::FileDownloadStarted(FileDownloadStartedEvent {
                    model_id,
                    file_id: session_id.clone(),
                    file_name,
                    file_size: total_bytes,
                    timestamp: chrono::Utc::now(),
                })
            }
            ME::Completed { .. } => {
                ModelDownloadEvent::FileDownloadCompleted(FileDownloadCompletedEvent {
                    model_id,
                    file_id: session_id.clone(),
                    file_name,
                    file_size: total_bytes,
                    timestamp: chrono::Utc::now(),
                })
            }
            ME::Failed { error, .. } => {
                ModelDownloadEvent::FileDownloadFailed(FileDownloadFailedEvent {
                    model_id,
                    file_id: session_id.clone(),
                    file_name,
                    error: error.clone(),
                    timestamp: chrono::Utc::now(),
                })
            }
            ME::Cancelled { .. } => {
                ModelDownloadEvent::FileDownloadFailed(FileDownloadFailedEvent {
                    model_id,
                    file_id: session_id.clone(),
                    file_name,
                    error: "Download cancelled".to_string(),
                    timestamp: chrono::Utc::now(),
                })
            }
            ME::Progress { .. } | ME::Paused { .. } | ME::Resumed { .. } => {
                // Filtered out by the session_id match above. This arm
                // exists only to satisfy match exhaustiveness; the
                // early `return Ok(())` ensures we never reach here.
                return Ok(());
            }
        };

        if let Err(e) = event_bus.publish(event) {
            tracing::warn!(
                session_id = %session_id,
                error = %e,
                "No subscribers for domain download event"
            );
        }

        Ok(())
    }

    /// Handle domain events from EventBus (model-level aggregated events from DownloadSaga)
    async fn handle_domain_event(
        &self,
        domain_event: crate::features::download::events::model_download_events::ModelDownloadEvent,
    ) {
        use crate::features::download::events::model_download_events::ModelDownloadEvent;

        // Only forward ModelDownloadCompleted to frontend
        match domain_event {
            ModelDownloadEvent::ModelDownloadCompleted(payload) => {
                tracing::info!(
                    "Forwarding ModelDownloadCompleted to frontend: model_id={}, model_name={}, files={}",
                    payload.model_id,
                    payload.model_name,
                    payload.file_count
                );

                if let Err(e) = self.emitter.app_handle.emit("download:completed", &payload) {
                    tracing::error!("Failed to emit download:completed event to frontend: {}", e);
                }
            }
            _ => {
                // Ignore other domain events
            }
        }
    }

    /// Check if a download session is part of a multi-file batch
    /// Returns Some(model_id) if session belongs to a batch with >1 files
    async fn is_batch_download(&self, session_id: &str) -> Result<Option<String>, String> {
        let session = self
            .repository
            .get(session_id)
            .await
            .map_err(|e| format!("Failed to fetch session: {}", e))?
            .ok_or_else(|| format!("Session {} not found", session_id))?;

        if let Some(model_id) = session.model_id() {
            // Query repository for all sessions with this model_id
            let all_sessions = self
                .repository
                .list()
                .await
                .map_err(|e| format!("Failed to list sessions: {}", e))?;

            let model_sessions: Vec<_> = all_sessions
                .iter()
                .filter(|s| s.model_id() == Some(model_id))
                .collect();

            // If multiple sessions share this model_id, it's a batch
            if model_sessions.len() > 1 {
                return Ok(Some(model_id.to_string()));
            }
        }

        Ok(None)
    }

    /// Create batch snapshot by aggregating all files in a model download
    async fn create_batch_snapshot(
        &self,
        model_id: &str,
    ) -> Result<crate::domain::download_snapshot::BatchSnapshot, String> {
        use crate::domain::download_snapshot::{
            BatchSnapshot, DownloadStatus, FileSnapshot, FileStatus,
        };

        let all_sessions = self
            .repository
            .list()
            .await
            .map_err(|e| format!("Failed to list sessions: {}", e))?;

        let model_sessions: Vec<_> = all_sessions
            .into_iter()
            .filter(|s| s.model_id() == Some(model_id))
            .collect();

        if model_sessions.is_empty() {
            return Err(format!("No sessions found for model {}", model_id));
        }

        let model_name = model_sessions
            .first()
            .and_then(|s| s.model_name())
            .unwrap_or("Unknown Model")
            .to_string();

        let mut files = Vec::new();
        let mut aggregate_bytes_downloaded = 0u64;
        let mut aggregate_total_bytes = 0u64;
        let mut aggregate_bytes_per_second = 0.0;
        let mut completed_files = 0usize;
        let mut overall_status = DownloadStatus::Downloading;

        for session in &model_sessions {
            // The path inside the model, not the bare file name: a sentence-
            // transformers model has a `config.json` and a `1_Pooling/config.json`,
            // and the frontend keys a row by this.
            let filename = session
                .model_file_name()
                .or_else(|| session.destination().file_name().and_then(|n| n.to_str()))
                .unwrap_or("unknown")
                .to_string();

            let file_status = match session.state() {
                crate::domain::download::DownloadState::Pending => FileStatus::Pending,
                crate::domain::download::DownloadState::Downloading => FileStatus::Downloading,
                crate::domain::download::DownloadState::Completed => {
                    completed_files += 1;
                    FileStatus::Completed
                }
                crate::domain::download::DownloadState::Failed => FileStatus::Error,
                crate::domain::download::DownloadState::Cancelled => FileStatus::Error,
                crate::domain::download::DownloadState::Paused => FileStatus::Pending,
            };

            files.push(FileSnapshot {
                id: session.id().to_string(),
                filename,
                bytes_downloaded: session.progress().bytes_downloaded(),
                total_bytes: session.progress().total_bytes().unwrap_or(0),
                status: file_status,
            });

            aggregate_bytes_downloaded += session.progress().bytes_downloaded();
            aggregate_total_bytes += session.progress().total_bytes().unwrap_or(0);
            aggregate_bytes_per_second += session.progress().bytes_per_second();
        }

        // Determine overall status
        if completed_files == model_sessions.len() && !model_sessions.is_empty() {
            overall_status = DownloadStatus::Completed;
        } else if model_sessions
            .iter()
            .any(|s| s.state() == &crate::domain::download::DownloadState::Failed)
        {
            overall_status = DownloadStatus::Error;
        } else if model_sessions
            .iter()
            .any(|s| s.state() == &crate::domain::download::DownloadState::Paused)
        {
            overall_status = DownloadStatus::Paused;
        }

        let aggregate_percentage = if aggregate_total_bytes > 0 {
            (aggregate_bytes_downloaded as f64 / aggregate_total_bytes as f64) * 100.0
        } else {
            0.0
        };

        let aggregate_eta_seconds = if aggregate_bytes_per_second > 0.0
            && aggregate_total_bytes > aggregate_bytes_downloaded
        {
            Some(
                ((aggregate_total_bytes - aggregate_bytes_downloaded) as f64
                    / aggregate_bytes_per_second) as u64,
            )
        } else {
            None
        };

        Ok(BatchSnapshot {
            id: model_id.to_string(),
            group_name: model_name,
            files,
            total_files: model_sessions.len() as u32,
            completed_files: completed_files as u32,
            aggregate_bytes_downloaded,
            aggregate_total_bytes,
            aggregate_bytes_per_second: aggregate_bytes_per_second as u64,
            aggregate_percentage,
            aggregate_eta_seconds,
            status: overall_status,
        })
    }

    async fn convert_event(
        &self,
        event: crate::features::download::manager::DownloadEvent,
    ) -> Result<DownloadEvent, String> {
        use crate::domain::download_snapshot::{DownloadStatus, SingleFileSnapshot};
        use crate::features::download::manager::DownloadEvent as ME;

        match event {
            ME::Started { id } => {
                let session = self
                    .repository
                    .get(&id)
                    .await
                    .map_err(|e| format!("Failed to fetch session: {}", e))?
                    .ok_or_else(|| format!("Session {} not found", id))?;

                if let Some(model_id) = self.is_batch_download(&id).await? {
                    let batch = self.create_batch_snapshot(&model_id).await?;
                    return Ok(DownloadEvent::from_batch(batch));
                }

                Ok(DownloadEvent::from_single(SingleFileSnapshot {
                    id,
                    filename: session
                        .destination()
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    bytes_downloaded: 0,
                    total_bytes: session.progress().total_bytes(),
                    bytes_per_second: 0,
                    percentage: Some(0.0),
                    eta_seconds: None,
                    status: DownloadStatus::Pending,
                }))
            }
            ME::Progress {
                id,
                bytes_downloaded,
                bytes_per_second,
            } => {
                let session = self
                    .repository
                    .get(&id)
                    .await
                    .map_err(|e| format!("Failed to fetch session: {}", e))?
                    .ok_or_else(|| format!("Session {} not found", id))?;

                if let Some(model_id) = self.is_batch_download(&id).await? {
                    let batch = self.create_batch_snapshot(&model_id).await?;
                    return Ok(DownloadEvent::from_batch(batch));
                }

                let total_bytes = session.progress().total_bytes();
                let (percentage, eta_seconds) = if let Some(total) = total_bytes {
                    let pct = if total > 0 {
                        Some((bytes_downloaded as f64 / total as f64) * 100.0)
                    } else {
                        None
                    };
                    let eta = if bytes_per_second > 0.0 && total > bytes_downloaded {
                        Some(((total - bytes_downloaded) as f64 / bytes_per_second) as u64)
                    } else {
                        None
                    };
                    (pct, eta)
                } else {
                    (None, None)
                };

                Ok(DownloadEvent::from_single(SingleFileSnapshot {
                    id,
                    filename: session
                        .destination()
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    bytes_downloaded,
                    total_bytes,
                    bytes_per_second: bytes_per_second as u64,
                    percentage,
                    eta_seconds,
                    status: DownloadStatus::Downloading,
                }))
            }
            ME::Paused { id } => {
                let session = self
                    .repository
                    .get(&id)
                    .await
                    .map_err(|e| format!("Failed to fetch session: {}", e))?
                    .ok_or_else(|| format!("Session {} not found", id))?;

                if let Some(model_id) = self.is_batch_download(&id).await? {
                    let batch = self.create_batch_snapshot(&model_id).await?;
                    return Ok(DownloadEvent::from_batch(batch));
                }

                let bytes_downloaded = session.progress().bytes_downloaded();
                let total_bytes = session.progress().total_bytes();
                let percentage = if let Some(total) = total_bytes {
                    if total > 0 {
                        Some((bytes_downloaded as f64 / total as f64) * 100.0)
                    } else {
                        None
                    }
                } else {
                    None
                };

                Ok(DownloadEvent::from_single(SingleFileSnapshot {
                    id,
                    filename: session
                        .destination()
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    bytes_downloaded,
                    total_bytes,
                    bytes_per_second: 0,
                    percentage,
                    eta_seconds: None,
                    status: DownloadStatus::Paused,
                }))
            }
            ME::Resumed { id } => {
                let session = self
                    .repository
                    .get(&id)
                    .await
                    .map_err(|e| format!("Failed to fetch session: {}", e))?
                    .ok_or_else(|| format!("Session {} not found", id))?;

                if let Some(model_id) = self.is_batch_download(&id).await? {
                    let batch = self.create_batch_snapshot(&model_id).await?;
                    return Ok(DownloadEvent::from_batch(batch));
                }

                let bytes_downloaded = session.progress().bytes_downloaded();
                let total_bytes = session.progress().total_bytes();
                let percentage = if let Some(total) = total_bytes {
                    if total > 0 {
                        Some((bytes_downloaded as f64 / total as f64) * 100.0)
                    } else {
                        None
                    }
                } else {
                    None
                };

                Ok(DownloadEvent::from_single(SingleFileSnapshot {
                    id,
                    filename: session
                        .destination()
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    bytes_downloaded,
                    total_bytes,
                    bytes_per_second: 0,
                    percentage,
                    eta_seconds: None,
                    status: DownloadStatus::Downloading,
                }))
            }
            ME::Completed { id } => {
                let session = self
                    .repository
                    .get(&id)
                    .await
                    .map_err(|e| format!("Failed to fetch session: {}", e))?
                    .ok_or_else(|| format!("Session {} not found", id))?;

                if let Some(model_id) = self.is_batch_download(&id).await? {
                    let batch = self.create_batch_snapshot(&model_id).await?;
                    return Ok(DownloadEvent::from_batch(batch));
                }

                let total_bytes = session.progress().total_bytes();
                let percentage = if total_bytes.is_some() {
                    Some(100.0)
                } else {
                    None
                };

                Ok(DownloadEvent::from_single(SingleFileSnapshot {
                    id,
                    filename: session
                        .destination()
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    bytes_downloaded: total_bytes.unwrap_or(0),
                    total_bytes,
                    bytes_per_second: 0,
                    percentage,
                    eta_seconds: None,
                    status: DownloadStatus::Completed,
                }))
            }
            ME::Failed { id, error: _ } => {
                let session = self
                    .repository
                    .get(&id)
                    .await
                    .map_err(|e| format!("Failed to fetch session: {}", e))?
                    .ok_or_else(|| format!("Session {} not found", id))?;

                if let Some(model_id) = self.is_batch_download(&id).await? {
                    let batch = self.create_batch_snapshot(&model_id).await?;
                    return Ok(DownloadEvent::from_batch(batch));
                }

                let bytes_downloaded = session.progress().bytes_downloaded();
                let total_bytes = session.progress().total_bytes();
                let percentage = if let Some(total) = total_bytes {
                    if total > 0 {
                        Some((bytes_downloaded as f64 / total as f64) * 100.0)
                    } else {
                        None
                    }
                } else {
                    None
                };

                Ok(DownloadEvent::from_single(SingleFileSnapshot {
                    id,
                    filename: session
                        .destination()
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    bytes_downloaded,
                    total_bytes,
                    bytes_per_second: 0,
                    percentage,
                    eta_seconds: None,
                    status: DownloadStatus::Error,
                }))
            }
            ME::Cancelled { id } => {
                let session = self
                    .repository
                    .get(&id)
                    .await
                    .map_err(|e| format!("Failed to fetch session: {}", e))?
                    .ok_or_else(|| format!("Session {} not found", id))?;

                if let Some(model_id) = self.is_batch_download(&id).await? {
                    let batch = self.create_batch_snapshot(&model_id).await?;
                    return Ok(DownloadEvent::from_batch(batch));
                }

                let bytes_downloaded = session.progress().bytes_downloaded();
                let total_bytes = session.progress().total_bytes();
                let percentage = if let Some(total) = total_bytes {
                    if total > 0 {
                        Some((bytes_downloaded as f64 / total as f64) * 100.0)
                    } else {
                        None
                    }
                } else {
                    None
                };

                Ok(DownloadEvent::from_single(SingleFileSnapshot {
                    id,
                    filename: session
                        .destination()
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    bytes_downloaded,
                    total_bytes,
                    bytes_per_second: 0,
                    percentage,
                    eta_seconds: None,
                    status: DownloadStatus::Cancelled,
                }))
            }
        }
    }
}

fn model_file_name_for_session(session: &crate::domain::download::DownloadSession) -> String {
    session
        .model_file_name()
        .map(str::to_string)
        .or_else(|| {
            session
                .destination()
                .file_name()
                .and_then(|s| s.to_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::model_file_name_for_session;
    use crate::domain::download::DownloadSession;

    #[test]
    fn session_preserves_manifest_relative_file_name() {
        let session = DownloadSession::new(
            "download-id".to_string(),
            "https://example.test/onnx/model.onnx".to_string(),
            std::path::PathBuf::from("/home/u/.cache/lattice/models/all-mpnet-base-v2/model.onnx"),
            Some(42),
            None,
        )
        .expect("session")
        .with_model_file_name("onnx/model.onnx".to_string());

        assert_eq!(model_file_name_for_session(&session), "onnx/model.onnx");
    }
}
