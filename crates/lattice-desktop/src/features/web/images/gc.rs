//! `ChatImageGc::sweep`, run at startup (§7.6): drops the rows no message
//! holds, with their files (except a resuming research turn's and pictures
//! staged since this start), then the files whose rows were let go of; the
//! second half also runs after a truncate or regenerate
//! (`remove_unclaimed_files`). A picture goes only when a record of its
//! release names it, with its prepared copies; an unfinished write goes once it
//! is older than an hour. A file no row and no record names is kept and
//! counted. It touches only direct children of the image directory with names
//! it recognises, and `prepared/` files. While the sweep gate is shut the file
//! passes touch no file and clear the records: a database that does not own
//! the files never acts on them.

use super::is_allowed_extension;
use super::repository::{ImageRepository, ImageRow};
use super::store::{is_valid_id, recognised_prepared, ImageStore, PART_SUFFIX};
use crate::features::web::snapshots::gc::STALE_PART_AGE;
use chrono::{DateTime, Utc};
use lattice_core::application::ports::chat_images::DRAFT_REQUEST_PREFIX;
use lattice_core::shared::error::Result;
use lattice_core::shared::fs::SweepGate;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImageSweepReport {
    /// Rows a turn leased and never attached, removed with their files.
    pub removed_leases: u64,
    /// Files their rows let go of, and unfinished writes older than an hour.
    pub removed_orphans: u64,
    pub retained: u64,
    pub bytes_freed: u64,
    /// Files no row and no record names: kept.
    pub unexplained: u64,
}

#[derive(Clone)]
pub struct ChatImageGc {
    store: ImageStore,
    repository: ImageRepository,
    /// When this process built the service: a draft staged since then
    /// belongs to a composer that is still open.
    started_at: DateTime<Utc>,
    gate: Arc<SweepGate>,
}

/// `(id, ext)` of a file name the store writes, finished or not.
fn recognised(name: &str) -> Option<(&str, &str)> {
    let name = name.strip_suffix(PART_SUFFIX).unwrap_or(name);
    let (id, ext) = name.split_once('.')?;
    (is_valid_id(id) && is_allowed_extension(ext)).then_some((id, ext))
}

/// Whether the file `meta` describes was last written an hour or more ago.
fn stale(meta: &std::fs::Metadata, now: SystemTime) -> bool {
    meta.modified()
        .ok()
        .and_then(|modified| now.duration_since(modified).ok())
        .is_some_and(|age| age >= STALE_PART_AGE)
}

