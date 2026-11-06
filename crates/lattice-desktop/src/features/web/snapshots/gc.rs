//! `WebSnapshotGc::sweep`: drops `.part` files older than an hour, the pages a
//! delete recorded as let go of, and the pages of rows whose capture failed.
//! A page no row and no record names is kept and counted. Touches nothing but
//! direct children of the snapshot directory whose names it recognises (§7,
//! T14), and no file at all while the sweep gate is shut, when it clears the
//! records instead: a database that does not own the files never acts on them.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use super::repository::SnapshotRepository;
use super::store::{is_valid_id, SnapshotStore, HTML_SUFFIX, PART_SUFFIX};
use crate::infrastructure::storage::{FileIntents, FileStore};
use lattice_core::shared::error::Result;
use lattice_core::shared::fs::SweepGate;

/// How old an unfinished write must be before it is treated as abandoned.
pub const STALE_PART_AGE: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SweepReport {
    pub removed_parts: u64,
    /// Pages a delete let go of, and pages of failed captures.
    pub removed_orphans: u64,
    pub retained: u64,
    pub bytes_freed: u64,
    /// Pages no row and no record names: kept.
    pub unexplained: u64,
}

#[derive(Clone)]
pub struct WebSnapshotGc {
    store: SnapshotStore,
    repository: SnapshotRepository,
    intents: FileIntents,
    gate: Arc<SweepGate>,
}

impl WebSnapshotGc {
    pub fn new(
        store: SnapshotStore,
        repository: SnapshotRepository,
        intents: FileIntents,
        gate: Arc<SweepGate>,
    ) -> Self {
        Self {
            store,
            repository,
            intents,
            gate,
        }
    }

    pub async fn sweep(&self) -> Result<SweepReport> {
        let mut released: HashMap<String, Vec<i64>> = HashMap::new();
        for intent in self.intents.pending(FileStore::WebSnapshot).await? {
            released.entry(intent.name).or_default().push(intent.id);
        }
        if !self.gate.is_open() {
            tracing::debug!("Left the web snapshot files alone: this database does not own them");
            let records: Vec<i64> = released.into_values().flatten().collect();
            self.intents.forget(&records).await?;
            return Ok(SweepReport::default());
        }
        let live = self.repository.live_ids().await?;
        let failed = self.repository.failed_ids().await?;
        let mut kept_records: HashSet<String> = HashSet::new();
        let mut report = SweepReport::default();
        let root = self.store.root().to_path_buf();
        if tokio::fs::try_exists(&root).await.unwrap_or(false) {
            let now = SystemTime::now();
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
                let (remove, page) = if let Some(id) = name.strip_suffix(PART_SUFFIX) {
                    let age = meta
                        .modified()
                        .ok()
                        .and_then(|modified| now.duration_since(modified).ok())
                        .unwrap_or_default();
                    (is_valid_id(id) && age >= STALE_PART_AGE, None)
                } else if let Some(id) = name.strip_suffix(HTML_SUFFIX) {
                    if !is_valid_id(id) || live.contains(id) {
                        (false, None)
                    } else if failed.contains(id) || released.contains_key(id) {
                        (true, Some(id))
                    } else {
                        report.unexplained += 1;
                        (false, None)
                    }
                } else {
                    (false, None)
                };
                if !remove {
                    report.retained += 1;
                    continue;
                }
                match tokio::fs::remove_file(entry.path()).await {
                    Ok(()) => {
                        report.bytes_freed += meta.len();
                        if page.is_some() {
                            report.removed_orphans += 1;
                        } else {
                            report.removed_parts += 1;
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%error, file = name, "Could not remove a web snapshot file");
                        report.retained += 1;
                        if let Some(id) = page {
                            kept_records.insert(id.to_string());
                        }
                    }
                }
            }
        }
        // A record is done once its page is gone, or a row claims the id again.
        let settled: Vec<i64> = released
            .into_iter()
            .filter(|(id, _)| !kept_records.contains(id))
            .flat_map(|(_, ids)| ids)
            .collect();
        if let Err(error) = self.intents.forget(&settled).await {
            tracing::warn!(%error, "Could not clear settled web snapshot records; the next sweep clears them");
        }
        if report.unexplained > 0 {
            tracing::info!(
                count = report.unexplained,
                "{} frozen pages no record explains; kept",
                report.unexplained
            );
        }
        Ok(report)
    }
}
