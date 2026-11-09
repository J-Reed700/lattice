//! Deciding when the vector index is read from disk, and when it is written.
//! Startup asks the manifest ([`open_or_rebuild`]) and rebuilds the HNSW graph
//! only when SQLite changed. Mutations only mark the index dirty;
//! [`IndexPersistence`] writes it when a run goes quiet, at periodic
//! checkpoints and on shutdown, which is safe because a lost write makes the
//! next launch rebuild. The stamp is read before the index is written and the
//! manifest after, so a write during a save makes the manifest look stale
//! rather than vouch for missing vectors; startup also compares source counts
//! and key membership.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use sqlx::SqlitePool;

use super::manifest::{
    read_manifest, read_source_stamp, write_manifest, IndexManifest, ManifestMismatch,
};
use crate::infrastructure::vector_index::USearchVectorIndex;
use lattice_core::application::ports::VectorSearchPort;
use lattice_core::shared::error::AppError;
use lattice_core::shared::error::Result;

/// How often the flusher looks at the index.
const FLUSH_TICK: Duration = Duration::from_secs(5);
/// Quiet time after the last mutation that counts as "the run has finished".
const QUIET_PERIOD: Duration = Duration::from_secs(30);
/// A bulk import of tens of thousands of files never goes quiet for half an
/// hour. Checkpoint periodically to reduce graph rebuilding after a crash;
/// embeddings themselves are already committed to SQLite.
const CHECKPOINT_INTERVAL: Duration = Duration::from_secs(300);
/// Rows per page when checking key membership against SQLite.
const HYDRATE_PAGE: i64 = 512;

/// Which way startup went, for the log and for tests that need to assert no
/// rebuild happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupPath {
    /// The persisted graph matched SQLite and was used as it stands.
    Reused { vectors: usize, hydrated: usize },
    /// The graph was rebuilt from SQLite, for this reason.
    Rebuilt {
        vectors: usize,
        reason: ManifestMismatch,
    },
}

impl StartupPath {
    pub fn rebuilt(&self) -> bool {
        matches!(self, Self::Rebuilt { .. })
    }
}

/// Bring an already-opened index into agreement with SQLite, rebuilding only
/// if it is not already there.
///
/// `restored` is the rows a rebuild would use — produced lazily, because
/// reading every embedding blob is the cost this whole mechanism exists to
/// avoid. It is only called when the manifest says a rebuild is needed.
pub async fn open_or_rebuild<F, Fut>(
    index: &Arc<USearchVectorIndex>,
    pool: &SqlitePool,
    index_path: &Path,
    identity: &str,
    generation: &str,
    dimension: usize,
    restored: F,
) -> Result<StartupPath>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<Vec<(String, Vec<f32>, String, String, String)>>>,
{
    let started = Instant::now();
    let stamp = read_source_stamp(pool, identity, dimension).await?;
    let config = index.index_config();

    let mut mismatch = match read_manifest(index_path) {
        Some(manifest) => manifest.mismatch(generation, &config, index.count(), &stamp),
        None => Some(ManifestMismatch::Missing),
    };

    if mismatch.is_none() {
        let hydrated = hydrate_content(index, pool, identity, dimension).await?;
        if hydrated == index.count() {
            let path = StartupPath::Reused {
                vectors: index.count(),
                hydrated,
            };
            tracing::info!(
                vectors = index.count(),
                hydrated,
                took_ms = started.elapsed().as_millis(),
                "Vector index reused: the persisted graph still matches SQLite"
            );
            return Ok(path);
        }
        // A delete followed by an insert can keep the count unchanged while
        // publication is still pending. Every key must belong to the source
        // generation, not merely to a chunk that still exists in the library.
        mismatch = Some(ManifestMismatch::SourceMembership);
    }

    let reason = mismatch.unwrap_or(ManifestMismatch::Missing);
    let rows = restored().await?;
    let expected = rows.len();
    let rebuild_index = Arc::clone(index);
    let added = tokio::task::spawn_blocking(move || rebuild_index.rebuild_from_embeddings(rows))
        .await
        .map_err(|e| AppError::InternalError(format!("Vector index rebuild task failed: {e}")))??;
    if added != expected {
        return Err(AppError::InvalidState(
            "Incomplete vector index rebuild".into(),
        ));
    }

    // The stamp was read before the rebuild started, so anything written to
    // SQLite while it ran leaves the manifest looking stale — a wasted rebuild
    // next launch, never a wrong index.
    write_manifest(
        index_path,
        &IndexManifest::new(generation.to_string(), config, added, stamp),
    )?;

    tracing::info!(
        vectors = added,
        reason = %reason,
        took_ms = started.elapsed().as_millis(),
        "Vector index rebuilt from SQLite"
    );
    Ok(StartupPath::Rebuilt {
        vectors: added,
        reason,
    })
}

