//! Which index embeds first when several want the one embedding model.
//!
//! Every passage index embeds through the same model, so two builds that
//! embed at once split its pace: neither finishes sooner than it would going
//! first. Builds therefore take turns by what waits on them. Whoever waits on
//! an index claims it at a [`Priority`] (a course open in its workspace or a
//! lesson about to read it; a folder open in the Explorer), and a run holds a
//! [`Running`] ticket while it embeds. Before each batch a run takes its
//! turn: while an index claimed above its own is embedding, it waits, and it
//! carries on from there once that run ends. A claim on an index with
//! nothing to embed holds nobody back.
//!
//! A waiting run keeps what its owner gave it (its job's slot, its index's
//! locks). An owner whose builds queue in one job kind still hands that slot
//! to the build that goes first itself, as Explorer has always done for its
//! open folder: the order here is between indexes that are embedding, never
//! the order of one kind's queue.
//!
//! The order is a value the composition root makes once and hands to every
//! owner of an index, never global state. Indexes are named by their
//! directory, which no two share.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

/// How much waits on an index. An index nobody claims ranks below both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Priority {
    /// Open where it is searched by its words until it is embedded: a folder
    /// in the Explorer.
    Open,
    /// Someone's work is held until it is embedded: a course in its
    /// workspace, whose evidence actions wait while it indexes, or a lesson
    /// about to read it.
    Awaited,
}

#[derive(Default)]
struct State {
    /// How many hold each claim.
    claims: HashMap<(PathBuf, Priority), usize>,
    /// How many runs are embedding each index.
    running: HashMap<PathBuf, usize>,
}

impl State {
    fn rank(&self, dir: &Path) -> Option<Priority> {
        self.claims
            .keys()
            .filter(|(claimed, _)| claimed == dir)
            .map(|(_, priority)| *priority)
            .max()
    }

    /// Whether another index ranked above `dir` is embedding.
    fn behind(&self, dir: &Path) -> bool {
        let own = self.rank(dir);
        self.running
            .keys()
            .any(|other| other != dir && self.rank(other) > own)
    }
}

/// One index embedding now, and the indexes it waits for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    pub dir: PathBuf,
    /// Indexes ranked above it that are embedding; empty when it goes on.
    pub ahead: Vec<PathBuf>,
}

/// The turn order every index's embedding follows.
pub struct EmbedTurns {
    state: Mutex<State>,
    /// Moves on every claim and ticket taken or given back, so a waiting run
    /// looks again.
    changed: watch::Sender<u64>,
}

impl Default for EmbedTurns {
    fn default() -> Self {
        Self {
            state: Mutex::new(State::default()),
            changed: watch::channel(0).0,
        }
    }
}

impl EmbedTurns {
    /// Claims `dir` at `priority` until the claim is dropped.
    pub fn claim(self: &Arc<Self>, dir: &Path, priority: Priority) -> Claim {
        let key = (dir.to_path_buf(), priority);
        *self.state.lock().claims.entry(key.clone()).or_default() += 1;
        self.moved();
        Claim {
            turns: Arc::clone(self),
            key,
        }
    }

    /// The highest claim on `dir`, if any.
    pub fn claimed(&self, dir: &Path) -> Option<Priority> {
        self.state.lock().rank(dir)
    }

    /// A run is embedding `dir` until the ticket is dropped.
    pub fn running(self: &Arc<Self>, dir: &Path) -> Running {
        *self
            .state
            .lock()
            .running
            .entry(dir.to_path_buf())
            .or_default() += 1;
        self.moved();
        Running {
            turns: Arc::clone(self),
            dir: dir.to_path_buf(),
        }
    }

    /// Whether a run over `dir` must wait before its next batch.
    pub fn behind(&self, dir: &Path) -> bool {
        self.state.lock().behind(dir)
    }

    /// Every index embedding now, each with the indexes ranked above it
    /// that are embedding too: those it waits for at its next batch.
    pub fn standing(&self) -> Vec<Standing> {
        let state = self.state.lock();
        let mut standing: Vec<Standing> = state
            .running
            .keys()
            .map(|dir| {
                let own = state.rank(dir);
                let mut ahead: Vec<PathBuf> = state
                    .running
                    .keys()
                    .filter(|other| *other != dir && state.rank(other) > own)
                    .cloned()
                    .collect();
                ahead.sort();
                Standing {
                    dir: dir.clone(),
                    ahead,
                }
            })
            .collect();
        standing.sort_by(|a, b| a.dir.cmp(&b.dir));
        standing
    }

    /// Moves on every claim and ticket taken or given back.
    pub fn changes(&self) -> watch::Receiver<u64> {
        self.changed.subscribe()
    }

    /// Waits until no index ranked above `dir` is embedding. `false` when
    /// `cancel` fired first.
    pub async fn wait_turn(&self, dir: &Path, cancel: &CancellationToken) -> bool {
        // Subscribed before looking, so a change between the look and the
        // wait still wakes it.
        let mut changes = self.changed.subscribe();
        loop {
            if !self.behind(dir) {
                return true;
            }
            tokio::select! {
                () = cancel.cancelled() => return false,
                changed = changes.changed() => {
                    if changed.is_err() {
                        return true;
                    }
                }
            }
        }
    }

    fn moved(&self) {
        self.changed
            .send_modify(|count| *count = count.wrapping_add(1));
    }
}

/// A claim on an index; dropping it gives the claim back.
pub struct Claim {
    turns: Arc<EmbedTurns>,
    key: (PathBuf, Priority),
}

impl Claim {
    pub fn dir(&self) -> &Path {
        &self.key.0
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        {
            let mut state = self.turns.state.lock();
            if let Some(count) = state.claims.get_mut(&self.key) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    state.claims.remove(&self.key);
                }
            }
        }
        self.turns.moved();
    }
}

/// A run embedding an index; dropping it ends the run's turn.
pub struct Running {
    turns: Arc<EmbedTurns>,
    dir: PathBuf,
}

impl Drop for Running {
    fn drop(&mut self) {
        {
            let mut state = self.turns.state.lock();
            if let Some(count) = state.running.get_mut(&self.dir) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    state.running.remove(&self.dir);
                }
            }
        }
        self.turns.moved();
    }
}
