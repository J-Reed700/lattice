//! Keeps an open folder's index current while it stays open.
//!
//! `notify-debouncer-full` collapses an editor's save (write, rename, chmod)
//! into one event about 1.5 s after the folder goes quiet. The callback only
//! forwards paths; the folder's own task decides what they mean, through the
//! same per-file update the full run uses. No file-id cache: building one
//! walks the whole tree, ignored directories included, and the update reads
//! each path's state from the disk anyway.

use crate::shared::{AppError, Result};
use notify_debouncer_full::notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use notify_debouncer_full::{new_debouncer_opt, DebounceEventResult, Debouncer, NoCache};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc;

/// What the watcher saw.
#[derive(Debug)]
pub enum Change {
    Paths(Vec<PathBuf>),
    /// The watcher lost events; only a full pass is trustworthy now.
    Rescan,
}

/// Watches until dropped.
pub struct FolderWatcher {
    _debouncer: Debouncer<RecommendedWatcher, NoCache>,
}

pub fn watch(
    root: &Path,
    debounce: Duration,
) -> Result<(FolderWatcher, mpsc::UnboundedReceiver<Change>)> {
    let (tx, rx) = mpsc::unbounded_channel();
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
            let _ = tx.send(change);
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
        rx,
    ))
}
