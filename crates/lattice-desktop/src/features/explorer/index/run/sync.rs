//! Bringing the rows in line with the disk: the full run over a folder, and
//! the per-path updates the watcher asks for.

use super::files::{candidate, prepare, walk, Candidate, Prepared, Walked};
use super::{per_second, Indexer, Outcome};
use crate::features::explorer::fs;
use crate::features::explorer::index::dto::FolderIndexState;
use crate::features::explorer::index::store::{self, file_stamp, TooLarge};
use crate::infrastructure::passage_index::Document;
use lattice_core::shared::{AppError, Result};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;
use tracing::info;

/// Files read and chunked per trip to the blocking pool.
const PREPARE_GROUP: usize = 64;

impl Indexer {
    /// Walks the folder, brings the rows in line with it, and embeds what is
    /// new.
    pub async fn run(&self) -> Result<Outcome> {
        let started = Instant::now();
        self.status
            .update(|status| status.state = FolderIndexState::Scanning);
        let scope = self.scope.clone();
        let prefix = self.prefix.clone();
        let max_files = self.limits.max_files;
        let cancel = self.cancel.clone();
        let status = Arc::clone(&self.status);
        // Read before the walk, so a folder past the bound keeps the files it
        // already indexed rather than whichever the disk lists first.
        let existing = self.store.documents(&self.prefix).await?;
        let indexed: HashSet<String> = existing.keys().cloned().collect();
        let walked = tokio::task::spawn_blocking(move || {
            walk(&scope, &prefix, max_files, &indexed, &cancel, Some(&status))
        })
        .await
        .map_err(|error| AppError::InternalError(format!("Folder walk failed: {error}")))??;
        // Only a run over the whole index speaks for it; a sub-folder's run
        // leaves the parent's markers alone.
        let whole = self.prefix.is_empty();
        let (candidates, skipped) = match walked {
            Walked::Cancelled => return Ok(Outcome::Cancelled),
            Walked::Files { files, skipped } => (files, skipped),
        };
        // A folder past the bound is indexed up to it, and how many files were
        // left to text search is recorded, not dropped without a word.
        let found = candidates.len() + skipped;
        if whole {
            if skipped > 0 {
                let found = TooLarge { count: found };
                self.store
                    .set_meta(store::META_TOO_LARGE, &found.encode())
                    .await?;
            } else {
                self.store.delete_meta(store::META_TOO_LARGE).await?;
            }
        }
        let total = u32::try_from(candidates.len()).unwrap_or(u32::MAX);
        self.status.update(|status| status.files_total = total);

        let seen: HashSet<String> = candidates
            .iter()
            .map(|found| found.relative.clone())
            .collect();
        let Some(changed) = self.sync_candidates(candidates, &existing).await? else {
            return Ok(Outcome::Cancelled);
        };
        let gone: Vec<String> = existing
            .into_keys()
            .filter(|path| !seen.contains(path))
            .collect();
        self.forget(&gone).await?;
        let counts = self.refresh_counts().await?;
        info!(
            root = %self.folder().display(),
            files = counts.documents_total,
            new_or_changed = changed,
            removed = gone.len(),
            passages_to_embed = counts.passages_total.saturating_sub(counts.passages_embedded),
            elapsed_ms = started.elapsed().as_millis(),
            "Folder index scan finished"
        );

        let embedded = self.embed_pending().await?;
        if embedded.outcome != Outcome::Done {
            return Ok(embedded.outcome);
        }
        if whole {
            self.store
                .set_meta(
                    store::META_COMPLETE,
                    &chrono::Utc::now().timestamp_millis().to_string(),
                )
                .await?;
        }
        let status = self.status.get();
        let seconds = embedded.elapsed.as_secs_f64();
        info!(
            root = %self.folder().display(),
            files = status.files_total,
            passages = status.passages_total,
            embedded = embedded.passages,
            elapsed_ms = started.elapsed().as_millis(),
            per_second = %per_second(
                (embedded.passages > 0 && seconds > 0.0).then(|| embedded.passages as f64 / seconds)
            ),
            "Folder index ready"
        );
        Ok(if skipped > 0 {
            Outcome::TooLarge { count: found }
        } else {
            Outcome::Done
        })
    }

