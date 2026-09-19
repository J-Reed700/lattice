//! Deciding when the vector index is read from disk, and when it is written.
//!
//! Two halves of the same bargain.
//!
//! **Startup** used to rebuild the HNSW graph unconditionally, on the grounds
//! that SQLite is authoritative and a rebuild also sweeps up keys a crash left
//! behind. It is still authoritative, but proving agreement by rebuilding
//! costs a full-table read of every embedding blob plus an O(N log N) graph
//! construction on every launch. [`open_or_rebuild`] asks the manifest first
//! and rebuilds only when the answer is anything other than "unchanged".
//!
//! **Saving** used to happen after every mutation, which meant indexing a
//! thousand-document folder wrote the whole corpus a thousand times. Mutations
//! now only mark the index dirty; [`IndexPersistence`] writes it when a run
//! goes quiet, at a periodic checkpoint through a long run, and on shutdown.
//! Losing a coalesced write to a crash is safe precisely because of the first
//! half: the next launch sees SQLite ahead of the index and rebuilds.
//!
//! The order matters. The stamp is read *before* the index is written and the
//! manifest *after*, so a write that lands during a save makes the manifest
//! look stale and buys a rebuild. The opposite order would let a manifest
//! vouch for vectors the index never received.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use sqlx::SqlitePool;

use super::manifest::{
    read_manifest, read_source_stamp, write_manifest, IndexManifest, ManifestMismatch,
};
use super::usearch_index::USearchVectorIndex;
use crate::application::ports::VectorSearchPort;
use crate::shared::error::AppError;
use crate::shared::result::Result;

