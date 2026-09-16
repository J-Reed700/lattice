//! Spawning and running a single transfer task.
//!
//! Covers the whole life of one in-flight download: resume-offset recovery,
//! progress reporting, reacting to a stop signal, and finalisation
//! (validation, checksum verification, terminal state and event emission).

use super::state::{ActiveDownload, DownloadManagerService, StopReason};
use super::types::DownloadEvent;
use super::validation::{recover_resume_offset, validate_downloaded_file};
use crate::domain::download::DownloadError;
use crate::features::download::engine::{DownloadOptions, ProgressCallback};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

impl DownloadManagerService {
    pub(super) async fn start_download_task(&self, id: &str) -> Result<(), DownloadError> {
        let session = self
            .repository
            .get(id)
            .await?
            .ok_or_else(|| DownloadError::SessionNotFound(id.to_string()))?;

        if session.state().is_active() {
            return Ok(());
        }

        let mut session = session;
        session.start()?;
        self.repository.update(&session).await?;

        // Emit Started event IMMEDIATELY after DB update, before spawning task
        // This ensures UI gets state transition synchronously with no async gap
        if let Err(e) = self.event_tx.send(DownloadEvent::Started {
            id: session.id().to_string(),
        }) {
            warn!(
                session_id = %session.id(),
                error = %e,
                "Failed to send Started event (no receivers)"
            );
        }

        let (cancel_tx, mut cancel_rx) = mpsc::channel::<StopReason>(1);

        let engine = self.engine.clone();
        let repository = self.repository.clone();
        let event_tx = self.event_tx.clone();
        let active_downloads = self.active_downloads.clone();
        let auth_tokens = self.auth_tokens.clone();
        let file_cleanup = self.file_cleanup.clone();
        let session_id = session.id().to_string();
        let session_id_for_task = session_id.clone();
        let url = session.url().to_string();
        let destination = session.destination().clone();
        let checksum = session.checksum().cloned();

        let auth_token = {
            let tokens = auth_tokens.read().await;
            tokens.get(&session_id).cloned()
        };

        let progress_callback: ProgressCallback = Arc::new({
            let session_id_for_progress = session_id.clone();
            let event_tx = event_tx.clone();
            let repository_for_progress = repository.clone();
            move |bytes, speed| {
                let session_id_clone = session_id_for_progress.clone();
                let repository_clone = repository_for_progress.clone();

                // Column-scoped write: cannot clobber `state`, so a tick that
                // lands after a terminal transition can no longer revive a
                // completed or cancelled session as `downloading`. These tasks
                // are still detached and unordered with respect to each other,
                // which is fine now — the worst case is that byte counts
                // briefly go backwards, not that the state machine breaks.
                tokio::spawn(async move {
                    if let Err(e) = repository_clone
                        .update_progress(&session_id_clone, bytes, speed)
                        .await
                    {
                        error!(
                            session_id = %session_id_clone,
                            error = %e,
                            "Failed to update download progress in database"
                        );
                    }
                });

                if let Err(e) = event_tx.send(DownloadEvent::Progress {
                    id: session_id_for_progress.clone(),
                    bytes_downloaded: bytes,
                    bytes_per_second: speed,
                }) {
                    warn!(
                        session_id = %session_id_for_progress,
                        error = %e,
                        "Failed to send progress event (no receivers)"
                    );
                }
            }
        });

        let task_handle = tokio::spawn(async move {
            info!(session_id = %session_id_for_task, "Download started");

            let resume_from =
                if let Ok(Some(current_session)) = repository.get(&session_id_for_task).await {
                    match tokio::fs::metadata(&destination).await {
                        Ok(metadata) => {
                            let file_size = metadata.len();
                            let recorded = current_session.progress().bytes_downloaded();
                            let total = current_session.progress().total_bytes();
                            let offset = recover_resume_offset(recorded, file_size, total);

                            if let Some(offset) = offset {
                                info!(
                                    session_id = %session_id_for_task,
                                    recorded_bytes = recorded,
                                    file_bytes = file_size,
                                    "Resuming from authoritative partial-file length"
                                );
                                Some(offset)
                            } else {
                                warn!(
                                    session_id = %session_id_for_task,
                                    recorded_bytes = recorded,
                                    file_bytes = file_size,
                                    total_bytes = ?total,
                                    "Partial file failed resume consistency checks; restarting"
                                );
                                None
                            }
                        }
                        Err(_) => {
                            if current_session.progress().bytes_downloaded() > 0 {
                                warn!(
                                    session_id = %session_id_for_task,
                                    "Partial file missing, starting fresh"
                                );
                            }
                            None
                        }
                    }
                } else {
                    None
                };

            // `Some(result)` when the transfer ran to completion or failed;
            // `None` when we were told to stop. Note this branch no longer
            // returns early — doing so skipped the cleanup below, so the
            // session kept its slot in `active_downloads` forever. With a
            // concurrency limit of 2, pausing two files wedged the queue for
            // the rest of the process and leaked the auth token with it.
            let download_result = tokio::select! {
                result = engine.download(DownloadOptions {
                    url: url.clone(),
                    destination: destination.clone(),
                    resume_from,
                    progress_callback: Some(progress_callback),
                    auth_token,
                }) => Some(result),
                reason = cancel_rx.recv() => {
                    let reason = reason.unwrap_or(StopReason::Cancel);
                    info!(session_id = %session_id_for_task, ?reason, "Download stopped");

                    match reason {
                        StopReason::Pause => {
                            // Deliberately no state transition and no file
                            // removal. `pause_download` already wrote `Paused`;
                            // touching the session here is what used to turn a
                            // pause into a cancel. The partial file stays so
                            // resume can continue from its offset.
                        }
                        StopReason::Cancel => {
                            if let Ok(Some(mut session)) = repository.get(&session_id_for_task).await {
                                if let Err(e) = session.cancel() {
                                    error!(session_id = %session_id_for_task, error = %e, "Failed to mark session as cancelled");
                                }

                                if let Err(e) = repository.update(&session).await {
                                    error!(session_id = %session_id_for_task, error = %e, "Failed to update cancelled session in database");
                                }

                                if let Err(e) = event_tx.send(DownloadEvent::Cancelled {
                                    id: session_id_for_task.clone(),
                                }) {
                                    warn!(session_id = %session_id_for_task, error = %e, "Failed to send Cancelled event");
                                }
                            }
                        }
                    }

                    None
                }
            };

            let Some(download_result) = download_result else {
                // Stopped rather than finished: fall through to the shared
                // cleanup so the concurrency slot and auth token are released.
                let mut active = active_downloads.write().await;
                active.remove(&session_id_for_task);
                drop(active);

                let mut tokens = auth_tokens.write().await;
                tokens.remove(&session_id_for_task);
                return;
            };

            match download_result {
                Ok(result) => {
                    info!(
                        session_id = %session_id_for_task,
                        bytes = result.bytes_downloaded,
                        "Download completed successfully"
                    );

                    if let Ok(Some(mut session)) = repository.get(&session_id_for_task).await {
                        session.update_progress(result.bytes_downloaded, 0.0);

                        let expected_size = session
                            .progress()
                            .total_bytes()
                            .unwrap_or(result.bytes_downloaded);
                        if let Err(validation_error) =
                            validate_downloaded_file(&destination, expected_size).await
                        {
                            error!(
                                session_id = %session_id_for_task,
                                error = %validation_error,
                                "Downloaded file validation failed"
                            );

                            file_cleanup.delete_file_best_effort(&destination).await;

                            session.update_progress(0, 0.0);

                            if let Err(e) = session
                                .fail(format!("File validation failed: {}", validation_error))
                            {
                                error!(session_id = %session_id_for_task, error = %e, "Failed to mark session as failed");
                            }

                            if let Err(e) = repository.update(&session).await {
                                error!(session_id = %session_id_for_task, error = %e, "Failed to update session in database");
                            }

                            if let Err(e) = event_tx.send(DownloadEvent::Failed {
                                id: session_id_for_task.clone(),
                                error: format!("File validation failed: {}", validation_error),
                            }) {
                                warn!(session_id = %session_id_for_task, error = %e, "Failed to send Failed event");
                            }

                            return;
                        }

                        if let Some(expected_checksum) = &checksum {
                            if let Err(e) = expected_checksum.verify(&result.sha256_checksum) {
                                error!(
                                    session_id = %session_id_for_task,
                                    expected = expected_checksum.value(),
                                    actual = %result.sha256_checksum,
                                    "Checksum verification failed"
                                );

                                file_cleanup.delete_file_best_effort(&destination).await;

                                session.update_progress(0, 0.0);

                                if let Err(fail_err) =
                                    session.fail(format!("Checksum verification failed: {}", e))
                                {
                                    error!(session_id = %session_id_for_task, error = %fail_err, "Failed to mark session as failed");
                                }

                                if let Err(update_err) = repository.update(&session).await {
                                    error!(session_id = %session_id_for_task, error = %update_err, "Failed to update session in database");
                                }

                                if let Err(event_err) = event_tx.send(DownloadEvent::Failed {
                                    id: session_id_for_task.clone(),
                                    error: format!("Checksum mismatch: {}", e),
                                }) {
                                    warn!(session_id = %session_id_for_task, error = %event_err, "Failed to send Failed event");
                                }

                                return;
                            }
                        }

                        // Mark as completed
                        if let Err(e) = session.complete() {
                            error!(session_id = %session_id_for_task, error = %e, "Failed to mark session as completed");
                        }

                        if let Err(e) = repository.update(&session).await {
                            error!(session_id = %session_id_for_task, error = %e, "Failed to update session in database");
                        }

                        if let Err(e) = event_tx.send(DownloadEvent::Completed {
                            id: session_id_for_task.clone(),
                        }) {
                            warn!(session_id = %session_id_for_task, error = %e, "Failed to send Completed event");
                        }

                        info!(session_id = %session_id_for_task, "Download marked as completed");
                    }
                }
                Err(e) => {
                    error!(
                        session_id = %session_id_for_task,
                        error = %e,
                        "Download failed"
                    );

                    if let Ok(Some(mut session)) = repository.get(&session_id_for_task).await {
                        if let Err(fail_err) = session.fail(format!("{}", e)) {
                            error!(session_id = %session_id_for_task, error = %fail_err, "Failed to mark session as failed");
                        }

                        if let Err(update_err) = repository.update(&session).await {
                            error!(session_id = %session_id_for_task, error = %update_err, "Failed to update session in database");
                        }

                        if let Err(event_err) = event_tx.send(DownloadEvent::Failed {
                            id: session_id_for_task.clone(),
                            error: format!("{}", e),
                        }) {
                            warn!(session_id = %session_id_for_task, error = %event_err, "Failed to send Failed event");
                        }
                    }
                }
            }

            let mut active = active_downloads.write().await;
            active.remove(&session_id_for_task);

            let mut tokens = auth_tokens.write().await;
            tokens.remove(&session_id_for_task);
        });

        let mut active = self.active_downloads.write().await;
        active.insert(
            session_id.clone(),
            ActiveDownload {
                session,
                task_handle: Some(task_handle),
                cancel_tx: Some(cancel_tx),
            },
        );

        Ok(())
    }
}