    /// Brings the rows for `paths` (relative to the index root, as the
    /// watcher reported them) in line with the disk, then embeds what moved.
    pub async fn sync_paths(&self, paths: Vec<String>) -> Result<Outcome> {
        for relative in paths {
            if self.cancel.is_cancelled() {
                return Ok(Outcome::Cancelled);
            }
            let absolute = relative
                .split('/')
                .fold(self.scope.root().to_path_buf(), |path, part| {
                    path.join(part)
                });
            let scope = self.scope.clone();
            let probe = absolute.clone();
            // An ignored path is dropped, and so is anything the walk would
            // never have reached: `.git`, a symlink, a special file.
            let (kind, ignored) = tokio::task::spawn_blocking(move || {
                let kind = std::fs::symlink_metadata(&probe)
                    .ok()
                    .map(|m| m.file_type());
                let ignored = probe.ancestors().any(fs::is_git_dir)
                    || (kind.is_some() && fs::is_ignored(&scope, &probe));
                (kind, ignored)
            })
            .await
            .map_err(|error| AppError::InternalError(format!("Folder update failed: {error}")))?;
            match kind {
                _ if ignored => {
                    // It may have been indexed before the rule that hides it.
                    self.forget_under(&relative).await?;
                }
                Some(kind) if kind.is_dir() => {
                    if self.sync_tree(&relative).await? == Outcome::Cancelled {
                        return Ok(Outcome::Cancelled);
                    }
                }
                Some(kind) if kind.is_file() => {
                    let scope = self.scope.clone();
                    let found = tokio::task::spawn_blocking(move || candidate(&scope, &absolute))
                        .await
                        .map_err(|error| {
                            AppError::InternalError(format!("Folder update failed: {error}"))
                        })?;
                    match found {
                        Some(found) => {
                            let existing = self.store.documents(&relative).await?;
                            self.sync_candidates(vec![found], &existing).await?;
                        }
                        None => self.forget(std::slice::from_ref(&relative)).await?,
                    }
                }
                _ => self.forget_under(&relative).await?,
            }
        }
        self.refresh_counts().await?;
        Ok(self.embed_pending().await?.outcome)
    }

    /// A directory that appeared or changed: walk just it.
    async fn sync_tree(&self, relative: &str) -> Result<Outcome> {
        let scope = self.scope.clone();
        let start = relative.to_string();
        let max_files = self.limits.max_files;
        let cancel = self.cancel.clone();
        let walked = tokio::task::spawn_blocking(move || {
            walk(&scope, &start, max_files, &HashSet::new(), &cancel, None)
        })
        .await
        .map_err(|error| AppError::InternalError(format!("Folder walk failed: {error}")))??;
        let candidates = match walked {
            Walked::Files { files, skipped: 0 } => files,
            Walked::Cancelled => return Ok(Outcome::Cancelled),
            // A single new directory over the whole bound is left to the next
            // full run, which indexes up to the bound and counts the rest.
            Walked::Files { .. } => return Ok(Outcome::Done),
        };
        let existing = self.store.documents(relative).await?;
        let seen: HashSet<String> = candidates
            .iter()
            .map(|found| found.relative.clone())
            .collect();
        if self.sync_candidates(candidates, &existing).await?.is_none() {
            return Ok(Outcome::Cancelled);
        }
        let gone: Vec<String> = existing
            .into_keys()
            .filter(|path| !seen.contains(path))
            .collect();
        self.forget(&gone).await?;
        Ok(Outcome::Done)
    }

    /// Reads and re-chunks the candidates whose size or time moved. Returns
    /// how many that was (new files included); `None` when cancelled.
    async fn sync_candidates(
        &self,
        candidates: Vec<Candidate>,
        existing: &HashMap<String, Document>,
    ) -> Result<Option<usize>> {
        let changed: Vec<(Candidate, Option<String>)> = candidates
            .into_iter()
            .filter_map(|found| match existing.get(&found.relative) {
                Some(row) if row.stamp == file_stamp(found.size, found.mtime) => None,
                Some(row) => Some((found, Some(row.content_hash.clone()))),
                None => Some((found, None)),
            })
            .collect();
        let count = changed.len();
        let mut groups = changed.into_iter().peekable();
        while groups.peek().is_some() {
            if self.cancel.is_cancelled() {
                return Ok(None);
            }
            let group: Vec<(Candidate, Option<String>)> =
                groups.by_ref().take(PREPARE_GROUP).collect();
            let prepared = tokio::task::spawn_blocking(move || {
                group
                    .into_iter()
                    .map(|(found, hash)| {
                        let decision = prepare(&found.absolute, hash.as_deref());
                        (found, decision)
                    })
                    .collect::<Vec<_>>()
            })
            .await
            .map_err(|error| AppError::InternalError(format!("Folder read failed: {error}")))?;
            for (found, decision) in prepared {
                match decision {
                    Prepared::Touch => {
                        self.store
                            .touch(&found.relative, &file_stamp(found.size, found.mtime))
                            .await?
                    }
                    Prepared::Replace { hash, chunks } => {
                        let passages: Vec<_> = chunks
                            .iter()
                            .map(|chunk| chunk.passage(&found.relative))
                            .collect();
                        let stamp = file_stamp(found.size, found.mtime);
                        let replaced = self
                            .store
                            .replace(&found.relative, &stamp, &hash, &passages)
                            .await?;
                        self.vectors.remove(&replaced)?;
                    }
                    Prepared::Gone => self.forget(std::slice::from_ref(&found.relative)).await?,
                }
            }
        }
        Ok(Some(count))
    }

    /// Forgets the files at `paths` and takes their vectors out of the
    /// in-memory index. They reach the file with the next save; until then a
    /// stale vector on disk is harmless, because a search keeps only hits
    /// whose passage row still exists.
    async fn forget(&self, paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let removed = self.store.remove(paths).await?;
        self.vectors.remove(&removed)
    }

    /// Forgets `prefix` and everything under it (a removed directory).
    async fn forget_under(&self, prefix: &str) -> Result<()> {
        let paths = self.store.keys_under(prefix).await?;
        self.forget(&paths).await
    }
}
