//! `ChatImageGc::sweep`, run at startup (§7.6): drops rows no message holds
//! with their files — a turn cannot survive a restart, except a research turn
//! that resumes, whose request the caller keeps — then every file no row
//! claims. That second half also runs alone after a truncate or regenerate
//! (`remove_unclaimed_files`). Touches nothing but direct children of the
//! image directory whose names it recognises: `<id>.<ext>` and
//! `<id>.<ext>.part`.

use super::is_allowed_extension;
use super::repository::ImageRepository;
use super::store::{is_valid_id, ImageStore, PART_SUFFIX};
use lattice_core::shared::error::Result;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImageSweepReport {
    /// Rows a turn leased and never attached, removed with their files.
    pub removed_leases: u64,
    /// Files no row claims, including unfinished writes.
    pub removed_orphans: u64,
    pub retained: u64,
    pub bytes_freed: u64,
}

#[derive(Clone)]
pub struct ChatImageGc {
    store: ImageStore,
    repository: ImageRepository,
}

/// `(id, ext)` of a file name the store writes, finished or not.
fn recognised(name: &str) -> Option<(&str, &str)> {
    let name = name.strip_suffix(PART_SUFFIX).unwrap_or(name);
    let (id, ext) = name.split_once('.')?;
    (is_valid_id(id) && is_allowed_extension(ext)).then_some((id, ext))
}

impl ChatImageGc {
    pub fn new(store: ImageStore, repository: ImageRepository) -> Self {
        Self { store, repository }
    }

    /// Drop the leases of requests not in `kept_requests`, with their files,
    /// then every file no row claims.
    pub async fn sweep(&self, kept_requests: &HashSet<String>) -> Result<ImageSweepReport> {
        let ids: Vec<String> = self
            .repository
            .orphans_without_message()
            .await?
            .into_iter()
            .filter(|row| !kept_requests.contains(&row.request_id))
            .map(|row| row.id)
            .collect();
        let removed_leases = if ids.is_empty() {
            0
        } else {
            self.repository.delete_ids(&ids).await?
        };
        Ok(ImageSweepReport {
            removed_leases,
            ..self.remove_unclaimed_files().await?
        })
    }

    /// Remove every recognised file no row claims.
    pub async fn remove_unclaimed_files(&self) -> Result<ImageSweepReport> {
        let mut report = ImageSweepReport::default();
        let root = self.store.root().to_path_buf();
        if !tokio::fs::try_exists(&root).await.unwrap_or(false) {
            return Ok(report);
        }
        let live = self.repository.live_files().await?;
        let mut entries = tokio::fs::read_dir(&root).await?;
        while let Some(entry) = entries.next_entry().await? {
            let Ok(meta) = tokio::fs::symlink_metadata(entry.path()).await else {
                continue;
            };
            if !meta.is_file() {
                continue;
            }
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some((id, ext)) = recognised(name) else {
                report.retained += 1;
                continue;
            };
            let finished = !name.ends_with(PART_SUFFIX);
            if finished && live.contains(&(id.to_string(), ext.to_string())) {
                report.retained += 1;
                continue;
            }
            // A row is always written before its file, so a row that
            // appeared since `live` was read still claims the file.
            if let Ok(Some(row)) = self.repository.get(id).await {
                if row.ext == ext && (finished || row.message_id.is_none()) {
                    report.retained += 1;
                    continue;
                }
            }
            match tokio::fs::remove_file(entry.path()).await {
                Ok(()) => {
                    report.bytes_freed += meta.len();
                    report.removed_orphans += 1;
                }
                Err(error) => {
                    tracing::warn!(%error, file = name, "Could not remove a chat image file");
                    report.retained += 1;
                }
            }
        }
        Ok(report)
    }
}
