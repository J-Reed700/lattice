//! Runs a batch import as a job: one item at a time, in batch order, each
//! item's outcome saved before the next starts. The runtime owns delivery,
//! cancellation and recovery; a stopped import resumes from its pending items.
use super::items::{BatchItem, BatchItems, ItemState};
use crate::shared::{
    error::Result,
    runtime::jobs::{JobContext, JobHandler, JobKindConfig, JobOutcome, JobStatus, RecoveryPolicy},
};
use async_trait::async_trait;
use std::sync::Arc;

/// What importing one item came to.
pub(super) enum ItemResult {
    Imported {
        document_id: String,
    },
    /// `document_id` is a document the item committed before a later step
    /// failed; its retry finishes that step.
    Failed {
        error: String,
        document_id: Option<String>,
    },
    /// The job was cancelled or the app is closing; nothing was committed.
    Stopped,
}

/// Imports one item of a batch.
#[async_trait]
pub(super) trait ItemImporter: Send + Sync + 'static {
    /// Called once per attempt, before the first item: loads what every item
    /// needs, or fails the attempt.
    async fn prepare(&self, context: &JobContext) -> Result<()>;

    async fn import(&self, context: &JobContext, item: &BatchItem) -> Result<ItemResult>;

    /// "file" or "URL", for progress messages.
    fn noun(&self) -> &'static str;
}

/// Imports run alongside each other, as they always have: contention for the
/// embedding model is that model's scheduler's decision. Items are durable, so
/// an import resumes from its pending items after a restart.
pub(super) fn import_job_config() -> JobKindConfig {
    JobKindConfig::new(RecoveryPolicy::Requeue)
}

pub(super) struct BatchImportJob<I> {
    pub items: BatchItems,
    pub importer: I,
}

fn plural(count: u32, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

#[async_trait]
impl<I: ItemImporter> JobHandler for BatchImportJob<I> {
    async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
        let job_id = context.id();
        self.items.reset_processing(job_id).await?;
        self.importer.prepare(context).await?;
        let noun = self.importer.noun();
        loop {
            if context.is_cancelled() {
                return Ok(JobOutcome::Stopped);
            }
            let Some(item) = self.items.claim_next(job_id).await? else {
                break;
            };
            match self.importer.import(context, &item).await {
                Ok(ItemResult::Imported { document_id }) => {
                    self.items
                        .finish(&item.id, ItemState::Completed, Some(&document_id), None)
                        .await?
                }
                Ok(ItemResult::Failed { error, document_id }) => {
                    tracing::warn!(job_id, target = %item.target, %error, "Batch import item failed");
                    self.items
                        .finish(
                            &item.id,
                            ItemState::Failed,
                            document_id.as_deref(),
                            Some(&error),
                        )
                        .await?
                }
                Ok(ItemResult::Stopped) => {
                    let cancelled =
                        context.store().get(job_id).await?.status == JobStatus::Cancelled;
                    let state = if cancelled {
                        ItemState::Cancelled
                    } else {
                        ItemState::Pending
                    };
                    self.items.release(&item.id, state).await?;
                    return Ok(JobOutcome::Stopped);
                }
                Err(error) => {
                    // The attempt fails; the item resumes with the next one.
                    self.items.release(&item.id, ItemState::Pending).await?;
                    return Err(error);
                }
            }
            let counts = self.items.counts_of(job_id).await?;
            let activity = serde_json::json!({
                "completed": counts.completed,
                "failed": counts.failed,
            });
            let message = format!(
                "Imported {} of {}",
                counts.processed(),
                plural(counts.total, noun)
            );
            if !context
                .progress(
                    counts.processed().min(context.job().progress_total),
                    &message,
                    Some(&activity),
                )
                .await?
            {
                return Ok(JobOutcome::Stopped);
            }
        }
        let counts = self.items.counts_of(job_id).await?;
        if counts.failed > 0 {
            return Ok(JobOutcome::Failed {
                code: "items_failed".into(),
                message: format!(
                    "{} of {} could not be imported",
                    counts.failed,
                    plural(counts.total, noun)
                ),
            });
        }
        Ok(JobOutcome::Completed {
            result_ref: job_id.to_string(),
            message: format!("Imported {}", plural(counts.completed, noun)),
        })
    }
}

/// Registers both import kinds with the runtime, settling imports the last
/// process left running.
pub(super) async fn register(
    jobs: &Arc<crate::shared::runtime::jobs::JobRuntime>,
    items: &BatchItems,
    files: super::file_job::FileImporter,
    urls: super::url_job::UrlImporter,
) -> Result<()> {
    jobs.register(
        super::imports::FILE_IMPORT,
        Arc::new(BatchImportJob {
            items: items.clone(),
            importer: files,
        }),
        import_job_config(),
    )
    .await?;
    jobs.register(
        super::imports::URL_IMPORT,
        Arc::new(BatchImportJob {
            items: items.clone(),
            importer: urls,
        }),
        import_job_config(),
    )
    .await
}
