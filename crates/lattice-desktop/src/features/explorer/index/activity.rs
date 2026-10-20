//! The folder index builds as the activity view lists them: named by their
//! folder, with the passages embedded and the pace each build measured.

use super::build::{activity_status, BuildRequest, FOLDER_INDEX};
use super::dto::{FolderIndexState, FolderIndexStatusDto};
use super::manager::FolderIndexManager;
use async_trait::async_trait;
use lattice_core::application::ports::{
    ActivityItem, ActivityProgress, ActivityResource, ActivitySource, ActivityTarget,
};
use lattice_core::shared::runtime::jobs::{JobRecord, JobStatus};
use lattice_core::shared::Result;
use std::path::Path;
use std::sync::Arc;

/// Explorer's live folder builds.
pub struct FolderActivity {
    manager: Arc<FolderIndexManager>,
}

impl FolderActivity {
    pub fn new(manager: Arc<FolderIndexManager>) -> Self {
        Self { manager }
    }

    fn item(&self, job: &JobRecord) -> ActivityItem {
        let request = BuildRequest::of(job);
        let root = request.as_ref().map_or_else(
            || job.subject_id.clone().unwrap_or_default(),
            |r| r.root.clone(),
        );
        let name = Path::new(&root)
            .file_name()
            .map_or_else(|| root.clone(), |name| name.to_string_lossy().into_owned());
        let index_dir = request
            .as_ref()
            .map(|request| self.manager.index_dir_of(Path::new(&request.index_root)));
        let item = ActivityItem::of_job(job, format!("Indexing {name}"))
            .subject(name.clone())
            .queue(FOLDER_INDEX)
            .index_dir(index_dir)
            .target(ActivityTarget::Folder { root, name });
        match activity_status(job) {
            Some(status) if job.status == JobStatus::Running => running(item, &status),
            _ => item,
        }
    }
}

/// A running build's words, its passages and its measured pace and time
/// left.
fn running(item: ActivityItem, status: &FolderIndexStatusDto) -> ActivityItem {
    match status.state {
        FolderIndexState::Scanning => item.detail(match status.files_total {
            0 => "Scanning the folder".to_string(),
            1 => "Scanning the folder: 1 file found".to_string(),
            found => format!("Scanning the folder: {found} files found"),
        }),
        FolderIndexState::Indexing => {
            let mut item = item
                .detail(
                    status
                        .message
                        .clone()
                        .unwrap_or_else(|| "Embedding passages".to_string()),
                )
                .progress(Some(ActivityProgress::of(
                    u64::from(status.passages_embedded),
                    u64::from(status.passages_total),
                    "passages",
                )))
                .pace(status.passages_per_second.map(f64::from))
                .uses(ActivityResource::EmbeddingModel);
            // The folder's own time left, from the pace it measured.
            if item.per_second.is_some() {
                item.seconds_left = status.eta_seconds.map(u64::from).or(item.seconds_left);
            }
            item
        }
        _ => item,
    }
}

#[async_trait]
impl ActivitySource for FolderActivity {
    async fn activity(&self) -> Result<Vec<ActivityItem>> {
        let jobs = self.manager.jobs.store().live(FOLDER_INDEX).await?;
        Ok(jobs.iter().map(|job| self.item(job)).collect())
    }
}
