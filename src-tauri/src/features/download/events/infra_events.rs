use lattice_core::domain::download::snapshot::{
    BatchSnapshot, DownloadStateSnapshot, SingleFileSnapshot,
};
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, RwLock};

mod snapshots;

const RECONCILIATION_BATCH_SIZE: usize = 64;
const RECONCILIATION_ACTIVE_POLL: std::time::Duration = std::time::Duration::from_secs(5);
const RECONCILIATION_IDLE_POLL: std::time::Duration = std::time::Duration::from_secs(30);

fn reconciliation_poll_delay(page_len: usize, failed: bool) -> std::time::Duration {
    if !failed && page_len == RECONCILIATION_BATCH_SIZE {
        RECONCILIATION_ACTIVE_POLL
    } else {
        RECONCILIATION_IDLE_POLL
    }
}
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

    pub async fn emit(&self, event: DownloadEvent) -> Result<()> {
        self.app_handle
            .emit("download:progress", event)
            .map_err(|e| AppError::InternalError(format!("Failed to emit event: {}", e)))
    }

    pub async fn emit_batch(&self, events: Vec<DownloadEvent>) -> Result<()> {
        for event in events {
            self.emit(event).await?;
        }
        Ok(())
    }

    pub async fn emit_failed(&self, event: DownloadFailedEvent) -> Result<()> {
        self.app_handle
            .emit("download:failed", event)
            .map_err(|e| AppError::InternalError(format!("Failed to emit failed event: {}", e)))
    }
}

pub struct DownloadEventBridge {
    emitter: Arc<DownloadEventEmitter>,
    download_manager: Arc<dyn crate::features::download::manager::DownloadManager>,
    repository: Arc<dyn crate::features::download::download_repository::DownloadRepository>,
    event_rx_arc:
        Arc<RwLock<Option<mpsc::Receiver<crate::features::download::manager::DownloadEvent>>>>,
    // reason: holds the repository handle supplied by DI wiring in
    // bootstrap/app.rs; dropping it would change the public `new` signature.
    #[allow(dead_code)]
    downloaded_model_repository: Option<
        Arc<crate::infrastructure::persistence::repositories::downloaded_models::DownloadedModelRepository>,
    >,
    event_bus: Option<
        Arc<
            crate::infrastructure::events::event_bus::EventBus<
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
            RwLock<Option<mpsc::Receiver<crate::features::download::manager::DownloadEvent>>>,
        >,
        downloaded_model_repository: Option<
            Arc<crate::infrastructure::persistence::repositories::downloaded_models::DownloadedModelRepository>,
        >,
        event_bus: Option<
            Arc<
                crate::infrastructure::events::event_bus::EventBus<
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
        // Terminal manager events are an in-memory notification, while their
        // durable session row survives a full channel or process restart.
        // Reconcile a bounded keyset page until the saga's model_files write
        // matches the session state.
        let mut reconciliation_cursor: Option<String> = None;
        let mut reconciliation_tick = Box::pin(tokio::time::sleep(std::time::Duration::ZERO));

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
                _ = &mut reconciliation_tick => {
                    match self.repository
                        .list_terminal_sessions_needing_reconciliation(
                            reconciliation_cursor.as_deref(),
                            RECONCILIATION_BATCH_SIZE,
                        )
                        .await
                    {
                        Ok(sessions) if sessions.is_empty() => {
                            reconciliation_cursor = None;
                            reconciliation_tick.as_mut().reset(
                                tokio::time::Instant::now()
                                    + reconciliation_poll_delay(0, false),
                            );
                        }
                        Ok(sessions) => {
                            if sessions.len() == RECONCILIATION_BATCH_SIZE {
                                reconciliation_cursor = sessions.last().map(|session| session.id().to_string());
                            } else {
                                // This page exhausted the current backlog. Keep
                                // the fallback, but stop querying an idle DB on
                                // every short poll interval.
                                reconciliation_cursor = None;
                            }
                            let delay = reconciliation_poll_delay(sessions.len(), false);
                            for session in sessions {
                                use lattice_core::domain::download::DownloadState;
                                use crate::features::download::manager::DownloadEvent as ManagerEvent;

                                let event = match session.state() {
                                    DownloadState::Completed => Some(ManagerEvent::Completed {
                                        id: session.id().to_string(),
                                    }),
                                    DownloadState::Failed => Some(ManagerEvent::Failed {
                                        id: session.id().to_string(),
                                        error: session.error_message().unwrap_or("Download failed").to_string(),
                                    }),
                                    DownloadState::Cancelled => Some(ManagerEvent::Failed {
                                        id: session.id().to_string(),
                                        error: session.error_message().unwrap_or("Download cancelled").to_string(),
                                    }),
                                    _ => None,
                                };
                                if let Some(event) = event {
                                    tracing::warn!(
                                        download_id = %session.id(),
                                        state = ?session.state(),
                                        "Replaying terminal download state whose model file has not been reconciled"
                                    );
                                    self.handle_infrastructure_event(event).await;
                                }
                            }
                            reconciliation_tick.as_mut().reset(tokio::time::Instant::now() + delay);
                        }
                        Err(error) => {
                            tracing::error!(%error, "Failed to reconcile terminal download sessions");
                            reconciliation_cursor = None;
                            reconciliation_tick.as_mut().reset(
                                tokio::time::Instant::now()
                                    + reconciliation_poll_delay(0, true),
                            );
                        }
                    }
                }
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
        let terminal_event = is_terminal_manager_event(&manager_event);
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

        // Terminal transfers release a concurrency slot in the manager task.
        // Event delivery follows that cleanup; advancing here preserves the
        // queue's normal lifecycle without waiting for another user command.
        if let Err(error) =
            process_queue_after_terminal(self.download_manager.as_ref(), terminal_event).await
        {
            tracing::warn!(%error, "Failed to promote queued download after terminal event");
        }
    }

    async fn publish_domain_event(
        &self,
        manager_event: &crate::features::download::manager::DownloadEvent,
    ) -> Result<()> {
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

        let Some(session) = self.repository.get(session_id).await.map_err(|e| {
            AppError::Database(format!(
                "Failed to load download session {}: {}",
                session_id, e
            ))
        })?
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
}

async fn process_queue_after_terminal(
    manager: &dyn crate::features::download::manager::DownloadManager,
    terminal_event: bool,
) -> Result<(), lattice_core::domain::download::DownloadError> {
    if terminal_event {
        manager.process_pending_queue().await?;
    }
    Ok(())
}

fn is_terminal_manager_event(event: &crate::features::download::manager::DownloadEvent) -> bool {
    matches!(
        event,
        crate::features::download::manager::DownloadEvent::Completed { .. }
            | crate::features::download::manager::DownloadEvent::Failed { .. }
            | crate::features::download::manager::DownloadEvent::Cancelled { .. }
    )
}

fn model_file_name_for_session(
    session: &lattice_core::domain::download::DownloadSession,
) -> String {
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
mod tests;
