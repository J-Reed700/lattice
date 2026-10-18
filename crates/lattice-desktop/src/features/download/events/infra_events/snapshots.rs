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
        use lattice_core::domain::download::snapshot::{
            BatchSnapshot, DownloadStatus, FileSnapshot, FileStatus,
        };

        let all_sessions = self
            .repository
            .list()
            .await
            .map_err(|e| AppError::Database(format!("Failed to list sessions: {}", e)))?;

        let model_sessions: Vec<_> = all_sessions
            .into_iter()
            .filter(|s| s.model_id() == Some(model_id))
            .collect();

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
                lattice_core::domain::download::DownloadState::Pending => FileStatus::Pending,
                lattice_core::domain::download::DownloadState::Downloading => {
                    FileStatus::Downloading
                }
                lattice_core::domain::download::DownloadState::Completed => {
                    completed_files += 1;
                    FileStatus::Completed
                }
                lattice_core::domain::download::DownloadState::Failed => FileStatus::Error,
                lattice_core::domain::download::DownloadState::Cancelled => FileStatus::Error,
                lattice_core::domain::download::DownloadState::Paused => FileStatus::Pending,
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
            .any(|s| s.state() == &lattice_core::domain::download::DownloadState::Failed)
        {
            overall_status = DownloadStatus::Error;
        } else if model_sessions
            .iter()
            .any(|s| s.state() == &lattice_core::domain::download::DownloadState::Paused)
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
