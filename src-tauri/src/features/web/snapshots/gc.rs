//! `WebSnapshotGc::sweep`: drops `.part` files older than an hour and `.html`
//! files no pending or ready row claims. Touches nothing but direct children
//! of the snapshot directory whose names it recognises (§7, T14).

use std::time::{Duration, SystemTime};

use super::repository::SnapshotRepository;
use super::store::{is_valid_id, SnapshotStore, HTML_SUFFIX, PART_SUFFIX};
use lattice_core::shared::error::Result;

/// How old an unfinished write must be before it is treated as abandoned.
pub const STALE_PART_AGE: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SweepReport {
    pub removed_parts: u64,
    pub removed_orphans: u64,
    pub retained: u64,
    pub bytes_freed: u64,
}

#[derive(Clone)]
pub struct WebSnapshotGc {
    store: SnapshotStore,
    repository: SnapshotRepository,
}

impl WebSnapshotGc {
    pub fn new(store: SnapshotStore, repository: SnapshotRepository) -> Self {
        Self { store, repository }
    }

    pub async fn sweep(&self) -> Result<SweepReport> {
        let root = self.store.root().to_path_buf();
        if !tokio::fs::try_exists(&root).await.unwrap_or(false) {
            return Ok(SweepReport::default());
        }
        let live = self.repository.live_ids().await?;
        let now = SystemTime::now();
        let mut report = SweepReport::default();
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
            let remove = if let Some(id) = name.strip_suffix(PART_SUFFIX) {
                let age = meta
                    .modified()
                    .ok()
                    .and_then(|modified| now.duration_since(modified).ok())
                    .unwrap_or_default();
                is_valid_id(id) && age >= STALE_PART_AGE
            } else if let Some(id) = name.strip_suffix(HTML_SUFFIX) {
                is_valid_id(id) && !live.contains(id)
            } else {
                false
            };
            if !remove {
                report.retained += 1;
                continue;
            }
            match tokio::fs::remove_file(entry.path()).await {
                Ok(()) => {
                    report.bytes_freed += meta.len();
                    if name.ends_with(PART_SUFFIX) {
                        report.removed_parts += 1;
                    } else {
                        report.removed_orphans += 1;
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, file = name, "Could not remove a web snapshot file");
                    report.retained += 1;
                }
            }
        }
        Ok(report)
    }
}