/// The production startup path. Restore one keyset page at a time, decode it,
/// then publish it into the runtime index before reading the next page.
pub async fn open_or_rebuild_streaming<F, Fut>(
    index: &Arc<USearchVectorIndex>,
    pool: &SqlitePool,
    index_path: &Path,
    identity: &str,
    generation: &str,
    dimension: usize,
    mut restore_page: F,
) -> Result<StartupPath>
where
    F: FnMut(String) -> Fut,
    Fut: std::future::Future<
        Output = Result<
            crate::infrastructure::persistence::repositories::embedding::generation::RestorePage,
        >,
    >,
{
    let started = Instant::now();
    let stamp = read_source_stamp(pool, identity, dimension).await?;
    let config = index.index_config();
    let mut mismatch = match read_manifest(index_path) {
        Some(manifest) => manifest.mismatch(generation, &config, index.count(), &stamp),
        None => Some(ManifestMismatch::Missing),
    };
    if mismatch.is_none() {
        let hydrated = hydrate_content(index, pool, identity, dimension).await?;
        if hydrated == index.count() {
            return Ok(StartupPath::Reused {
                vectors: index.count(),
                hydrated,
            });
        }
        mismatch = Some(ManifestMismatch::SourceMembership);
    }

    let reason = mismatch.unwrap_or(ManifestMismatch::Missing);
    index.begin_rebuild()?;
    let mut cursor = String::new();
    let mut added = 0usize;
    loop {
        let page = restore_page(cursor.clone()).await?;
        if page.cursor.is_empty() {
            break;
        }
        cursor = page.cursor;
        let expected = page.entries.len();
        let rebuild_index = Arc::clone(index);
        let count =
            tokio::task::spawn_blocking(move || rebuild_index.add_rebuild_batch(page.entries))
                .await
                .map_err(|e| {
                    AppError::InternalError(format!("Vector index rebuild batch failed: {e}"))
                })??;
        if count != expected {
            return Err(AppError::InvalidState(
                "Incomplete vector index rebuild batch".into(),
            ));
        }
        added += count;
    }
    let end_stamp = read_source_stamp(pool, identity, dimension).await?;
    if end_stamp != stamp {
        return Err(AppError::InvalidState(
            "Vector sources changed during startup rebuild; retry startup to restore a consistent generation".into(),
        ));
    }
    let finish_index = Arc::clone(index);
    tokio::task::spawn_blocking(move || finish_index.finish_rebuild())
        .await
        .map_err(|e| {
            AppError::InternalError(format!("Vector index finalize task failed: {e}"))
        })??;
    write_manifest(
        index_path,
        &IndexManifest::new(generation.to_string(), config, added, stamp),
    )?;
    tracing::info!(vectors = added, reason = %reason, took_ms = started.elapsed().as_millis(), "Vector index rebuilt from SQLite in bounded batches");
    Ok(StartupPath::Rebuilt {
        vectors: added,
        reason,
    })
}

