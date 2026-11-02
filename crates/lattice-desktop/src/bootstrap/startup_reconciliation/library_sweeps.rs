//! The startup sweeps of files on disk that no row of this database claims.

use std::collections::HashSet;
use std::future::Future;
use std::sync::Arc;

use sqlx::SqlitePool;

use crate::features::indexing::library_owner::{self, LibraryFiles};
use crate::features::indexing::LibraryGc;
use crate::features::web::images::ChatImageService;
use crate::features::web::snapshots::WebPageSnapshotService;
use lattice_core::shared::error::Result;
use lattice_core::shared::fs::SweepGate;
use lattice_core::shared::runtime::background::BackgroundTasks;

/// Decide, once per start, whether the database behind `pool` owns the files
/// `files` names, and open `gate` if it does. Every cleanup that deletes a file
/// because no row names it reads the gate afterwards; none reads the database
/// or the owner file again.
pub async fn open_gate_if_owner(pool: &SqlitePool, files: &LibraryFiles, gate: &SweepGate) {
    if library_owner::may_sweep(pool, files).await {
        gate.open();
    }
}

/// The three collectors of files under the lattice home.
pub struct LibrarySweeps {
    pub library_gc: Arc<LibraryGc>,
    pub snapshots: Arc<WebPageSnapshotService>,
    pub images: Arc<ChatImageService>,
}

impl LibrarySweeps {
    /// Start all three in the background, if `gate` is open. `resuming` names
    /// the requests whose chat pictures are kept.
    pub fn start<R>(&self, gate: &Arc<SweepGate>, background: &BackgroundTasks, resuming: R)
    where
        R: Future<Output = Result<HashSet<String>>> + Send + 'static,
    {
        if !gate.is_open() {
            return;
        }
        let library_gc = Arc::clone(&self.library_gc);
        let gate = Arc::clone(gate);
        background.spawn(async move {
            match library_gc.sweep(&gate).await {
                Ok(report) => tracing::info!(
                    removed = report.removed,
                    retained = report.retained,
                    bytes_freed = report.bytes_freed,
                    "Swept the imported-file library"
                ),
                Err(error) => {
                    tracing::error!(%error, "Failed to sweep the imported-file library");
                }
            }
        });
        // Frozen web snapshots whose rows are gone, and writes a crash cut short.
        self.snapshots.sweep_in_background("startup");
        // Chat pictures a turn leased and never attached, and files no row
        // claims. No turn survives a restart except a research turn, which
        // resumes from its saved rounds and still owns the pictures they found.
        self.images.sweep_in_background("startup", resuming);
    }
}

#[cfg(test)]
mod tests;
