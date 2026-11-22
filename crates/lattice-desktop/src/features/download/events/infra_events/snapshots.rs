//! Turning download-manager events into the single-file and batch snapshots the UI shows.

use lattice_core::shared::error::{AppError, Result};

use super::{DownloadEvent, DownloadEventBridge};

impl DownloadEventBridge {
    /// Check if a download session is part of a multi-file batch
    /// Returns Some(model_id) if session belongs to a batch with >1 files
    async fn is_batch_download(&self, session_id: &str) -> Result<Option<String>> {
        let session = self
            .repository
            .get(session_id)
            .await
            .map_err(|e| AppError::Database(format!("Failed to fetch session: {}", e)))?
            .ok_or_else(|| AppError::NotFound(format!("Session {} not found", session_id)))?;

        if let Some(model_id) = session.model_id() {
            // Query repository for all sessions with this model_id
            let all_sessions = self
                .repository
                .list()
                .await
                .map_err(|e| AppError::Database(format!("Failed to list sessions: {}", e)))?;

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
    ) -> Result<lattice_core::domain::download::snapshot::BatchSnapshot> {
        let all_sessions = self
            .repository
            .list()
            .await
            .map_err(|e| AppError::Database(format!("Failed to list sessions: {}", e)))?;

        let model_sessions: Vec<_> = all_sessions
            .into_iter()
            .filter(|s| s.model_id() == Some(model_id))
            .collect();

        build_batch_snapshot_from_sessions(model_id, &model_sessions)
    }

    pub(super) async fn convert_event(
        &self,
        event: lattice_core::application::ports::download_manager::DownloadEvent,
    ) -> Result<DownloadEvent> {
        use lattice_core::application::ports::download_manager::DownloadEvent as ME;
        use lattice_core::domain::download::snapshot::{DownloadStatus, SingleFileSnapshot};

        match event {
            ME::Started { id } => {
                let session = self
                    .repository
                    .get(&id)
                    .await
                    .map_err(|e| AppError::Database(format!("Failed to fetch session: {}", e)))?
                    .ok_or_else(|| AppError::NotFound(format!("Session {} not found", id)))?;

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
                    destination: session.destination().to_string_lossy().to_string(),
                    model_id: session.model_id().map(str::to_string),
                    model_name: session.model_name().map(str::to_string),
                    bytes_downloaded: 0,
                    total_bytes: session.progress().total_bytes(),
                    bytes_per_second: 0,
                    percentage: Some(0.0),
                    eta_seconds: None,
                    status: DownloadStatus::Pending,
                    error_message: session.error_message().map(str::to_string),
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
                    .map_err(|e| AppError::Database(format!("Failed to fetch session: {}", e)))?
                    .ok_or_else(|| AppError::NotFound(format!("Session {} not found", id)))?;

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
                    destination: session.destination().to_string_lossy().to_string(),
                    model_id: session.model_id().map(str::to_string),
                    model_name: session.model_name().map(str::to_string),
                    bytes_downloaded,
                    total_bytes,
                    bytes_per_second: bytes_per_second as u64,
                    percentage,
                    eta_seconds,
                    status: DownloadStatus::Downloading,
                    error_message: session.error_message().map(str::to_string),
                }))
            }
            ME::Paused { id } => {
                let session = self
                    .repository
                    .get(&id)
                    .await
                    .map_err(|e| AppError::Database(format!("Failed to fetch session: {}", e)))?
                    .ok_or_else(|| AppError::NotFound(format!("Session {} not found", id)))?;

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
                    destination: session.destination().to_string_lossy().to_string(),
                    model_id: session.model_id().map(str::to_string),
                    model_name: session.model_name().map(str::to_string),
                    bytes_downloaded,
                    total_bytes,
                    bytes_per_second: 0,
                    percentage,
                    eta_seconds: None,
                    status: DownloadStatus::Paused,
                    error_message: session.error_message().map(str::to_string),
                }))
            }
            ME::Resumed { id } => {
                let session = self
                    .repository
                    .get(&id)
                    .await
                    .map_err(|e| AppError::Database(format!("Failed to fetch session: {}", e)))?
                    .ok_or_else(|| AppError::NotFound(format!("Session {} not found", id)))?;

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
                    destination: session.destination().to_string_lossy().to_string(),
                    model_id: session.model_id().map(str::to_string),
                    model_name: session.model_name().map(str::to_string),
                    bytes_downloaded,
                    total_bytes,
                    bytes_per_second: 0,
                    percentage,
                    eta_seconds: None,
                    status: status_for(session.state()),
                    error_message: session.error_message().map(str::to_string),
                }))
            }
            ME::Completed { id } => {
                let session = self
                    .repository
                    .get(&id)
                    .await
                    .map_err(|e| AppError::Database(format!("Failed to fetch session: {}", e)))?
                    .ok_or_else(|| AppError::NotFound(format!("Session {} not found", id)))?;

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
                    destination: session.destination().to_string_lossy().to_string(),
                    model_id: session.model_id().map(str::to_string),
                    model_name: session.model_name().map(str::to_string),
                    bytes_downloaded: total_bytes.unwrap_or(0),
                    total_bytes,
                    bytes_per_second: 0,
                    percentage,
                    eta_seconds: None,
                    status: DownloadStatus::Completed,
                    error_message: None,
                }))
            }
            ME::Failed { id, error } => {
                let session = self
                    .repository
                    .get(&id)
                    .await
                    .map_err(|e| AppError::Database(format!("Failed to fetch session: {}", e)))?
                    .ok_or_else(|| AppError::NotFound(format!("Session {} not found", id)))?;

                if let Some(model_id) = self.is_batch_download(&id).await? {
                    let batch = self.create_batch_snapshot(&model_id).await?;
                    return Ok(DownloadEvent::from_batch(batch));
                }

                Ok(DownloadEvent::from_single(build_single_failed_snapshot(
                    id, &session, error,
                )))
            }
            ME::Cancelled { id } => {
                let session = self
                    .repository
                    .get(&id)
                    .await
                    .map_err(|e| AppError::Database(format!("Failed to fetch session: {}", e)))?
                    .ok_or_else(|| AppError::NotFound(format!("Session {} not found", id)))?;

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
                    destination: session.destination().to_string_lossy().to_string(),
                    model_id: session.model_id().map(str::to_string),
                    model_name: session.model_name().map(str::to_string),
                    bytes_downloaded,
                    total_bytes,
                    bytes_per_second: 0,
                    percentage,
                    eta_seconds: None,
                    status: DownloadStatus::Cancelled,
                    error_message: session.error_message().map(str::to_string),
                }))
            }
        }
    }
}

