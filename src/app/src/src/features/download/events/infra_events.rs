use crate::domain::download_snapshot::{BatchSnapshot, DownloadStateSnapshot, SingleFileSnapshot};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, RwLock};

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
        // Emit snapshot events to frontend
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
    download_manager: Arc<dyn crate::infrastructure::services::download_manager::DownloadManager>,
    repository: Arc<dyn crate::infrastructure::persistence::download_repository::DownloadRepository>,
    event_rx_arc: Arc<RwLock<Option<mpsc::UnboundedReceiver<crate::infrastructure::services::download_manager::DownloadEvent>>>>,
    downloaded_model_repository: Option<Arc<crate::infrastructure::persistence::repositories::downloaded_model_repository::DownloadedModelRepository>>,
    event_bus: Option<Arc<crate::infrastructure::event_bus::EventBus<crate::domain::events::model_download_events::ModelDownloadEvent>>>,
}

impl DownloadEventBridge {
    pub fn new(
        app_handle: AppHandle,
        download_manager: Arc<
            dyn crate::infrastructure::services::download_manager::DownloadManager,
        >,
        repository: Arc<
            dyn crate::infrastructure::persistence::download_repository::DownloadRepository,
        >,
        event_rx_arc: Arc<
            RwLock<
                Option<
                    mpsc::UnboundedReceiver<
                        crate::infrastructure::services::download_manager::DownloadEvent,
                    >,
                >,
            >,
        >,
        downloaded_model_repository: Option<Arc<crate::infrastructure::persistence::repositories::downloaded_model_repository::DownloadedModelRepository>>,
        event_bus: Option<
            Arc<
                crate::infrastructure::event_bus::EventBus<
                    crate::domain::events::model_download_events::ModelDownloadEvent,
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

    pub async fn start(self) {
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

        // Subscribe to domain events from EventBus if available
        let mut domain_subscriber = self.event_bus.as_ref().map(|bus| bus.subscribe());

        // Run two concurrent event loops:
        // STREAM A: Infrastructure events from DownloadManager
        // STREAM B: Domain events from EventBus (ModelDownloadCompleted)
        loop {
            tokio::select! {
                // STREAM A: Infrastructure events (file-level progress, started, etc.)
                Some(manager_event) = event_rx.recv() => {
                    self.handle_infrastructure_event(manager_event).await;
                }
                // STREAM B: Domain events from EventBus (aggregated model-level events)
                Some(Ok(domain_event)) = async {
                    match &mut domain_subscriber {
                        Some(rx) => Some(rx.recv().await),
                        None => None,
                    }
                } => {
                    self.handle_domain_event(domain_event).await;
                }
                else => {
                    // Both streams closed
                    break;
                }
            }
        }

        tracing::info!("Download event bridge stopped");
    }

    /// Handle infrastructure events from DownloadManager (file-level events)
    /// Converts to snapshot and emits
    async fn handle_infrastructure_event(
        &self,
        manager_event: crate::infrastructure::services::download_manager::DownloadEvent,
    ) {
        let is_terminal_event = matches!(
            manager_event,
            crate::infrastructure::services::download_manager::DownloadEvent::Completed { .. }
                | crate::infrastructure::services::download_manager::DownloadEvent::Failed { .. }
                | crate::infrastructure::services::download_manager::DownloadEvent::Cancelled { .. }
        );

        // Bridge manager events into domain download events so DownloadSaga can
        // update model_files/model status atomically.
        if let Err(e) = self.publish_domain_event(&manager_event).await {
            tracing::warn!("Failed to publish domain download event: {}", e);
        }

        if let crate::infrastructure::services::download_manager::DownloadEvent::Failed {
            id,
            error,
        } = &manager_event
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

        // Convert manager event to snapshot
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
        manager_event: &crate::infrastructure::services::download_manager::DownloadEvent,
    ) -> Result<(), String> {
        use crate::domain::events::model_download_events::{
            FileDownloadCompletedEvent, FileDownloadFailedEvent, FileDownloadProgressEvent,
            FileDownloadStartedEvent, ModelDownloadEvent,
        };
        use crate::infrastructure::services::download_manager::DownloadEvent as ME;

        let Some(event_bus) = &self.event_bus else {
            return Ok(());
        };

        let session_id = match manager_event {
            ME::Started { id }
            | ME::Progress { id, .. }
            | ME::Completed { id }
            | ME::Failed { id, .. }
            | ME::Cancelled { id } => id,
            ME::Paused { .. } | ME::Resumed { .. } => return Ok(()),
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

        // Extract relative filename from URL (e.g., "onnx/model.onnx" from
        // "https://huggingface.co/repo/resolve/main/onnx/model.onnx").
        // This must match the file_name stored in model_files DB records.
        // Falling back to destination basename for non-HF URLs.
        let file_name = extract_relative_filename_from_url(session.url())
            .unwrap_or_else(|| {
                session
                    .destination()
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string()
            });

        let total_bytes = session
            .progress()
            .total_bytes()
            .unwrap_or_else(|| session.progress().bytes_downloaded());

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
            ME::Progress {
                bytes_downloaded, ..
            } => ModelDownloadEvent::FileDownloadProgress(FileDownloadProgressEvent {
                model_id,
                file_id: session_id.clone(),
                file_name,
                bytes_downloaded: *bytes_downloaded,
                total_bytes,
                timestamp: chrono::Utc::now(),
            }),
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
            ME::Paused { .. } | ME::Resumed { .. } => return Ok(()),
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
        domain_event: crate::domain::events::model_download_events::ModelDownloadEvent,
    ) {
        use crate::domain::events::model_download_events::ModelDownloadEvent;

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

        // Check if session has model metadata
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

        // Get all sessions for this model
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
            let filename = session
                .destination()
                .file_name()
                .and_then(|n| n.to_str())
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
        event: crate::infrastructure::services::download_manager::DownloadEvent,
    ) -> Result<DownloadEvent, String> {
        use crate::domain::download_snapshot::{DownloadStatus, SingleFileSnapshot};
        use crate::infrastructure::services::download_manager::DownloadEvent as ME;

        match event {
            ME::Started { id } => {
                let session = self
                    .repository
                    .get(&id)
                    .await
                    .map_err(|e| format!("Failed to fetch session: {}", e))?
                    .ok_or_else(|| format!("Session {} not found", id))?;

                // Check if this is a batch download
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

                // Check if this is a batch download
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

                // Check if this is a batch download
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

                // Check if this is a batch download
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

                // Check if this is a batch download
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

                // Check if this is a batch download
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

                // Check if this is a batch download
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

/// Extract the relative filename from a HuggingFace download URL.
///
/// Given `https://huggingface.co/org/repo/resolve/main/onnx/model.onnx`,
/// returns `Some("onnx/model.onnx")`.
/// Returns `None` for non-HF URLs or if the pattern is not found.
fn extract_relative_filename_from_url(url: &str) -> Option<String> {
    let marker = "/resolve/main/";
    let idx = url.find(marker)?;
    let relative = &url[idx + marker.len()..];
    if relative.is_empty() {
        None
    } else {
        // URL-decode percent-encoded characters (spaces, etc.)
        Some(
            percent_decode_simple(relative)
                .trim_end_matches('/')
                .to_string(),
        )
    }
}

/// Minimal percent-decoding for common URL characters.
fn percent_decode_simple(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.bytes();
    while let Some(b) = chars.next() {
        if b == b'%' {
            let hi = chars.next();
            let lo = chars.next();
            if let (Some(h), Some(l)) = (hi, lo) {
                let hex = [h, l];
                if let Ok(s) = std::str::from_utf8(&hex) {
                    if let Ok(byte) = u8::from_str_radix(s, 16) {
                        result.push(byte as char);
                        continue;
                    }
                }
                // Couldn't decode — emit raw
                result.push('%');
                result.push(h as char);
                result.push(l as char);
            } else {
                result.push('%');
            }
        } else {
            result.push(b as char);
        }
    }
    result
}
