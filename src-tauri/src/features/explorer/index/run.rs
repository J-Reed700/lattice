//! One folder's indexing work: the full run when a folder opens, and the
//! per-path updates the watcher asks for afterwards.
//!
//! Everything that writes goes through one [`Indexer`] on one task, so a
//! watcher update never races the run it follows. The run is incremental:
//! a file whose size and modification time match its row is not read, one
//! whose content hash matches is not re-chunked, and one whose chunks are all
//! in the saved vectors file is not embedded again.
//!
//! Progress is told in passages, after every batch, with a time left from a
//! smoothed rate; files move when a save marks them. A full run logs one line
//! when its scan ends, one per tenth of the passages, and one when it is
//! ready or paused.

use super::dto::FolderIndexStatusDto;
use super::store::{Counts, FolderStore};
use crate::features::explorer::scope::Scope;
use crate::features::search::engine::vector_search::USearchVectorIndex;
use lattice_core::application::ports::EmbeddingPort;
use lattice_core::shared::Result;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

mod embed;
mod files;
mod sync;

/// A status goes to its sink at most this often while counts move; a change of
/// state always goes at once.
const STATUS_INTERVAL: Duration = Duration::from_millis(250);

/// Where a status goes as it moves: a build job's progress while one runs,
/// and the window while the watcher updates the open folder.
pub type StatusSink = Arc<dyn Fn(&FolderIndexStatusDto) + Send + Sync>;

/// A folder index's status, and the throttle on reporting it to a sink.
pub struct StatusCell {
    status: Mutex<FolderIndexStatusDto>,
    last_emit: Mutex<Option<Instant>>,
    sink: Mutex<Option<StatusSink>>,
}

impl StatusCell {
    pub fn new(initial: FolderIndexStatusDto) -> Self {
        Self {
            status: Mutex::new(initial),
            last_emit: Mutex::new(None),
            sink: Mutex::new(None),
        }
    }

    /// Sends every later change to `sink`, starting with the current status.
    pub fn attach(&self, sink: StatusSink) {
        self.forward(sink);
        self.announce();
    }

    /// Sends every later change to `sink`; the current status is already
    /// known where it goes.
    pub fn forward(&self, sink: StatusSink) {
        *self.sink.lock() = Some(sink);
    }

    pub fn detach(&self) {
        *self.sink.lock() = None;
    }

    pub fn get(&self) -> FolderIndexStatusDto {
        self.status.lock().clone()
    }

    /// Takes `status` as it is, without reporting it.
    pub fn replace(&self, status: FolderIndexStatusDto) {
        *self.status.lock() = status;
    }

    /// Applies `change` and reports it: at once when the state moved,
    /// otherwise only if the last report is old enough.
    pub fn update(&self, change: impl FnOnce(&mut FolderIndexStatusDto)) {
        let (snapshot, state_moved) = {
            let mut status = self.status.lock();
            let before = status.state;
            change(&mut status);
            (status.clone(), status.state != before)
        };
        let mut last = self.last_emit.lock();
        let due = last.is_none_or(|at| at.elapsed() >= STATUS_INTERVAL);
        if state_moved || due {
            *last = Some(Instant::now());
            drop(last);
            self.emit(&snapshot);
        }
    }

    /// Reports the current status regardless of the throttle.
    pub fn announce(&self) {
        *self.last_emit.lock() = Some(Instant::now());
        self.emit(&self.get());
    }

    fn emit(&self, status: &FolderIndexStatusDto) {
        let sink = self.sink.lock().clone();
        if let Some(sink) = sink {
            sink(status);
        }
    }
}

/// Bounds on one folder's work.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// More indexable files than this and nothing is embedded.
    pub max_files: usize,
    /// Chunks per `embed_batch` call. Small, so a chat turn's query embedding
    /// waits behind at most one batch.
    pub batch_size: usize,
    /// Chunks embedded between saves of the vectors file.
    pub save_every: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_files: 20_000,
            batch_size: 16,
            save_every: 500,
        }
    }
}

