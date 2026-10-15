//! The `web_page_snapshot` job: capture, write, commit `ready` with the hash
//! (§10). Idempotent: a row that is no longer pending is left as it is.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;

use super::dto::WebPageSnapshotStatus;
use super::repository::{ReadyCapture, SnapshotRow};
use super::{to_dto, Shared};
use crate::features::web::traits::page_snapshot::{
    CaptureError, CaptureProgress, CaptureRequest, ProgressSink,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::{
    Backoff, JobContext, JobHandler, JobKindConfig, JobOutcome, RecoveryPolicy,
};

pub const KIND: &str = "web_page_snapshot";

/// Progress reaches the database and the window at most this often (4/s).
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

pub fn job_config() -> JobKindConfig {
    JobKindConfig::new(RecoveryPolicy::Requeue)
        .concurrency(2)
        .exclusive_per_subject()
        .backoff(Backoff {
            initial: Duration::from_secs(20),
            max: Duration::from_secs(10 * 60),
        })
}

/// Total 100: 10 after the document, resources proportionally up to 94,
/// 95 while writing, 100 on commit.
pub(crate) fn progress_units(progress: &CaptureProgress) -> u32 {
    if progress.total == 0 {
        return 10;
    }
    let done = progress.done.min(progress.total) as u64;
    10 + ((84 * done) / progress.total as u64) as u32
}

pub struct WebPageSnapshotJob {
    shared: Arc<Shared>,
}

impl WebPageSnapshotJob {
    pub(crate) fn new(shared: Arc<Shared>) -> Self {
        Self { shared }
    }

    /// Marks the row failed, tells the window, and ends the job as failed.
    async fn fail(&self, id: &str, code: &str, message: &str) -> Result<JobOutcome> {
        if let Err(error) = self.shared.repository.mark_failed(id, code, message).await {
            tracing::warn!(%error, snapshot_id = id, "Could not mark a web snapshot failed");
        }
        self.shared.announce(id).await;
        Ok(JobOutcome::Failed {
            code: code.to_string(),
            message: message.to_string(),
        })
    }

    async fn commit(
        &self,
        id: &str,
        page: crate::features::web::traits::page_snapshot::CapturedPage,
    ) -> Result<JobOutcome> {
        let (sha256, byte_size) = match self.shared.store.write(id, page.html.into_bytes()).await {
            Ok(written) => written,
            Err(error) => return self.fail(id, "storage", &error.to_string()).await,
        };
        let ready = ReadyCapture {
            final_url: page.final_url,
            title: page.title,
            method: page.method.as_str(),
            captured_at: page.captured_at.to_rfc3339(),
            byte_size,
            sha256,
            resource_count: page.resources.inlined,
            missing_resource_count: page.resources.dropped.len() as u32,
            partial: page.partial,
            partial_reasons: page.partial_reasons,
            favicon_data_url: page.favicon.as_ref().map(|icon| icon.to_data_url()),
        };
        match self.shared.repository.mark_ready(id, &ready).await {
            Ok(true) => {}
            Ok(false) => {
                // Deleted with its conversation while capturing, or settled by
                // someone else: never keep a file no row owns.
                if matches!(self.shared.repository.get(id).await, Ok(None)) {
                    self.shared.store.remove(id).await;
                }
                return Ok(JobOutcome::Completed {
                    result_ref: id.to_string(),
                    message: "The snapshot was settled elsewhere".into(),
                });
            }
            Err(error) => {
                self.shared.store.remove(id).await;
                return self.fail(id, "storage", &error.to_string()).await;
            }
        }
        self.shared.announce(id).await;
        Ok(JobOutcome::Completed {
            result_ref: id.to_string(),
            message: "Captured".into(),
        })
    }
}

#[async_trait]
impl JobHandler for WebPageSnapshotJob {
    async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
        let id = context
            .subject_id()
            .map(str::to_string)
            .or_else(|| {
                context
                    .job()
                    .requested
                    .get("snapshotId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_default();
        let Some(row) = self.shared.repository.get(&id).await? else {
            return Ok(JobOutcome::Completed {
                result_ref: id,
                message: "The snapshot no longer exists".into(),
            });
        };
        if row.status() != WebPageSnapshotStatus::Pending {
            return Ok(JobOutcome::Completed {
                result_ref: id,
                message: "Already settled".into(),
            });
        }
        let _ = context.progress(0, "Fetching the page", None).await;

        let Some(result) = context.until_cancelled(self.capture(context, &row)).await else {
            return Ok(JobOutcome::Stopped);
        };
        match result {
            Ok(page) => {
                let _ = context.progress(95, "Saving the frozen copy", None).await;
                let outcome = self.commit(&id, page).await?;
                if matches!(outcome, JobOutcome::Completed { .. }) {
                    let _ = context.progress(100, "Captured", None).await;
                }
                Ok(outcome)
            }
            Err(CaptureError::Cancelled) => Ok(JobOutcome::Stopped),
            // The runtime defers an outage and redelivers after the backoff;
            // the row stays pending.
            Err(error) if error.is_transient() => Err(AppError::Network(error.to_string())),
            Err(error) => self.fail(&id, error.code(), &error.to_string()).await,
        }
    }
}

impl WebPageSnapshotJob {
    /// Runs the capture while forwarding its progress, debounced.
    async fn capture(
        &self,
        context: &JobContext,
        row: &SnapshotRow,
    ) -> std::result::Result<crate::features::web::traits::page_snapshot::CapturedPage, CaptureError>
    {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<CaptureProgress>();
        let sink: ProgressSink = Arc::new(move |progress| {
            let _ = sender.send(progress);
        });
        let request = CaptureRequest {
            url: row.requested_url.clone(),
            snapshot_id: row.id.clone(),
        };
        let capture = self
            .shared
            .capture
            .capture(request, sink, context.cancellation().clone());
        tokio::pin!(capture);
        let mut open = true;
        let mut last: Option<Instant> = None;
        loop {
            tokio::select! {
                result = &mut capture => return result,
                progress = receiver.recv(), if open => {
                    let Some(progress) = progress else {
                        open = false;
                        continue;
                    };
                    if last.is_some_and(|at| at.elapsed() < PROGRESS_INTERVAL) {
                        continue;
                    }
                    last = Some(Instant::now());
                    let _ = context
                        .progress(progress_units(&progress), &progress.message, None)
                        .await;
                    self.shared
                        .emit_changed(to_dto(row, Some(progress.message), None));
                }
            }
        }
    }
}
