//! Queue draining: promote pending downloads while concurrency slots are free.

use super::state::DownloadManagerService;
use crate::domain::download::DownloadError;
use tracing::{debug, error};

impl DownloadManagerService {
    pub(super) async fn process_queue(&self) -> Result<(), DownloadError> {
        loop {
            // Count all active downloads (not just Downloading state)
            let active_count = {
                let active = self.active_downloads.read().await;
                active.len()
            };

            if active_count >= self.max_concurrent_downloads {
                debug!(
                    active = active_count,
                    max = self.max_concurrent_downloads,
                    "Download queue processing stopped: max concurrent limit reached"
                );
                break;
            }

            let next_id = {
                let mut queue = self.download_queue.write().await;
                queue.pop_front()
            };

            match next_id {
                Some(id) => {
                    debug!(id = %id, "Starting download from queue");

                    // Don't propagate error - log it and continue processing remaining items
                    if let Err(e) = self.start_download_task(&id).await {
                        error!(
                            download_id = %id,
                            error = %e,
                            "Failed to start download task - marking as failed and continuing queue processing"
                        );

                        // Mark the download as failed in database since it was popped from queue
                        if let Ok(Some(mut session)) = self.repository.get(&id).await {
                            if let Err(fail_err) = session.fail(format!("Failed to start: {}", e)) {
                                error!(
                                    download_id = %id,
                                    error = %fail_err,
                                    "Failed to mark download as failed"
                                );
                            } else if let Err(update_err) = self.repository.update(&session).await {
                                error!(
                                    download_id = %id,
                                    error = %update_err,
                                    "Failed to update download status in database"
                                );
                            }
                        }

                        // Continue processing remaining items in queue
                        continue;
                    }
                }
                None => {
                    debug!("Download queue is empty");
                    break;
                }
            }
        }

        Ok(())
    }
}
