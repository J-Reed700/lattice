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
use lattice_core::shared::runtime::background::BackgroundTasks;

/// The three collectors of files under the lattice home.
pub struct LibrarySweeps {
    pub library_gc: Arc<LibraryGc>,
    pub snapshots: Arc<WebPageSnapshotService>,
    pub images: Arc<ChatImageService>,
}

impl LibrarySweeps {
    /// Start all three in the background, but only if `pool`'s database owns
    /// the files in `files`: each deletes whatever this database does not
    /// reference, and only the database that imported the files may say so.
    /// `resuming` names the requests whose chat pictures are kept.
    pub async fn start<R>(
        self,
        pool: &SqlitePool,
        files: &LibraryFiles,
        background: &BackgroundTasks,
        resuming: R,
    ) where
        R: Future<Output = Result<HashSet<String>>> + Send + 'static,
    {
        if !library_owner::may_sweep(pool, files).await {
            return;
        }
        let Self {
            library_gc,
            snapshots,
            images,
        } = self;
        background.spawn(async move {
            match library_gc.sweep().await {
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
        snapshots.sweep_in_background("startup");
        // Chat pictures a turn leased and never attached, and files no row
        // claims. No turn survives a restart except a research turn, which
        // resumes from its saved rounds and still owns the pictures they found.
        images.sweep_in_background("startup", resuming);
    }
}

#[cfg(test)]
mod tests;
