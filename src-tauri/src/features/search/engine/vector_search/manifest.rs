//! What the persisted USearch index claims to be.
//!
//! SQLite stays authoritative for vectors, but proving that by rebuilding the
//! HNSW graph on every launch costs a full-table read of every embedding blob
//! plus an O(N log N) graph construction — seconds at ten thousand chunks and
//! minutes at a million. The manifest lets startup ask the cheaper question
//! instead: *is the index on disk still the index SQLite would produce?*
//!
//! It answers that with three things:
//!
//! - the **format version**, so a change to the on-disk layout rebuilds rather
//!   than misreads (this is pre-release: there is no migration, only a rebuild);
//! - the **generation and configuration** the index was built for — the
//!   embedding model's artifact identity, the chunking strategy, the vector
//!   dimension and the HNSW knobs — so a model switch or a retuned graph is
//!   never silently reused;
//! - a **source stamp**: how many rows SQLite would restore for that
//!   generation, and a monotonic counter that every embedding or chunk write
//!   bumps from inside its own transaction (see the `vector_index_state`
//!   triggers in the schema migration).
//!
//! The row count is what catches a crash between an SQLite commit and an index
//! save; the counter is what catches a delete and an insert that happen to
//! leave the count unchanged. Either one differing means rebuild.
//!
//! The manifest is written **last**, after the index and key map are safely on
//! disk, so a crash mid-save leaves no manifest to trust and the next launch
//! rebuilds.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use super::compression::{VectorIndexCompression, VectorQuantization};
use crate::shared::error::AppError;
use crate::shared::result::Result;

/// Bumped whenever the meaning of anything the index persists changes. A
/// manifest written under a different version is not read; the index is
/// rebuilt.
pub const MANIFEST_FORMAT_VERSION: u32 = 1;

/// Compute the manifest path from the index path
/// (`foo.usearch` → `foo.usearch.manifest.json`).
pub fn manifest_path_for(index_path: &Path) -> PathBuf {
    let mut p = index_path.as_os_str().to_owned();
    p.push(".manifest.json");
    PathBuf::from(p)
}

/// The graph and vector layout the index was built with. Every field here
/// changes what the persisted file means, so any difference forces a rebuild.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexConfig {
    /// Embedding dimension callers hand in.
    pub dimension: usize,
    /// Dimension actually stored in USearch (smaller under truncation).
    pub stored_dimension: usize,
    pub metric: String,
    pub quantization: String,
    /// HNSW `M`.
    pub connectivity: usize,
    pub expansion_add: usize,
    pub expansion_search: usize,
}

impl IndexConfig {
    pub fn new(
        dimension: usize,
        compression: &VectorIndexCompression,
        connectivity: usize,
        expansion_add: usize,
        expansion_search: usize,
    ) -> Self {
        Self {
            dimension,
            stored_dimension: compression.stored_dimension(dimension),
            metric: "cos".to_string(),
            quantization: match compression.quantization() {
                VectorQuantization::F32 => "f32",
                VectorQuantization::I8 => "i8",
            }
            .to_string(),
            connectivity,
            expansion_add,
            expansion_search,
        }
    }
}

/// A cheap fingerprint of the SQLite embedding set backing one generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceStamp {
    /// Chunks that have a vector for this generation — the number of rows a
    /// rebuild would restore.
    pub row_count: u64,
    /// Global monotonic write counter from `vector_index_state`.
    pub write_counter: u64,
}

/// Read the stamp for one generation.
///
/// Two aggregate queries against covering indexes; no embedding blob is
/// touched. The counter is a single-row primary key lookup.
pub async fn read_source_stamp(
    pool: &SqlitePool,
    identity: &str,
    dimension: usize,
) -> Result<SourceStamp> {
    let dimension = dimension as i64;
    // Mirrors `embedding::generation::restore`'s row set. `restore` also drops
    // a prepared vector whose source text has since changed, which this count
    // cannot see; that direction is safe, because the extra row makes the
    // stamp differ and buys a rebuild.
    let row_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM text_chunks tc \
         LEFT JOIN text_embeddings te ON te.chunk_id = tc.id AND te.model_name = ? AND te.dimension = ? \
         LEFT JOIN embedding_generation_vectors eg ON eg.chunk_id = tc.id AND eg.model_identity = ? AND eg.dimension = ? \
         WHERE te.chunk_id IS NOT NULL OR eg.chunk_id IS NOT NULL",
    )
    .bind(identity)
    .bind(dimension)
    .bind(identity)
    .bind(dimension)
    .fetch_one(pool)
    .await?;

    let write_counter: i64 =
        sqlx::query_scalar("SELECT write_counter FROM vector_index_state WHERE id = 1")
            .fetch_optional(pool)
            .await?
            .unwrap_or(0);

    Ok(SourceStamp {
        row_count: row_count.max(0) as u64,
        write_counter: write_counter.max(0) as u64,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexManifest {
    pub format_version: u32,
    /// The vector space this index was built for: model artifact identity,
    /// chunking strategy and compression layout, as composed by `search::di`.
    pub generation: String,
    pub config: IndexConfig,
    /// Vectors the index held when it was saved.
    pub vector_count: usize,
    pub stamp: SourceStamp,
}

/// Why a persisted index could not be trusted, for the startup log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestMismatch {
    Missing,
    FormatVersion { found: u32 },
    Generation,
    Config,
    VectorCount { manifest: usize, index: usize },
    Stamp,
}

impl std::fmt::Display for ManifestMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing => write!(f, "no readable manifest"),
            Self::FormatVersion { found } => {
                write!(
                    f,
                    "manifest format {found} is not {MANIFEST_FORMAT_VERSION}"
                )
            }
            Self::Generation => write!(f, "built for a different embedding generation"),
            Self::Config => write!(f, "built with a different index configuration"),
            Self::VectorCount { manifest, index } => {
                write!(f, "manifest claims {manifest} vectors, index holds {index}")
            }
            Self::Stamp => write!(f, "SQLite embeddings changed since the index was saved"),
        }
    }
}