/// How often the flusher looks at the index.
const FLUSH_TICK: Duration = Duration::from_secs(5);
/// Quiet time after the last mutation that counts as "the run has finished".
const QUIET_PERIOD: Duration = Duration::from_secs(30);
/// A bulk import of tens of thousands of files never goes quiet for half an
/// hour, and losing all of it to a crash would mean re-embedding all of it.
/// Check it in periodically regardless.
const CHECKPOINT_INTERVAL: Duration = Duration::from_secs(300);
/// Rows per page when refilling chunk text from SQLite.
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

    let mismatch = match read_manifest(index_path) {
        Some(manifest) => manifest.mismatch(generation, &config, index.count(), &stamp),
        None => Some(ManifestMismatch::Missing),
    };

    if mismatch.is_none() {
        let hydrated = hydrate_content(index, pool).await?;
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

/// Refill the chunk text the key map deliberately does not persist.
///
/// Paged rather than loaded whole: a large library's chunk text is hundreds of
/// megabytes, and there is no reason for two copies of it to exist at once.
async fn hydrate_content(index: &Arc<USearchVectorIndex>, pool: &SqlitePool) -> Result<usize> {
    let mut cursor = String::new();
    let mut filled = 0;
    loop {
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT id, content FROM text_chunks WHERE id > ? ORDER BY id LIMIT ?")
                .bind(&cursor)
                .bind(HYDRATE_PAGE)
                .fetch_all(pool)
                .await?;
        let Some((last, _)) = rows.last() else {
            break;
        };
        cursor = last.clone();
        filled += index.hydrate_content(rows.into_iter().map(|(id, content)| {
            (
                crate::features::embedding::encoding::vector_key(&id),
                content,
            )
        }));
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
        }
    }

    /// Write the index and the manifest that vouches for it. Does nothing when
    /// the index and the file already agree.
    pub async fn flush_if_dirty(&self) -> Result<bool> {
        if !self.index.is_dirty() {
            return Ok(false);
        }
        let started = Instant::now();
        // Before the save, never after: see the module comment.
        let stamp = read_source_stamp(&self.pool, &self.identity, self.dimension).await?;
        let config = self.index.index_config();

        let index = Arc::clone(&self.index);
        tokio::task::spawn_blocking(move || index.save_to_disk())
            .await
            .map_err(|e| {
                AppError::InternalError(format!("Vector index save task failed: {e}"))
            })??;

        let vectors = self.index.count();
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
    /// Shutdown does not cancel this loop; it calls [`Self::flush_if_dirty`]
    /// directly, before the database closes, so the last save is not racing a
    /// worker-abort deadline.
    pub async fn run(self: Arc<Self>) {
        loop {
            tokio::time::sleep(FLUSH_TICK).await;
            if let Err(e) = self.flush_if_due().await {
                tracing::warn!(error = %e, "Could not persist the vector index");
            }
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod tests {
    use super::*;
    use crate::application::ports::vector_search_port::VectorIndexEntry;
    use crate::application::ports::VectorSearchPort;
    use crate::features::search::engine::vector_search::compression::VectorIndexCompression;
    use crate::features::search::engine::vector_search::manifest::manifest_path_for;

    const GENERATION: &str = "model-a";
    const DIM: usize = 4;

    async fn database() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        for statement in [
            "CREATE TABLE text_chunks (id TEXT PRIMARY KEY, content TEXT, contextualized_content TEXT, document_id TEXT)",
            "CREATE TABLE text_embeddings (id TEXT PRIMARY KEY, chunk_id TEXT, embedding BLOB, model_name TEXT, dimension INTEGER)",
            "CREATE TABLE embedding_generation_vectors (model_identity TEXT, chunk_id TEXT, content_hash TEXT, embedding BLOB, dimension INTEGER, PRIMARY KEY(model_identity, chunk_id))",
            "CREATE TABLE vector_index_state (id INTEGER PRIMARY KEY, write_counter INTEGER NOT NULL DEFAULT 0)",
            "INSERT INTO vector_index_state VALUES (1, 0)",
            "CREATE TRIGGER te_i AFTER INSERT ON text_embeddings BEGIN UPDATE vector_index_state SET write_counter = write_counter + 1 WHERE id = 1; END",
            "CREATE TRIGGER te_d AFTER DELETE ON text_embeddings BEGIN UPDATE vector_index_state SET write_counter = write_counter + 1 WHERE id = 1; END",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        pool
    }

    fn vector(i: usize) -> Vec<f32> {
        let angle = i as f32;
        vec![angle.cos(), angle.sin(), 0.0, 0.0]
    }

    async fn add_chunk(pool: &SqlitePool, i: usize) {
        let id = format!("chunk{i}");
        sqlx::query("INSERT INTO text_chunks (id, content, document_id) VALUES (?, ?, 'doc')")
            .bind(&id)
            .bind(format!("passage {i}"))
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO text_embeddings (id, chunk_id, embedding, model_name, dimension) VALUES (?, ?, x'00', ?, ?)",
        )
        .bind(format!("emb_{id}"))
        .bind(&id)
        .bind(GENERATION)
        .bind(DIM as i64)
        .execute(pool)
        .await
        .unwrap();
    }

    /// What a rebuild would restore, without going near the real encoder.
    async fn restore(pool: &SqlitePool) -> Result<Vec<(String, Vec<f32>, String, String, String)>> {
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT tc.id, tc.content FROM text_chunks tc JOIN text_embeddings te ON te.chunk_id = tc.id ORDER BY tc.id")
                .fetch_all(pool)
                .await?;
        Ok(rows
            .into_iter()
            .enumerate()
            .map(|(i, (id, content))| {
                (
                    format!("emb_{id}"),
                    vector(i),
                    content,
                    id,
                    "doc".to_string(),
                )
            })
            .collect())
    }

    fn open(path: &Path) -> Arc<USearchVectorIndex> {
        Arc::new(
            USearchVectorIndex::open_or_create_with_compression(
                DIM,
                path.to_path_buf(),
                VectorIndexCompression::None,
            )
            .unwrap()
            .with_coalesced_saves(),
        )
    }

    async fn start(pool: &SqlitePool, path: &Path) -> (Arc<USearchVectorIndex>, StartupPath) {
        let index = open(path);
        let outcome = open_or_rebuild(&index, pool, path, GENERATION, GENERATION, DIM, || {
            restore(pool)
        })
        .await
        .unwrap();
        (index, outcome)
    }

    fn persistence(
        index: &Arc<USearchVectorIndex>,
        pool: &SqlitePool,
        path: &Path,
    ) -> IndexPersistence {
        IndexPersistence::new(
            Arc::clone(index),
            pool.clone(),
            GENERATION.into(),
            GENERATION.into(),
            DIM,
            path.to_path_buf(),
        )
    }

    #[tokio::test]
    async fn a_matching_manifest_skips_the_rebuild_and_still_serves_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.usearch");
        let pool = database().await;
        for i in 0..8 {
            add_chunk(&pool, i).await;
        }

        let (first, outcome) = start(&pool, &path).await;
        assert_eq!(
            outcome,
            StartupPath::Rebuilt {
                vectors: 8,
                reason: ManifestMismatch::Missing
            }
        );
        drop(first);

        let (second, outcome) = start(&pool, &path).await;
        assert_eq!(
            outcome,
            StartupPath::Reused {
                vectors: 8,
                hydrated: 8
            },
            "an unchanged library must not rebuild"
        );
        // Hydration, not the key map, is what puts the text back.
        let hits = VectorSearchPort::search(second.as_ref(), &vector(0), 1, 0.0).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content, "passage 0");
        assert_eq!(hits[0].doc_id, "doc");
    }

    #[tokio::test]
    async fn a_new_embedding_row_forces_a_rebuild() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.usearch");
        let pool = database().await;
        for i in 0..4 {
            add_chunk(&pool, i).await;
        }
        let (first, _) = start(&pool, &path).await;
        drop(first);

        add_chunk(&pool, 4).await;
        let (_, outcome) = start(&pool, &path).await;
        assert_eq!(
            outcome,
            StartupPath::Rebuilt {
                vectors: 5,
                reason: ManifestMismatch::Stamp
            }
        );
    }

    /// A delete and an insert between two launches leave the row count alone.
    /// Only the write counter notices, and it has to.
    #[tokio::test]
    async fn a_swapped_row_forces_a_rebuild_even_though_the_count_is_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.usearch");
        let pool = database().await;
        for i in 0..4 {
            add_chunk(&pool, i).await;
        }
        let (first, _) = start(&pool, &path).await;
        drop(first);

        sqlx::query("DELETE FROM text_embeddings WHERE chunk_id = 'chunk0'")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO text_embeddings (id, chunk_id, embedding, model_name, dimension) VALUES ('emb_chunk0', 'chunk0', x'00', ?, ?)")
            .bind(GENERATION)
            .bind(DIM as i64)
            .execute(&pool)
            .await
            .unwrap();

        let (_, outcome) = start(&pool, &path).await;
        assert!(outcome.rebuilt(), "{outcome:?}");
    }

    #[tokio::test]
    async fn a_different_generation_or_configuration_forces_a_rebuild() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.usearch");
        let pool = database().await;
        add_chunk(&pool, 0).await;
        let (first, _) = start(&pool, &path).await;
        drop(first);

        let index = open(&path);
        let outcome = open_or_rebuild(&index, &pool, &path, GENERATION, "other-model", DIM, || {
            restore(&pool)
        })
        .await
        .unwrap();
        assert_eq!(
            outcome,
            StartupPath::Rebuilt {
                vectors: 1,
                reason: ManifestMismatch::Generation
            }
        );
    }

    #[tokio::test]
    async fn a_corrupt_manifest_or_index_still_recovers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.usearch");
        let pool = database().await;
        for i in 0..4 {
            add_chunk(&pool, i).await;
        }
        let (first, _) = start(&pool, &path).await;
        drop(first);

        std::fs::write(manifest_path_for(&path), b"{ truncated").unwrap();
        let (_, outcome) = start(&pool, &path).await;
        assert_eq!(
            outcome,
            StartupPath::Rebuilt {
                vectors: 4,
                reason: ManifestMismatch::Missing
            },
            "an unreadable manifest is no manifest"
        );
    }

    /// Keys a crash left in the index after their chunks were deleted must not
    /// survive recovery.
    #[tokio::test]
    async fn stale_keys_are_gone_after_a_rebuild() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.usearch");
        let pool = database().await;
        for i in 0..4 {
            add_chunk(&pool, i).await;
        }
        let (first, _) = start(&pool, &path).await;
        assert_eq!(first.count(), 4);
        drop(first);

        // The deletion reached SQLite; the process died before the index was
        // told, so the file on disk still has all four.
        sqlx::query("DELETE FROM text_embeddings WHERE chunk_id = 'chunk3'")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM text_chunks WHERE id = 'chunk3'")
            .execute(&pool)
            .await
            .unwrap();

        let (recovered, outcome) = start(&pool, &path).await;
        assert!(outcome.rebuilt(), "{outcome:?}");
        assert_eq!(recovered.count(), 3);
        let hits = VectorSearchPort::search(recovered.as_ref(), &vector(3), 8, 0.0).unwrap();
        assert!(
            hits.iter().all(|h| h.chunk_id != "chunk3"),
            "a deleted chunk came back: {hits:?}"
        );
    }

    #[tokio::test]
    async fn many_publications_produce_one_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.usearch");
        let pool = database().await;
        let index = open(&path);
        let persistence = persistence(&index, &pool, &path);

        for i in 0..16 {
            index
                .publish_embeddings(vec![VectorIndexEntry {
                    id: format!("emb_chunk{i}"),
                    embedding: vector(i),
                    content: format!("passage {i}"),
                    chunk_id: format!("chunk{i}"),
                    document_id: "doc".into(),
                }])
                .unwrap();
            assert!(!path.exists(), "publication {i} wrote the whole index");
        }
        assert!(index.is_dirty());

        assert!(persistence.flush_if_dirty().await.unwrap());
        assert!(path.exists());
        assert!(!index.is_dirty());
        assert!(
            !persistence.flush_if_dirty().await.unwrap(),
            "a clean index must not be rewritten"
        );
    }

    #[tokio::test]
    async fn a_leftover_temporary_is_ignored_and_cleaned_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.usearch");
        let pool = database().await;
        for i in 0..4 {
            add_chunk(&pool, i).await;
        }
        let (first, _) = start(&pool, &path).await;
        persistence(&first, &pool, &path)
            .flush_if_dirty()
            .await
            .unwrap();
        drop(first);

        // What a crash halfway through `index.save()` leaves behind.
        let temp = PathBuf::from({
            let mut p = path.clone().into_os_string();
            p.push(".tmp");
            p
        });
        std::fs::write(&temp, b"half a graph").unwrap();

        let (second, outcome) = start(&pool, &path).await;
        assert_eq!(
            outcome,
            StartupPath::Reused {
                vectors: 4,
                hydrated: 4
            },
            "a stray temporary must not invalidate a good index"
        );
        assert!(!temp.exists(), "the temporary should have been removed");
        assert_eq!(second.count(), 4);
    }

    /// What the manifest is actually worth, at a size worth measuring.
    ///
    /// Ignored because it builds a 20k × 1024 graph, which takes far longer
    /// than a unit test should. Run it deliberately:
    ///
    /// ```text
    /// cargo test --release --lib cold_start_against_a_forced_rebuild -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore = "benchmark: builds a 20k-vector HNSW graph"]
    async fn cold_start_against_a_forced_rebuild() {
        use crate::features::embedding::encoding::encode_embedding;

        const VECTORS: usize = 20_000;
        const WIDE: usize = 1_024;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bench.usearch");
        let pool = database().await;

        // Deterministic pseudo-random unit vectors, written through the same
        // encoder the real embedding path uses.
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        for i in 0..VECTORS {
            let mut values = Vec::with_capacity(WIDE);
            for _ in 0..WIDE {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                values.push((seed >> 40) as f32 / (1u64 << 23) as f32 - 1.0);
            }
            let norm = values.iter().map(|v| v * v).sum::<f32>().sqrt();
            for v in &mut values {
                *v /= norm;
            }
            let id = format!("chunk{i}");
            sqlx::query("INSERT INTO text_chunks (id, content, document_id) VALUES (?, ?, 'doc')")
                .bind(&id)
                .bind(format!("passage {i}"))
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query("INSERT INTO text_embeddings (id, chunk_id, embedding, model_name, dimension) VALUES (?, ?, ?, ?, ?)")
                .bind(format!("emb_{id}"))
                .bind(&id)
                .bind(encode_embedding(&values))
                .bind(GENERATION)
                .bind(WIDE as i64)
                .execute(&pool)
                .await
                .unwrap();
        }

        let restore = || {
            let pool = pool.clone();
            async move { crate::features::embedding::generation::restore(&pool, GENERATION, WIDE).await }
        };
        let open = || {
            Arc::new(
                USearchVectorIndex::open_or_create_with_compression(
                    WIDE,
                    path.clone(),
                    VectorIndexCompression::None,
                )
                .unwrap()
                .with_coalesced_saves(),
            )
        };

        // Build it once, the way a first launch would.
        let index = open();
        let built = Instant::now();
        let outcome = open_or_rebuild(&index, &pool, &path, GENERATION, GENERATION, WIDE, restore)
            .await
            .unwrap();
        assert!(outcome.rebuilt());
        let build = built.elapsed();
        IndexPersistence::new(
            Arc::clone(&index),
            pool.clone(),
            GENERATION.into(),
            GENERATION.into(),
            WIDE,
            path.clone(),
        )
        .flush_if_dirty()
        .await
        .unwrap();
        drop(index);

        let index = open();
        let reused_at = Instant::now();
        let outcome = open_or_rebuild(&index, &pool, &path, GENERATION, GENERATION, WIDE, restore)
            .await
            .unwrap();
        let reused = reused_at.elapsed();
        assert!(!outcome.rebuilt(), "{outcome:?}");
        assert_eq!(index.count(), VECTORS);
        drop(index);

        std::fs::remove_file(manifest_path_for(&path)).unwrap();
        let index = open();
        let rebuilt_at = Instant::now();
        let outcome = open_or_rebuild(&index, &pool, &path, GENERATION, GENERATION, WIDE, restore)
            .await
            .unwrap();
        let rebuilt = rebuilt_at.elapsed();
        assert!(outcome.rebuilt());

        println!(
            "{VECTORS} vectors x {WIDE} dims — first build {:?}; cold start with a matching \
             manifest {:?}; forced rebuild {:?}",
            build, reused, rebuilt
        );
    }

    #[tokio::test]
    async fn a_quiet_index_is_not_flushed_before_its_time() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.usearch");
        let pool = database().await;
        let index = open(&path);
        index
            .publish_embeddings(vec![VectorIndexEntry {
                id: "emb_chunk0".into(),
                embedding: vector(0),
                content: "passage 0".into(),
                chunk_id: "chunk0".into(),
                document_id: "doc".into(),
            }])
            .unwrap();

        let persistence = persistence(&index, &pool, &path);
        assert!(
            !persistence.flush_if_due().await.unwrap(),
            "a run that just mutated the index is still running"
        );
        assert!(!path.exists());
        assert!(persistence.flush_if_dirty().await.unwrap());
    }
}