/// Aggregate sessions sharing a `model_id` into a [`BatchSnapshot`], giving
/// active and failed states precedence (`Downloading > Error > Paused > Pending > Cancelled > Completed`).
pub(super) fn build_batch_snapshot_from_sessions(
    model_id: &str,
    model_sessions: &[lattice_core::domain::download::DownloadSession],
) -> Result<lattice_core::domain::download::snapshot::BatchSnapshot> {
    use lattice_core::domain::download::snapshot::{
        BatchSnapshot, DownloadStatus, FileSnapshot, FileStatus,
    };
    use lattice_core::domain::download::DownloadState;

    if model_sessions.is_empty() {
        return Err(AppError::NotFound(format!(
            "No sessions found for model {}",
            model_id
        )));
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

    for session in model_sessions {
        // The path inside the model, not the bare file name: a sentence-
        // transformers model has a `config.json` and a `1_Pooling/config.json`,
        // and the frontend keys a row by this.
        let filename = session
            .model_file_name()
            .or_else(|| session.destination().file_name().and_then(|n| n.to_str()))
            .unwrap_or("unknown")
            .to_string();

        let file_status = match session.state() {
            DownloadState::Pending => FileStatus::Pending,
            DownloadState::Downloading => FileStatus::Downloading,
            DownloadState::Completed => {
                completed_files += 1;
                FileStatus::Completed
            }
            DownloadState::Failed => FileStatus::Error,
            DownloadState::Cancelled => FileStatus::Cancelled,
            DownloadState::Paused => FileStatus::Paused,
        };

        files.push(FileSnapshot {
            id: session.id().to_string(),
            filename,
            bytes_downloaded: session.progress().bytes_downloaded(),
            total_bytes: session.progress().total_bytes().unwrap_or(0),
            status: file_status,
            error_message: session.error_message().map(str::to_string),
        });

        aggregate_bytes_downloaded += session.progress().bytes_downloaded();
        aggregate_total_bytes += session.progress().total_bytes().unwrap_or(0);
        aggregate_bytes_per_second += session.progress().bytes_per_second();
    }

    let overall_status = if model_sessions
        .iter()
        .any(|s| s.state() == &DownloadState::Downloading)
    {
        DownloadStatus::Downloading
    } else if model_sessions
        .iter()
        .any(|s| s.state() == &DownloadState::Failed)
    {
        DownloadStatus::Error
    } else if model_sessions
        .iter()
        .any(|s| s.state() == &DownloadState::Paused)
    {
        DownloadStatus::Paused
    } else if model_sessions
        .iter()
        .any(|s| s.state() == &DownloadState::Pending)
    {
        DownloadStatus::Pending
    } else if model_sessions
        .iter()
        .any(|s| s.state() == &DownloadState::Cancelled)
    {
        DownloadStatus::Cancelled
    } else {
        DownloadStatus::Completed
    };

    let aggregate_percentage = if aggregate_total_bytes > 0 {
        (aggregate_bytes_downloaded as f64 / aggregate_total_bytes as f64) * 100.0
    } else {
        0.0
    };

    let aggregate_eta_seconds =
        if aggregate_bytes_per_second > 0.0 && aggregate_total_bytes > aggregate_bytes_downloaded {
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

/// Build a single-file failed snapshot preserving `model_id` and `error_message`.
pub(super) fn build_single_failed_snapshot(
    id: String,
    session: &lattice_core::domain::download::DownloadSession,
    fallback_error: String,
) -> lattice_core::domain::download::snapshot::SingleFileSnapshot {
    use lattice_core::domain::download::snapshot::{DownloadStatus, SingleFileSnapshot};

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

    SingleFileSnapshot {
        id,
        filename: session
            .destination()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string(),
        destination: session.destination().to_string_lossy().to_string(),
        model_id: session.model_id().map(str::to_string),
        model_name: session.model_name().map(str::to_string),
        bytes_downloaded,
        total_bytes,
        bytes_per_second: 0,
        percentage,
        eta_seconds: None,
        status: DownloadStatus::Error,
        error_message: session
            .error_message()
            .map(str::to_string)
            .or(Some(fallback_error)),
    }
}

/// The wire status for a session's state, so an event reports what the row
/// says (a resumed download waiting for a slot is `pending`, not `downloading`).
fn status_for(
    state: &lattice_core::domain::download::DownloadState,
) -> lattice_core::domain::download::snapshot::DownloadStatus {
    use lattice_core::domain::download::snapshot::DownloadStatus;
    use lattice_core::domain::download::DownloadState;
    match state {
        DownloadState::Pending => DownloadStatus::Pending,
        DownloadState::Downloading => DownloadStatus::Downloading,
        DownloadState::Paused => DownloadStatus::Paused,
        DownloadState::Completed => DownloadStatus::Completed,
        DownloadState::Failed => DownloadStatus::Error,
        DownloadState::Cancelled => DownloadStatus::Cancelled,
    }
}

#[cfg(test)]
mod status_tests {
    use super::*;
    use lattice_core::domain::download::snapshot::DownloadStatus;
    use lattice_core::domain::download::DownloadState;

    #[test]
    fn a_resumed_session_waiting_for_a_slot_reports_pending() {
        assert!(matches!(
            status_for(&DownloadState::Pending),
            DownloadStatus::Pending
        ));
        assert!(matches!(
            status_for(&DownloadState::Downloading),
            DownloadStatus::Downloading
        ));
    }
}