impl IndexManifest {
    pub fn new(
        generation: String,
        config: IndexConfig,
        vector_count: usize,
        stamp: SourceStamp,
    ) -> Self {
        Self {
            format_version: MANIFEST_FORMAT_VERSION,
            generation,
            config,
            vector_count,
            stamp,
        }
    }

    /// `None` when the persisted index can be used as it stands.
    pub fn mismatch(
        &self,
        generation: &str,
        config: &IndexConfig,
        loaded_vector_count: usize,
        stamp: &SourceStamp,
    ) -> Option<ManifestMismatch> {
        if self.format_version != MANIFEST_FORMAT_VERSION {
            return Some(ManifestMismatch::FormatVersion {
                found: self.format_version,
            });
        }
        if self.generation != generation {
            return Some(ManifestMismatch::Generation);
        }
        if self.config != *config {
            return Some(ManifestMismatch::Config);
        }
        if self.vector_count != loaded_vector_count {
            return Some(ManifestMismatch::VectorCount {
                manifest: self.vector_count,
                index: loaded_vector_count,
            });
        }
        if self.stamp != *stamp {
            return Some(ManifestMismatch::Stamp);
        }
        None
    }
}

/// Read the manifest beside an index, or `None` when it is absent or
/// unreadable. A half-written manifest is indistinguishable from none: both
/// mean rebuild.
pub fn read_manifest(index_path: &Path) -> Option<IndexManifest> {
    let bytes = std::fs::read(manifest_path_for(index_path)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Write the manifest atomically, after the index and key map are on disk.
pub fn write_manifest(index_path: &Path, manifest: &IndexManifest) -> Result<()> {
    let path = manifest_path_for(index_path);
    let bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|e| AppError::Serialization(format!("serialize index manifest: {e}")))?;
    super::usearch_index::write_file_atomically(&path, &bytes)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod tests {
    use super::*;

    fn config() -> IndexConfig {
        IndexConfig::new(1024, &VectorIndexCompression::None, 16, 128, 64)
    }

    fn stamp() -> SourceStamp {
        SourceStamp {
            row_count: 42,
            write_counter: 7,
        }
    }

    fn manifest() -> IndexManifest {
        IndexManifest::new("model-a".into(), config(), 42, stamp())
    }

    #[test]
    fn manifest_path_appends_suffix() {
        assert_eq!(
            manifest_path_for(Path::new("/tmp/foo.usearch")),
            PathBuf::from("/tmp/foo.usearch.manifest.json")
        );
    }

    #[test]
    fn a_matching_manifest_reports_no_mismatch() {
        assert_eq!(
            manifest().mismatch("model-a", &config(), 42, &stamp()),
            None
        );
    }

    #[test]
    fn every_kind_of_difference_is_named() {
        let m = manifest();
        assert_eq!(
            m.mismatch("model-b", &config(), 42, &stamp()),
            Some(ManifestMismatch::Generation)
        );

        let mut retuned = config();
        retuned.expansion_search = 128;
        assert_eq!(
            m.mismatch("model-a", &retuned, 42, &stamp()),
            Some(ManifestMismatch::Config)
        );

        assert_eq!(
            m.mismatch("model-a", &config(), 41, &stamp()),
            Some(ManifestMismatch::VectorCount {
                manifest: 42,
                index: 41
            })
        );

        let moved = SourceStamp {
            write_counter: 8,
            ..stamp()
        };
        assert_eq!(
            m.mismatch("model-a", &config(), 42, &moved),
            Some(ManifestMismatch::Stamp)
        );

        let grown = SourceStamp {
            row_count: 43,
            ..stamp()
        };
        assert_eq!(
            m.mismatch("model-a", &config(), 42, &grown),
            Some(ManifestMismatch::Stamp)
        );

        let stale = IndexManifest {
            format_version: MANIFEST_FORMAT_VERSION + 1,
            ..manifest()
        };
        assert_eq!(
            stale.mismatch("model-a", &config(), 42, &stamp()),
            Some(ManifestMismatch::FormatVersion {
                found: MANIFEST_FORMAT_VERSION + 1
            })
        );
    }

    #[test]
    fn a_compression_change_is_a_config_change() {
        let compressed = IndexConfig::new(
            1024,
            &VectorIndexCompression::truncated(256, VectorQuantization::I8),
            16,
            128,
            64,
        );
        assert_ne!(compressed, config());
        assert_eq!(compressed.stored_dimension, 256);
        assert_eq!(compressed.quantization, "i8");
    }

    #[test]
    fn write_then_read_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let index = dir.path().join("test.usearch");
        write_manifest(&index, &manifest()).unwrap();
        assert_eq!(read_manifest(&index), Some(manifest()));
    }

    #[test]
    fn a_corrupt_manifest_reads_as_missing() {
        let dir = tempfile::tempdir().unwrap();
        let index = dir.path().join("test.usearch");
        std::fs::write(manifest_path_for(&index), b"{not json").unwrap();
        assert_eq!(read_manifest(&index), None);
    }

    async fn database() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        for statement in [
            "CREATE TABLE text_chunks (id TEXT PRIMARY KEY, content TEXT, document_id TEXT)",
            "CREATE TABLE text_embeddings (id TEXT PRIMARY KEY, chunk_id TEXT, embedding BLOB, model_name TEXT, dimension INTEGER)",
            "CREATE TABLE embedding_generation_vectors (model_identity TEXT, chunk_id TEXT, content_hash TEXT, embedding BLOB, dimension INTEGER, PRIMARY KEY(model_identity, chunk_id))",
            "CREATE TABLE vector_index_state (id INTEGER PRIMARY KEY, write_counter INTEGER NOT NULL DEFAULT 0)",
            "INSERT INTO vector_index_state VALUES (1, 0)",
            "CREATE TRIGGER t1 AFTER INSERT ON text_embeddings BEGIN UPDATE vector_index_state SET write_counter = write_counter + 1 WHERE id = 1; END",
            "CREATE TRIGGER t2 AFTER DELETE ON text_embeddings BEGIN UPDATE vector_index_state SET write_counter = write_counter + 1 WHERE id = 1; END",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        pool
    }

    #[tokio::test]
    async fn the_stamp_counts_only_this_generation_and_moves_with_every_write() {
        let pool = database().await;
        assert_eq!(
            read_source_stamp(&pool, "model-a", 4).await.unwrap(),
            SourceStamp {
                row_count: 0,
                write_counter: 0
            }
        );

        sqlx::query("INSERT INTO text_chunks VALUES ('c1', 'alpha', 'doc')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO text_embeddings VALUES ('emb_c1', 'c1', x'00', 'model-a', 4)")
            .execute(&pool)
            .await
            .unwrap();

        let after_insert = read_source_stamp(&pool, "model-a", 4).await.unwrap();
        assert_eq!(after_insert.row_count, 1);
        assert_eq!(after_insert.write_counter, 1);

        // Another model's vectors are not this index's business.
        assert_eq!(
            read_source_stamp(&pool, "model-b", 4)
                .await
                .unwrap()
                .row_count,
            0
        );
        // …nor is the same model at another dimension.
        assert_eq!(
            read_source_stamp(&pool, "model-a", 8)
                .await
                .unwrap()
                .row_count,
            0
        );

        // A prepared vector counts even with no `text_embeddings` row.
        sqlx::query("INSERT INTO text_chunks VALUES ('c2', 'beta', 'doc')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO embedding_generation_vectors VALUES ('model-a', 'c2', 'h', x'00', 4)",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            read_source_stamp(&pool, "model-a", 4)
                .await
                .unwrap()
                .row_count,
            2
        );

        // A delete followed by an insert leaves the count alone; the counter
        // is the only thing that notices.
        let before = read_source_stamp(&pool, "model-a", 4).await.unwrap();
        sqlx::query("DELETE FROM text_embeddings WHERE id = 'emb_c1'")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO text_embeddings VALUES ('emb_c1', 'c1', x'00', 'model-a', 4)")
            .execute(&pool)
            .await
            .unwrap();
        let after = read_source_stamp(&pool, "model-a", 4).await.unwrap();
        assert_eq!(after.row_count, before.row_count);
        assert_ne!(after.write_counter, before.write_counter);
    }
}
