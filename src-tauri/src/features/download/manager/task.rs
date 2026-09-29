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

const CONTROL_EVENT_DELIVERY_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(250);

/// Control events are snapshots of state already committed to the repository.
/// They must never hold the queue gate indefinitely when the UI bridge stalls.
pub(super) async fn send_control_event(
    event_tx: &mpsc::Sender<DownloadEvent>,
    shutdown: &tokio_util::sync::CancellationToken,
    event: DownloadEvent,
) {
    let result = tokio::select! {
        biased;
        _ = shutdown.cancelled() => return,
        result = tokio::time::timeout(CONTROL_EVENT_DELIVERY_TIMEOUT, event_tx.send(event)) => {
            match result {
                Ok(result) => result,
                Err(_) => {
                    warn!("Timed out sending download state notification; repository state remains authoritative");
                    return;
                }
            }
        }
    };

    if let Err(error) = result {
        warn!(%error, "Download event bridge is closed; repository state remains authoritative");
    }
}

/// Terminal notifications drive model-download saga updates, so give them a
/// bounded chance to reach the bridge without retaining waiting tasks. The
/// session row is authoritative if delivery times out.
pub(super) async fn send_terminal_event(
    event_tx: &mpsc::Sender<DownloadEvent>,
    shutdown: &tokio_util::sync::CancellationToken,
    event: DownloadEvent,
) {
    let result = tokio::select! {
        biased;
        _ = shutdown.cancelled() => {
            warn!("Application shutdown interrupted terminal download notification; refresh from repository state to recover");
            return;
        }
        result = tokio::time::timeout(CONTROL_EVENT_DELIVERY_TIMEOUT, event_tx.send(event)) => {
            match result {
                Ok(result) => result,
                Err(_) => {
                    warn!("Timed out sending terminal download notification; refresh from repository state to recover");
                    return;
                }
            }
        }
    };

    if let Err(error) = result {
        warn!(%error, "Download event bridge is closed; terminal repository state remains authoritative");
    }
}

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

        // Latest progress occupies one slot regardless of callback rate or
        // database latency. Persistence happens in the transfer task itself.
        let (progress_tx, mut progress_rx) = tokio::sync::watch::channel(None::<(u64, f64)>);
        let progress_callback: ProgressCallback = Arc::new(move |bytes, speed| {
            progress_tx.send_replace(Some((bytes, speed)));
        });
        let shutdown = self.shutdown.clone();
        // Register before the worker can finish/remove itself.
        let mut active = self.active_downloads.write().await;
        let task_handle = crate::shared::background::spawn(async move {
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
            let transfer = engine.download(DownloadOptions {
                url: url.clone(),
                destination: destination.clone(),
                resume_from,
                progress_callback: Some(progress_callback),
                auth_token,
            });
            tokio::pin!(transfer);
            let mut progress_tick = tokio::time::interval(std::time::Duration::from_millis(250));
            progress_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let download_result = loop {
                tokio::select! {
                    biased;
                    _ = shutdown.cancelled() => {
                        break None;
                    }
                    result = &mut transfer => break Some(result),
                    _ = progress_tick.tick() => {
                        if progress_rx.has_changed().unwrap_or(false) {
                            let latest = *progress_rx.borrow_and_update();
                            if let Some((bytes, speed)) = latest {
                                if let Err(error) = repository.update_progress(&session_id_for_task, bytes, speed).await {
                                    error!(%error, "Could not persist download progress");
                                }
                                let _ = event_tx.try_send(DownloadEvent::Progress {
                                    id: session_id_for_task.clone(), bytes_downloaded: bytes, bytes_per_second: speed,
                                });
                            }
                        }
                    }
                    reason = cancel_rx.recv() => {
                        let reason = reason.unwrap_or(StopReason::Cancel);
                        info!(session_id = %session_id_for_task, ?reason, "Download stopped");

                        // Caller joins this task, then persists pause/cancel with
                    // the final progress. It cannot race a resumed transfer.
                    break None;
                    }
                }
            };
            // No progress work can run after the terminal transition. Flush
            // the last callback even if the transfer finishes between ticks.
            let latest = *progress_rx.borrow_and_update();
            if let Some((bytes, speed)) = latest {
                if let Err(error) = repository
                    .update_progress(&session_id_for_task, bytes, speed)
                    .await
                {
                    error!(%error, "Could not flush final download progress");
                }
            }

            let Some(download_result) = download_result else {
                if shutdown.is_cancelled() {
                    if let Ok(Some(mut session)) = repository.get(&session_id_for_task).await {
                        if session.state().is_active() {
                            let _ = session.pause();
                            let _ = repository.update(&session).await;
                        }
                    }
                }
                // Stopped rather than finished: fall through to the shared
                // cleanup so the concurrency slot and auth token are released.
                let mut active = active_downloads.write().await;
                active.remove(&session_id_for_task);
                drop(active);

                let mut tokens = auth_tokens.write().await;
                tokens.remove(&session_id_for_task);
                return;
            };

            let mut terminal_event = None;
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

                            active_downloads.write().await.remove(&session_id_for_task);
                            auth_tokens.write().await.remove(&session_id_for_task);
                            send_terminal_event(
                                &event_tx,
                                &shutdown,
                                DownloadEvent::Failed {
                                    id: session_id_for_task.clone(),
                                    error: format!("File validation failed: {}", validation_error),
                                },
                            )
                            .await;
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

                                active_downloads.write().await.remove(&session_id_for_task);
                                auth_tokens.write().await.remove(&session_id_for_task);
                                send_terminal_event(
                                    &event_tx,
                                    &shutdown,
                                    DownloadEvent::Failed {
                                        id: session_id_for_task.clone(),
                                        error: format!("Checksum mismatch: {}", e),
                                    },
                                )
                                .await;
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

                        terminal_event = Some(DownloadEvent::Completed {
                            id: session_id_for_task.clone(),
                        });

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

                        terminal_event = Some(DownloadEvent::Failed {
                            id: session_id_for_task.clone(),
                            error: format!("{}", e),
                        });
                    }
                }
            }

            let mut active = active_downloads.write().await;
            active.remove(&session_id_for_task);

            let mut tokens = auth_tokens.write().await;
            tokens.remove(&session_id_for_task);
            drop(tokens);

            // Publish only after the active slot and auth token are released.
            // The bridge can now safely promote queued work on this terminal
            // event without racing the worker's cleanup.
            if let Some(event) = terminal_event {
                send_terminal_event(&event_tx, &shutdown, event).await;
            }
        });

        if task_handle.is_none() {
            // Admission can close after the session was marked Downloading.
            // The queue caller marks the durable row failed; release its
            // per-session credential here because no worker will own cleanup.
            self.auth_tokens.write().await.remove(&session_id);
            return Err(DownloadError::NetworkError(
                "Application is shutting down".into(),
            ));
        }
        active.insert(
            session_id.clone(),
            ActiveDownload {
                session,
                task_handle,
                cancel_tx: Some(cancel_tx),
            },
        );

        // Register the live worker before attempting UI notification. A full
        // event channel must never leave a Downloading row with no owner.
        if let Err(error) = self.event_tx.try_send(DownloadEvent::Started {
            id: session_id.clone(),
        }) {
            warn!(
                download_id = %session_id,
                %error,
                "Could not queue Started notification; repository state remains authoritative"
            );
        }

        Ok(())
    }
}
