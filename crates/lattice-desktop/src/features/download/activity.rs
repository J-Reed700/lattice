//! Model downloads as the activity view lists them: one line per model,
//! however many files it is, with the speed the transfers measured.

use async_trait::async_trait;
use lattice_core::application::ports::download_manager::DownloadManager;
use lattice_core::application::ports::{
    ActivityItem, ActivityProgress, ActivitySource, ActivityState,
};
use lattice_core::domain::download::{DownloadSession, DownloadState};
use lattice_core::shared::{AppError, Result};
use std::sync::Arc;

pub struct DownloadActivity {
    manager: Arc<dyn DownloadManager>,
}

impl DownloadActivity {
    pub fn new(manager: Arc<dyn DownloadManager>) -> Self {
        Self { manager }
    }
}

/// What the user knows a download by: its model, else its file.
fn name_of(session: &DownloadSession) -> String {
    session
        .model_name()
        .or(session.model_file_name())
        .map(str::to_string)
        .or_else(|| {
            session
                .destination()
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "a model".to_string())
}

/// One model's files, finished ones included, as one item. `None` once
/// none of them is still to do.
pub(super) fn download_item(files: &[DownloadSession]) -> Option<ActivityItem> {
    let live: Vec<&DownloadSession> = files
        .iter()
        .filter(|file| !file.state().is_terminal())
        .collect();
    let first = live.first()?;
    let counted: Vec<&DownloadSession> = files
        .iter()
        .filter(|file| {
            !matches!(
                file.state(),
                DownloadState::Failed | DownloadState::Cancelled
            )
        })
        .collect();
    let downloading: Vec<&&DownloadSession> = live
        .iter()
        .filter(|file| *file.state() == DownloadState::Downloading)
        .collect();
    let paused = live
        .iter()
        .all(|file| *file.state() == DownloadState::Paused);
    let state = if !downloading.is_empty() {
        ActivityState::Running
    } else if paused {
        ActivityState::Paused
    } else {
        ActivityState::Waiting
    };
    let name = name_of(first);
    let id = first.model_id().unwrap_or(first.id());
    let mut item = ActivityItem::new(
        format!("download:{id}"),
        "download",
        format!("Downloading {name}"),
        state,
    )
    .subject(name)
    .queue("download")
    .since(Some(first.created_at().timestamp_millis()));
    item = match state {
        ActivityState::Paused => item.detail("Paused"),
        ActivityState::Waiting => item.detail("Waiting to start"),
        ActivityState::Running if counted.len() > 1 => {
            let done = counted
                .iter()
                .filter(|file| *file.state() == DownloadState::Completed)
                .count();
            item.detail(format!("{done} of {} files done", counted.len()))
        }
        ActivityState::Running => item,
    };
    // Bytes only when every file's size is known, so the bar never runs
    // backwards as a sizeless file joins.
    let totals: Option<u64> = counted
        .iter()
        .map(|file| file.progress().total_bytes())
        .sum();
    if let Some(total) = totals {
        let done: u64 = counted
            .iter()
            .map(|file| file.progress().bytes_downloaded())
            .sum();
        item = item.progress(Some(ActivityProgress::of(done, total, "bytes")));
    }
    if state == ActivityState::Running {
        let speed: f64 = downloading
            .iter()
            .map(|file| file.progress().bytes_per_second())
            .sum();
        item = item.pace(Some(speed));
    }
    Some(item)
}

#[async_trait]
impl ActivitySource for DownloadActivity {
    async fn activity(&self) -> Result<Vec<ActivityItem>> {
        let sessions = self
            .manager
            .list_downloads()
            .await
            .map_err(|error| AppError::InternalError(error.to_string()))?;
        // A model's files share its id; a file of no model stands alone.
        let mut models: Vec<(String, Vec<DownloadSession>)> = Vec::new();
        for session in sessions {
            let key = session
                .model_id()
                .map_or_else(|| format!("file:{}", session.id()), str::to_string);
            match models.iter_mut().find(|(seen, _)| *seen == key) {
                Some((_, files)) => files.push(session),
                None => models.push((key, vec![session])),
            }
        }
        Ok(models
            .iter()
            .filter_map(|(_, files)| download_item(files))
            .collect())
    }
}
