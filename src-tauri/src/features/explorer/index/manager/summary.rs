//! What a folder's index holds, as its status and as the folders list shows
//! it: live for the open folder and running builds, read from disk otherwise.

use super::{relative_below, root_string, too_large_message, FolderIndexManager};
use crate::features::explorer::index::build::{self, BuildRequest};
use crate::features::explorer::index::dto::{
    FolderIndexState, FolderIndexStatusDto, FolderIndexSummaryDto, FolderIndexSummaryState,
};
use crate::features::explorer::index::store::{self, Counts, FolderStore, IndexEntry};
use crate::features::explorer::scope::Scope;
use crate::shared::{runtime::jobs::JobStatus, AppError, Result};
use std::path::{Path, PathBuf};
use tracing::warn;

/// A closed index's state from its counts: work left is a pause; nothing at
/// all is no index, unless a full run finished over an empty folder.
fn settled_state(counts: &Counts, complete: bool) -> FolderIndexSummaryState {
    if counts.files_indexed < counts.files_total || counts.passages_embedded < counts.passages_total
    {
        FolderIndexSummaryState::Partial
    } else if counts.files_total == 0 && !complete {
        FolderIndexSummaryState::NotIndexed
    } else {
        FolderIndexSummaryState::Indexed
    }
}

/// The open folder's live status as the folders list shows it.
fn live_summary(status: &FolderIndexStatusDto, own_dir: Option<&Path>) -> FolderIndexSummaryDto {
    let counts = Counts {
        files_total: status.files_total,
        files_indexed: status.files_indexed,
        passages_total: status.passages_total,
        passages_embedded: status.passages_embedded,
    };
    let state = match status.state {
        FolderIndexState::Scanning | FolderIndexState::Indexing => {
            FolderIndexSummaryState::Indexing
        }
        FolderIndexState::Ready => FolderIndexSummaryState::Indexed,
        FolderIndexState::TooLarge => FolderIndexSummaryState::TooLarge,
        FolderIndexState::Refused => FolderIndexSummaryState::Refused,
        FolderIndexState::Error => FolderIndexSummaryState::Error,
        // No model to embed with: what is on disk is all there is.
        FolderIndexState::Unavailable => settled_state(&counts, true),
    };
    FolderIndexSummaryDto {
        state,
        files_total: counts.files_total,
        files_indexed: counts.files_indexed,
        passages_total: counts.passages_total,
        passages_embedded: counts.passages_embedded,
        bytes: own_dir.map_or(0, store::dir_bytes),
        index_root: (status.index_root != status.root).then(|| status.index_root.clone()),
        eta_seconds: status.eta_seconds,
        message: status.message.clone(),
    }
}

impl FolderIndexManager {
    /// The running build of `root`, with the status it last saved.
    async fn running_status(&self, root: &Path) -> Option<FolderIndexStatusDto> {
        let text = root_string(root);
        let builds = self.live_builds().await.ok()?;
        builds
            .iter()
            .filter(|job| job.status == JobStatus::Running)
            .filter(|job| BuildRequest::of(job).is_some_and(|asked| asked.root == text))
            .find_map(build::activity_status)
    }

    /// The live status for the open folder or a running build; for another,
    /// what its index on disk holds.
    pub async fn status(&self, root: &str) -> Result<FolderIndexStatusDto> {
        let scope = Scope::open(root)?;
        let root = scope.root().to_path_buf();
        if let Some(view) = self.view().filter(|view| view.root == root) {
            return Ok(view.status.get());
        }
        if let Some(status) = self.running_status(&root).await {
            return Ok(status);
        }
        let root_text = root_string(&root);
        if let Some(reason) = self.refusal(&root) {
            return Ok(FolderIndexStatusDto::new(
                &root_text,
                &root_text,
                FolderIndexState::Refused,
            )
            .with_message(reason));
        }
        let indexes = store::list_indexes(&self.config.base_dir).await;
        let (index_root, dir) = self.choose_index(&root, &indexes);
        if !dir.join(store::DB_FILE).exists() {
            return Err(AppError::NotFound(format!(
                "{} has no folder index yet.",
                root.display()
            )));
        }
        let prefix = relative_below(&index_root, &root).unwrap_or_default();
        let store = FolderStore::open(&dir).await?;
        let counts = store.counts(&prefix).await;
        store.close().await;
        Ok(build::initial_status(
            &root,
            &index_root,
            FolderIndexState::Ready,
            counts?,
        ))
    }

    /// What each of `roots`' index holds, for the folders list: the live
    /// status for the open folder and running builds, what is on disk for
    /// the rest. `indexes` is the directory listing, read once for the lot.
    pub async fn summaries(
        &self,
        roots: &[PathBuf],
        indexes: &[IndexEntry],
    ) -> Vec<FolderIndexSummaryDto> {
        let view = self.view();
        let running: Vec<(PathBuf, FolderIndexStatusDto)> = match self.live_builds().await {
            Ok(builds) => builds
                .iter()
                .filter(|job| job.status == JobStatus::Running)
                .filter_map(|job| {
                    Some((
                        PathBuf::from(BuildRequest::of(job)?.root),
                        build::activity_status(job)?,
                    ))
                })
                .collect(),
            Err(error) => {
                warn!(%error, "Could not read running folder builds");
                Vec::new()
            }
        };
        let mut summaries = Vec::with_capacity(roots.len());
        for root in roots {
            let live = match &view {
                Some(view) if &view.root == root => Some(view.status.get()),
                _ => running
                    .iter()
                    .find(|(running, _)| running == root)
                    .map(|(_, status)| status.clone()),
            };
            summaries.push(match live {
                Some(status) => {
                    let own = (status.index_root == status.root)
                        .then(|| store::index_dir(&self.config.base_dir, root));
                    live_summary(&status, own.as_deref())
                }
                None => self.summary_on_disk(root, indexes).await,
            });
        }
        summaries
    }

    /// A closed folder's index, read from disk without opening it.
    async fn summary_on_disk(&self, root: &Path, indexes: &[IndexEntry]) -> FolderIndexSummaryDto {
        let mut summary = FolderIndexSummaryDto::new(FolderIndexSummaryState::NotIndexed);
        if let Some(reason) = self.refusal(root) {
            summary.state = FolderIndexSummaryState::Refused;
            summary.message = Some(reason.to_string());
            return summary;
        }
        let (index_root, dir) = self.choose_index(root, indexes);
        let prefix = relative_below(&index_root, root).unwrap_or_default();
        if prefix.is_empty() {
            summary.bytes = store::dir_bytes(&dir);
        } else {
            summary.index_root = Some(root_string(&index_root));
        }
        match store::inspect(&dir, &prefix).await {
            Ok(None) => {}
            Ok(Some(snapshot)) => match snapshot.too_large.filter(|_| prefix.is_empty()) {
                Some(found) => {
                    summary.state = FolderIndexSummaryState::TooLarge;
                    summary.files_total = u32::try_from(found.count).unwrap_or(u32::MAX);
                    summary.message = Some(too_large_message(found, self.config.limits.max_files));
                }
                None => {
                    let counts = snapshot.counts;
                    summary.state = settled_state(&counts, snapshot.complete);
                    summary.files_total = counts.files_total;
                    summary.files_indexed = counts.files_indexed;
                    summary.passages_total = counts.passages_total;
                    summary.passages_embedded = counts.passages_embedded;
                }
            },
            Err(error) => {
                summary.state = FolderIndexSummaryState::Error;
                summary.message = Some(error.to_string());
            }
        }
        summary
    }
}