/// Verify source membership without loading any chunk text. Candidate text is
/// fetched in small batches after ranking at the search boundary.
async fn hydrate_content(
    index: &Arc<USearchVectorIndex>,
    pool: &SqlitePool,
    identity: &str,
    dimension: usize,
) -> Result<usize> {
    let mut cursor = String::new();
    let mut filled = 0;
    loop {
        let rows: Vec<String> =
            sqlx::query_scalar(
                "SELECT tc.id FROM text_chunks tc \
                 WHERE tc.id > ? AND (\
                   EXISTS (SELECT 1 FROM text_embeddings te WHERE te.chunk_id = tc.id AND te.model_name = ? AND te.dimension = ?) \
                   OR EXISTS (SELECT 1 FROM embedding_generation_vectors eg WHERE eg.chunk_id = tc.id AND eg.model_identity = ? AND eg.dimension = ?)\
                 ) ORDER BY tc.id LIMIT ?",
            )
                .bind(&cursor)
                .bind(identity)
                .bind(dimension as i64)
                .bind(identity)
                .bind(dimension as i64)
                .bind(HYDRATE_PAGE)
                .fetch_all(pool)
                .await?;
        let Some(last) = rows.last() else {
            break;
        };
        cursor = last.clone();
        filled += index.hydrate_content(
            rows.into_iter()
                .map(|id| lattice_core::shared::persistence::embedding_encoding::vector_key(&id)),
        );
    }
    Ok(filled)
}

/// Owns the flush decision for one index.
pub struct IndexPersistence {
    index: Arc<USearchVectorIndex>,
    pool: SqlitePool,
    identity: String,
    generation: String,
    dimension: usize,
    index_path: PathBuf,
    flush_lock: tokio::sync::Mutex<()>,
}

impl IndexPersistence {
    pub fn new(
        index: Arc<USearchVectorIndex>,
        pool: SqlitePool,
        identity: String,
        generation: String,
        dimension: usize,
        index_path: PathBuf,
    ) -> Self {
        Self {
            index,
            pool,
            identity,
            generation,
            dimension,
            index_path,
            flush_lock: tokio::sync::Mutex::new(()),
        }
    }

    /// Write the index and the manifest that vouches for it. Does nothing when
    /// the index and the file already agree.
    pub async fn flush_if_dirty(&self) -> Result<bool> {
        // The background checkpoint and shutdown may overlap. Serialize the
        // whole operation, including its source stamp and manifest write.
        let _flush = self.flush_lock.lock().await;
        if !self.index.is_dirty() {
            return Ok(false);
        }
        let started = Instant::now();
        // Before the save, never after: see the module comment.
        let stamp = read_source_stamp(&self.pool, &self.identity, self.dimension).await?;
        let config = self.index.index_config();

        let index = Arc::clone(&self.index);
        let vectors = tokio::task::spawn_blocking(move || index.save_snapshot())
            .await
            .map_err(|e| {
                AppError::InternalError(format!("Vector index save task failed: {e}"))
            })??;

        write_manifest(
            &self.index_path,
            &IndexManifest::new(self.generation.clone(), config, vectors, stamp),
        )?;
        tracing::info!(
            vectors,
            took_ms = started.elapsed().as_millis(),
            "Vector index written to disk"
        );
        Ok(true)
    }

    /// Persist when a run has gone quiet, or when one has been going long
    /// enough that losing it would hurt.
    async fn flush_if_due(&self) -> Result<bool> {
        let Some(dirty) = self.index.dirty_for() else {
            return Ok(false);
        };
        if dirty.since_last_mutation < QUIET_PERIOD
            && dirty.since_first_mutation < CHECKPOINT_INTERVAL
        {
            return Ok(false);
        }
        self.flush_if_dirty().await
    }

    /// Watch the index for the lifetime of the process.
    ///
    /// There is no hook into "the folder finished indexing" on purpose: single
    /// files, folders, the batch import queue, re-indexing and deletion all
    /// end in different places, and a quiet period recognises every one of
    /// them without a flush call in five other features.
    ///
    /// Shutdown (`cancel`) stops ticks, waits for any active save, then
    /// performs the final flush before closing SQLite. Never cancel an active
    /// blocking save.
    pub async fn run(self: Arc<Self>, cancel: tokio_util::sync::CancellationToken) {
        loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => break,
                _ = tokio::time::sleep(FLUSH_TICK) => {}
            }
            // A scope that is never closed, as in standalone containers, must
            // not leave this worker as the only owner keeping an index alive.
            if Arc::strong_count(&self) == 1 {
                break;
            }
            if let Err(e) = self.flush_if_due().await {
                tracing::warn!(error = %e, "Could not persist the vector index");
            }
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
#[path = "persistence_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "persistence_smoke_tests.rs"]
mod smoke_tests;
