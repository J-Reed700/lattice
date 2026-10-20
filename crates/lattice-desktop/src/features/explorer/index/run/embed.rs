//! Embedding what is new, through the passage index's embedding loop: the
//! folder supplies its batch and save sizes and turns the loop's progress
//! into its status, with a rate, a time left and a log line per tenth.

use super::{per_second, Embedded, Indexer, Outcome, Pace};
use crate::features::explorer::index::dto::FolderIndexState;
use crate::infrastructure::passage_index::build::{self, Embedding, Progress};
use crate::infrastructure::passage_index::Passage;
use lattice_core::shared::Result;
use std::sync::Arc;
use std::time::Instant;
use tracing::info;

/// A folder's passages are embedded as they are stored: the line reference
/// heading, then the lines.
fn stored_text(passage: &Passage) -> String {
    passage.text.clone()
}

/// The loop's progress as the folder's status.
struct FolderProgress<'a> {
    indexer: &'a Indexer,
    pace: Pace,
    logged_tenths: u32,
    started: bool,
}

impl Progress for FolderProgress<'_> {
    fn started(&mut self, _documents: usize) {
        self.started = true;
        self.indexer.status.update(|status| {
            status.state = FolderIndexState::Indexing;
            status.passages_per_second = None;
            status.eta_seconds = None;
        });
        self.logged_tenths = self.indexer.status.get().percent() / 10;
    }

    /// `count` more passages are in the index: the status moves, with a rate
    /// and time left once the pace has enough behind it, and passing another
    /// tenth of the way gets a log line.
    fn embedded(&mut self, count: usize) {
        if count == 0 {
            return;
        }
        let now = Instant::now();
        self.pace.record(count, now);
        let added = u32::try_from(count).unwrap_or(u32::MAX);
        let rate = self.pace.rate(now);
        let pace = self.pace;
        self.indexer.status.update(|status| {
            status.passages_embedded = status
                .passages_embedded
                .saturating_add(added)
                .min(status.passages_total);
            status.passages_per_second = rate.map(|rate| rate as f32);
            status.eta_seconds = pace.eta(
                status
                    .passages_total
                    .saturating_sub(status.passages_embedded),
                now,
            );
        });
        let status = self.indexer.status.get();
        let tenths = status.percent() / 10;
        if tenths > self.logged_tenths && tenths < 10 {
            self.logged_tenths = tenths;
            info!(
                root = %self.indexer.folder().display(),
                percent = status.percent(),
                embedded = status.passages_embedded,
                total = status.passages_total,
                per_second = %per_second(rate),
                eta_seconds = ?status.eta_seconds,
                "Folder index progress"
            );
        }
    }

    /// Files move when a save marks them.
    fn saved(&mut self, documents: usize) {
        let added = u32::try_from(documents).unwrap_or(u32::MAX);
        self.indexer.status.update(|status| {
            status.files_indexed = status
                .files_indexed
                .saturating_add(added)
                .min(status.files_total)
        });
    }
}

impl Indexer {
    /// Embeds the passages of every file under the prefix not yet marked
    /// embedded, in the folder's batches, saving the vectors file every
    /// `save_every` passages. A file is marked only after a save that holds
    /// all of its passages, so a crash costs at most the work since the last
    /// save.
    pub(super) async fn embed_pending(&self) -> Result<Embedded> {
        let mut progress = FolderProgress {
            indexer: self,
            pace: Pace::starting_at(Instant::now()),
            logged_tenths: 0,
            started: false,
        };
        self.refresh_counts().await?;
        let embedding = Embedding {
            embedder: Arc::clone(&self.embedder),
            batch_size: self.limits.batch_size,
            save_every: self.limits.save_every,
            embed_text: Arc::new(stored_text),
        };
        let round = build::embed_pending(
            &self.store,
            &self.vectors,
            &embedding,
            &self.prefix,
            &self.cancel,
            &mut progress,
        )
        .await?;
        let outcome = if round.cancelled {
            Outcome::Cancelled
        } else {
            Outcome::Done
        };
        if progress.started && outcome == Outcome::Done {
            self.refresh_counts().await?;
        }
        Ok(Embedded {
            outcome,
            passages: round.passages,
            elapsed: round.elapsed,
        })
    }
}
