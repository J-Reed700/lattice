//! The startup sweeps of files under the lattice home: each deletes only what
//! a record this database wrote names as garbage, and keeps the rest.

use std::collections::HashSet;
use std::future::Future;
use std::sync::Arc;

use crate::features::indexing::library_owner::{self, LibraryFiles};
use crate::features::indexing::LibraryGc;
use crate::features::web::images::ChatImageService;
use crate::features::web::snapshots::WebPageSnapshotService;
use lattice_core::application::ports::ChatImagesPort;
use lattice_core::shared::error::Result;
use lattice_core::shared::fs::SweepGate;
use lattice_core::shared::persistence::Database;
use lattice_core::shared::runtime::background::BackgroundTasks;

/// Decide, once per start, whether the database behind `database` owns the files
/// `files` names, and open `gate` if it does. Every cleanup that deletes a file
/// because no row names it reads the gate afterwards; none reads the database
/// or the owner file again.
pub async fn open_gate_if_owner(database: &Database, files: &LibraryFiles, gate: &SweepGate) {
    if library_owner::may_sweep(database, files).await {
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
    /// Start all three in the background. With `gate` shut they remove no
    /// file and clear the records this database will never act on.
    /// `resuming` names the requests whose chat pictures are kept.
    pub fn start<R>(&self, gate: &Arc<SweepGate>, background: &BackgroundTasks, resuming: R)
    where
        R: Future<Output = Result<HashSet<String>>> + Send + 'static,
    {
        let library_gc = Arc::clone(&self.library_gc);
        background.spawn(async move {
            match library_gc.sweep().await {
                Ok(report) => tracing::info!(
                    removed = report.removed,
                    retained = report.retained,
                    bytes_freed = report.bytes_freed,
                    unexplained = report.unexplained,
                    "Swept the imported-file library: {} blob folders no record explains; kept",
                    report.unexplained
                ),
                Err(error) => {
                    tracing::error!(%error, "Failed to sweep the imported-file library");
                }
            }
        });
        // Frozen web snapshots a delete let go of, and writes a crash cut short.
        self.snapshots.sweep_in_background("startup");
        // Chat pictures a turn leased and never attached, and files a delete
        // let go of. No turn survives a restart except a research turn, which
        // resumes from its saved rounds and still owns the pictures they found.
        if gate.is_open() {
            self.images.sweep_in_background("startup", resuming);
        } else {
            let images = Arc::clone(&self.images);
            background.spawn(async move {
                if let Err(error) = images.remove_unclaimed_files().await {
                    tracing::warn!(%error, "Failed to clear the chat picture records");
                }
            });
        }
    }
}

#[cfg(test)]
mod tests;
