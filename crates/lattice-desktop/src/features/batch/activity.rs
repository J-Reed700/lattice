//! The imports in flight, for the activity view.

use super::imports::{FILE_IMPORT, URL_IMPORT};
use async_trait::async_trait;
use lattice_core::application::ports::{ActivityItem, ActivityProgress, ActivitySource};
use lattice_core::shared::runtime::jobs::{JobRecord, JobRuntime};
use lattice_core::shared::Result;
use std::sync::Arc;

/// File and web-page imports, read from their jobs.
pub struct ImportActivity {
    jobs: Arc<JobRuntime>,
}

impl ImportActivity {
    pub fn new(jobs: Arc<JobRuntime>) -> Self {
        Self { jobs }
    }
}

/// "Importing 12 files", "Importing 1 web page".
fn import_item(job: &JobRecord, one: &str, many: &str) -> ActivityItem {
    let unit = if job.progress_total == 1 { one } else { many };
    ActivityItem::of_job(job, format!("Importing {} {unit}", job.progress_total)).progress(Some(
        ActivityProgress::of(
            u64::from(job.progress_current),
            u64::from(job.progress_total),
            many,
        ),
    ))
}

#[async_trait]
impl ActivitySource for ImportActivity {
    async fn activity(&self) -> Result<Vec<ActivityItem>> {
        let files = self.jobs.store().live(FILE_IMPORT).await?;
        let pages = self.jobs.store().live(URL_IMPORT).await?;
        Ok(files
            .iter()
            .map(|job| import_item(job, "file", "files"))
            .chain(
                pages
                    .iter()
                    .map(|job| import_item(job, "web page", "web pages")),
            )
            .collect())
    }
}
