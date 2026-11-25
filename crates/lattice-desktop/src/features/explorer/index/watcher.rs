//! Keeps an open folder's index current while it stays open.
//!
//! `notify-debouncer-full` collapses an editor's save (write, rename, chmod)
//! into one event about 1.5 s after the folder goes quiet. The callback only
//! forwards paths; the folder's own task decides what they mean, through the
//! same per-file update the full run uses. No file-id cache: building one
//! walks the whole tree, ignored directories included, and the update reads
//! each path's state from the disk anyway.

use lattice_core::shared::{AppError, Result};
use notify_debouncer_full::notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use notify_debouncer_full::{new_debouncer_opt, DebounceEventResult, Debouncer, NoCache};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// Bounds buffered watcher batches so filesystem churn during a long folder
/// build never grows without bound in memory.
const WATCHER_CHANNEL_CAPACITY: usize = 256;

/// What the watcher saw.
#[derive(Debug)]
pub enum Change {
    Paths(Vec<PathBuf>),
    /// The watcher lost events or overflowed; only a full pass is trustworthy now.
    Rescan,
}

/// Bounded receiver for watcher events with overflow coalescing into [`Change::Rescan`].
pub struct ChangeReceiver {
    rx: mpsc::Receiver<Change>,
    overflow_rescan: Arc<AtomicBool>,
}

impl ChangeReceiver {
    pub async fn recv(&mut self) -> Option<Change> {
        if self.overflow_rescan.swap(false, Ordering::AcqRel) {
            return Some(Change::Rescan);
        }
        self.rx.recv().await
    }

    pub fn try_recv(&mut self) -> Option<Change> {
        if self.overflow_rescan.swap(false, Ordering::AcqRel) {
            return Some(Change::Rescan);
        }
        self.rx.try_recv().ok()
    }
}

fn push_change(tx: &mpsc::Sender<Change>, overflow_rescan: &AtomicBool, change: Change) {
    match tx.try_send(change) {
        Ok(()) => {}
        Err(mpsc::error::TrySendError::Full(_)) => {
            overflow_rescan.store(true, Ordering::Release);
            let _ = tx.try_send(Change::Rescan);
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {}
    }
}

/// Watches until dropped.
pub struct FolderWatcher {
    _debouncer: Debouncer<RecommendedWatcher, NoCache>,
}

pub fn watch(root: &Path, debounce: Duration) -> Result<(FolderWatcher, ChangeReceiver)> {
    let (tx, rx) = mpsc::channel(WATCHER_CHANNEL_CAPACITY);
    let overflow_rescan = Arc::new(AtomicBool::new(false));
    let callback_overflow = Arc::clone(&overflow_rescan);
    let mut debouncer = new_debouncer_opt::<_, RecommendedWatcher, NoCache>(
        debounce,
        None,
        move |result: DebounceEventResult| {
            let change = match result {
                Ok(events) => {
                    let paths: Vec<PathBuf> = events
                        .into_iter()
                        .filter(|event| !matches!(event.kind, EventKind::Access(_)))
                        .flat_map(|event| event.event.paths)
                        .collect();
                    if paths.is_empty() {
                        return;
                    }
                    Change::Paths(paths)
                }
                Err(errors) => {
                    tracing::warn!(?errors, "Folder index watcher lost events; rescanning");
                    Change::Rescan
                }
            };
            // The receiver is gone only once the folder is closed.
            push_change(&tx, &callback_overflow, change);
        },
        NoCache,
        notify_debouncer_full::notify::Config::default(),
    )
    .map_err(|error| {
        AppError::InternalError(format!("Folder index watcher failed to start: {error}"))
    })?;
    debouncer
        .watcher()
        .watch(root, RecursiveMode::Recursive)
        .map_err(|error| {
            AppError::InternalError(format!(
                "Folder index watcher could not watch {}: {error}",
                root.display()
            ))
        })?;
    Ok((
        FolderWatcher {
            _debouncer: debouncer,
        },
        ChangeReceiver {
            rx,
            overflow_rescan,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn full_bounded_channel_coalesces_overflow_into_rescan() {
        let (tx, rx) = mpsc::channel(2);
        let overflow_rescan = Arc::new(AtomicBool::new(false));
        let mut receiver = ChangeReceiver {
            rx,
            overflow_rescan: Arc::clone(&overflow_rescan),
        };

        push_change(
            &tx,
            &overflow_rescan,
            Change::Paths(vec![PathBuf::from("a")]),
        );
        push_change(
            &tx,
            &overflow_rescan,
            Change::Paths(vec![PathBuf::from("b")]),
        );
        // Channel is now full: subsequent bursts coalesce into Rescan without blocking or growing.
        for i in 0..100 {
            push_change(
                &tx,
                &overflow_rescan,
                Change::Paths(vec![PathBuf::from(format!("overflow-{i}"))]),
            );
        }

        let mut saw_rescan = false;
        while let Some(change) = receiver.try_recv() {
            if matches!(change, Change::Rescan) {
                saw_rescan = true;
            }
        }
        assert!(saw_rescan, "overflow must surface Change::Rescan");
        assert!(receiver.try_recv().is_none());
    }
}