/// How fast passages are embedding. Each batch's rate is folded into an
/// average weighted by how long the batch took, so the estimate follows the
/// last half-minute or so: a pause (a chat turn's own embedding, a save) moves
/// it without throwing it.
#[derive(Debug, Clone, Copy)]
pub struct Pace {
    started: Instant,
    last: Instant,
    /// Passages per second, smoothed.
    rate: Option<f64>,
}

impl Pace {
    /// Roughly how far back the average looks.
    const HORIZON_SECS: f64 = 30.0;
    /// No estimate until a run has embedded for this long: the first batches
    /// include loading the model.
    const WARMUP: Duration = Duration::from_secs(10);

    pub fn starting_at(now: Instant) -> Self {
        Self {
            started: now,
            last: now,
            rate: None,
        }
    }

    /// `passages` more were embedded, finishing at `now`.
    pub fn record(&mut self, passages: usize, now: Instant) {
        let seconds = now.duration_since(self.last).as_secs_f64().max(1e-3);
        self.last = now;
        let sample = passages as f64 / seconds;
        let weight = 1.0 - (-seconds / Self::HORIZON_SECS).exp();
        self.rate = Some(match self.rate {
            Some(rate) => rate + weight * (sample - rate),
            None => sample,
        });
    }

    /// The smoothed rate, once the run is past its warm-up.
    pub fn rate(&self, now: Instant) -> Option<f64> {
        if now.duration_since(self.started) < Self::WARMUP {
            return None;
        }
        self.rate.filter(|rate| *rate > 0.0)
    }

    /// Seconds to embed `remaining` more at the current rate.
    pub fn eta(&self, remaining: u32, now: Instant) -> Option<u32> {
        let rate = self.rate(now)?;
        Some(
            (f64::from(remaining) / rate)
                .ceil()
                .min(f64::from(u32::MAX)) as u32,
        )
    }
}

/// How a run or an update ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Done,
    Cancelled,
    /// The walk found `count` indexable files, more than the bound: the first
    /// up to it are indexed, and the rest are found by text search only.
    TooLarge {
        count: usize,
    },
}

pub struct Indexer {
    pub store: Arc<FolderStore>,
    pub vectors: Arc<USearchVectorIndex>,
    pub embedder: Arc<dyn EmbeddingPort>,
    /// The index root, which the walk starts from so its ignore rules apply.
    pub scope: Scope,
    /// The part of the index this folder covers: `""`, or the picked
    /// sub-folder when a parent's index is reused.
    pub prefix: String,
    pub status: Arc<StatusCell>,
    pub cancel: CancellationToken,
    pub limits: Limits,
}

/// What one round of embedding did.
struct Embedded {
    outcome: Outcome,
    passages: usize,
    elapsed: Duration,
}

/// `12.3`, or `-` with no rate yet: for log lines.
fn per_second(rate: Option<f64>) -> String {
    rate.map_or_else(|| "-".to_string(), |rate| format!("{rate:.1}"))
}

impl Indexer {
    /// The folder this indexer covers: the index root, or the sub-folder
    /// under it.
    fn folder(&self) -> PathBuf {
        self.prefix
            .split('/')
            .filter(|part| !part.is_empty())
            .fold(self.scope.root().to_path_buf(), |path, part| {
                path.join(part)
            })
    }

    /// Sets the status's counts from the store, which is exact; the run's
    /// own tallies only move them in between.
    async fn refresh_counts(&self) -> Result<Counts> {
        let counts = self.store.counts(&self.prefix).await?;
        self.status.update(|status| {
            status.files_total = counts.files_total;
            status.files_indexed = counts.files_indexed;
            status.passages_total = counts.passages_total;
            status.passages_embedded = counts.passages_embedded;
        });
        Ok(counts)
    }
}
