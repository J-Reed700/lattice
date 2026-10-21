//! The library's indexer as the activity view lists it: files being added,
//! with the pace it measured over the files done so far.

use super::engine::{IndexStatus, IndexingState};
use async_trait::async_trait;
use lattice_core::application::ports::{
    ActivityItem, ActivityProgress, ActivityResource, ActivitySource, ActivityState,
};
use lattice_core::shared::Result;
use std::path::Path;
use std::sync::Arc;

pub struct LibraryActivity {
    state: Arc<IndexingState>,
}

impl LibraryActivity {
    pub fn new(state: Arc<IndexingState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl ActivitySource for LibraryActivity {
    async fn activity(&self) -> Result<Vec<ActivityItem>> {
        let progress = self.state.get_snapshot();
        let scanning = match progress.status {
            IndexStatus::Scanning => true,
            IndexStatus::Processing => false,
            _ => return Ok(Vec::new()),
        };
        let paused = self.state.is_paused();
        let state = if paused {
            ActivityState::Paused
        } else {
            ActivityState::Running
        };
        let mut item = ActivityItem::new(
            "library.indexing",
            "library.indexing",
            "Adding files to your library",
            state,
        )
        .subject("your library");
        if paused {
            return Ok(vec![item.detail("Paused until you resume it")]);
        }
        if scanning {
            return Ok(vec![item.detail("Looking for files")]);
        }
        let reading = progress
            .current_file
            .as_deref()
            .and_then(|file| Path::new(file).file_name())
            .map(|name| format!("Reading {}", name.to_string_lossy()));
        item = item
            .detail(reading.unwrap_or_else(|| "Reading files".to_string()))
            .progress(Some(ActivityProgress::of(
                progress.processed as u64,
                progress.total_files as u64,
                "files",
            )))
            .uses(ActivityResource::EmbeddingModel);
        // The indexer's own time left, from its average over the files done.
        if progress.processed > 0 {
            item.seconds_left = progress.estimated_remaining_ms.map(|ms| ms.div_ceil(1000));
        }
        Ok(vec![item])
    }
}