/// Remove `path` if it is a file: the bytes it held, `None` if it was not there.
async fn remove_file(path: &Path) -> std::io::Result<Option<u64>> {
    let size = match tokio::fs::symlink_metadata(path).await {
        Ok(meta) if meta.is_file() => meta.len(),
        Ok(_) => return Ok(None),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(Some(size)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

impl ChatImageGc {
    pub fn new(store: ImageStore, repository: ImageRepository, gate: Arc<SweepGate>) -> Self {
        Self {
            store,
            repository,
            started_at: Utc::now(),
            gate,
        }
    }

    /// Drop the leases of requests not in `kept_requests`, and drafts staged
    /// before this start, recording their files as let go of, then the files
    /// recorded so.
    pub async fn sweep(&self, kept_requests: &HashSet<String>) -> Result<ImageSweepReport> {
        let ids: Vec<String> = self
            .repository
            .orphans_without_message()
            .await?
            .into_iter()
            .filter(|row| !kept_requests.contains(&row.request_id))
            .filter(|row| !self.staged_since_start(row))
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

    /// A composer draft written since this process started. An unreadable
    /// timestamp is treated as old: the row was not written by this process.
    fn staged_since_start(&self, row: &ImageRow) -> bool {
        row.request_id.starts_with(DRAFT_REQUEST_PREFIX)
            && DateTime::parse_from_rfc3339(&row.created_at)
                .is_ok_and(|created| created >= self.started_at)
    }

    /// Remove every picture a release record names that no row claims, with
    /// its unfinished write and its prepared copies, and every unfinished
    /// write older than an hour, once the sweep gate is open. Count the files
    /// no record explains, which stay.
    pub async fn remove_unclaimed_files(&self) -> Result<ImageSweepReport> {
        let mut report = ImageSweepReport::default();
        let released = self.repository.released().await?;
        if !self.gate.is_open() {
            tracing::debug!("Left the chat image files alone: this database does not own them");
            let records: Vec<i64> = released.into_values().flatten().collect();
            self.repository.forget_records(&records).await?;
            return Ok(report);
        }
        let root = self.store.root().to_path_buf();
        let mut settled: Vec<i64> = Vec::new();
        for (name, records) in &released {
            let Some((id, ext)) = recognised(name) else {
                settled.extend(records);
                continue;
            };
            // Ids are never reused, so a row here means the release did not
            // happen after all.
            match self.repository.get(id).await {
                Ok(Some(_)) => {
                    settled.extend(records);
                    continue;
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(%error, image_id = id, "Could not check a released picture's row; keeping it");
                    continue;
                }
            }
            let mut done = true;
            for path in [
                root.join(format!("{id}.{ext}")),
                root.join(format!("{id}.{ext}{PART_SUFFIX}")),
            ] {
                match remove_file(&path).await {
                    Ok(Some(bytes)) => {
                        report.removed_orphans += 1;
                        report.bytes_freed += bytes;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        tracing::warn!(%error, file = %path.display(), "Could not remove a chat image file");
                        report.retained += 1;
                        done = false;
                    }
                }
            }
            if !self.store.remove_prepared(id).await {
                report.retained += 1;
                done = false;
            }
            if done {
                settled.extend(records);
            }
        }
        if let Err(error) = self.repository.forget_records(&settled).await {
            tracing::warn!(%error, "Could not clear settled picture records; the next sweep clears them");
        }

        if tokio::fs::try_exists(&root).await.unwrap_or(false) {
            let live: HashSet<String> = self
                .repository
                .live_files()
                .await?
                .into_iter()
                .map(|(id, _)| id)
                .collect();
            self.walk(&root, &live, &mut report, |name| {
                recognised(name).map(|(id, _)| id)
            })
            .await?;
            let prepared = self.store.prepared_root();
            if tokio::fs::try_exists(&prepared).await.unwrap_or(false) {
                self.walk(&prepared, &live, &mut report, |name| {
                    recognised_prepared(name).map(|(id, ..)| id)
                })
                .await?;
            }
        }
        if report.unexplained > 0 {
            tracing::info!(
                count = report.unexplained,
                "{} chat picture files no record explains; kept",
                report.unexplained
            );
        }
        Ok(report)
    }

    /// One directory: unfinished writes older than an hour go; a finished
    /// file whose picture has no row is counted and kept.
    async fn walk(
        &self,
        dir: &Path,
        live: &HashSet<String>,
        report: &mut ImageSweepReport,
        picture_of: impl Fn(&str) -> Option<&str>,
    ) -> Result<()> {
        let now = SystemTime::now();
        let mut entries = tokio::fs::read_dir(dir).await?;
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
            let Some(id) = picture_of(name) else {
                report.retained += 1;
                continue;
            };
            if name.ends_with(PART_SUFFIX) {
                if !stale(&meta, now) {
                    report.retained += 1;
                    continue;
                }
                match tokio::fs::remove_file(entry.path()).await {
                    Ok(()) => {
                        report.bytes_freed += meta.len();
                        report.removed_orphans += 1;
                    }
                    Err(error) => {
                        tracing::warn!(%error, file = name, "Could not remove an unfinished chat image write");
                        report.retained += 1;
                    }
                }
                continue;
            }
            if !live.contains(id) && !matches!(self.repository.get(id).await, Ok(Some(_))) {
                report.unexplained += 1;
            }
            report.retained += 1;
        }
        Ok(())
    }
}
