//! Tests for startup restore or rebuild, manifests and the debounced flush.

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

#[tokio::test]
async fn incomplete_streamed_batch_cannot_receive_a_valid_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("incomplete.usearch");
    let pool = database().await;
    add_chunk(&pool, 0).await;
    let index = open(&path);
    let result = open_or_rebuild_streaming(
        &index,
        &pool,
        &path,
        GENERATION,
        GENERATION,
        DIM,
        |_| async {
            Ok(crate::features::embedding::generation::RestorePage {
                entries: vec![(
                    "emb_chunk0".into(),
                    vec![1.0],
                    String::new(),
                    "chunk0".into(),
                    "doc".into(),
                )],
                cursor: "chunk0".into(),
            })
        },
    )
    .await;
    assert!(result.is_err());
    assert!(read_manifest(&path).is_none());
}

#[tokio::test]
async fn startup_rebuild_streams_bounded_pages_and_keeps_text_out_of_index() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("streamed.usearch");
    let pool = database().await;
    for i in 0..5 {
        add_chunk(&pool, i).await;
    }
    let index = open(&path);
    let page_pool = pool.clone();
    let restore = move |cursor: String| {
        let pool = page_pool.clone();
        async move {
            let ids: Vec<String> = sqlx::query_scalar(
                "SELECT tc.id FROM text_chunks tc JOIN text_embeddings te ON te.chunk_id = tc.id \
                     WHERE tc.id > ? ORDER BY tc.id LIMIT 2",
            )
            .bind(&cursor)
            .fetch_all(&pool)
            .await?;
            let next = ids.last().cloned().unwrap_or_default();
            let entries = ids
                .into_iter()
                .map(|id| {
                    let i = id.trim_start_matches("chunk").parse::<usize>().unwrap();
                    (
                        format!("emb_{id}"),
                        vector(i),
                        String::new(),
                        id,
                        "doc".into(),
                    )
                })
                .collect();
            Ok(crate::features::embedding::generation::RestorePage {
                entries,
                cursor: next,
            })
        }
    };
    let outcome =
        open_or_rebuild_streaming(&index, &pool, &path, GENERATION, GENERATION, DIM, restore)
            .await
            .unwrap();
    assert!(outcome.rebuilt());
    assert_eq!(index.count(), 5);
    let hits = VectorSearchPort::search(index.as_ref(), &vector(2), 1, 0.0).unwrap();
    assert_eq!(hits[0].content, "");
    assert_eq!(hits[0].chunk_id, "chunk2");
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
    assert!(hits[0].content.is_empty(), "index metadata retains no text");
    assert_eq!(hits[0].doc_id, "doc");
    let repository =
        crate::infrastructure::persistence::repositories::ChunkRepository::new(pool.clone());
    let text = crate::application::ports::ChunkRepositoryPort::find_content_by_ids(
        &repository,
        std::slice::from_ref(&hits[0].chunk_id),
    )
    .await
    .unwrap();
    assert_eq!(text.get("chunk0").map(String::as_str), Some("passage 0"));
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

/// Simulate process death after a flush reads a committed database change
/// but before that change reaches the in-memory vector index.
#[tokio::test]
async fn a_flush_before_publication_cannot_certify_missing_or_swapped_vectors() {
    for swap in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.usearch");
        let pool = database().await;
        add_chunk(&pool, 0).await;
        let (index, _) = start(&pool, &path).await;

        add_chunk(&pool, 1).await;
        index
            .publish_embeddings(vec![VectorIndexEntry {
                id: "emb_chunk1".into(),
                embedding: vector(1),
                content: "passage 1".into(),
                chunk_id: "chunk1".into(),
                document_id: "doc".into(),
            }])
            .unwrap();
        if swap {
            // Keep the chunk itself: hydration must check generation
            // membership, rather than the existence of the text alone.
            sqlx::query("DELETE FROM text_embeddings WHERE chunk_id = 'chunk0'")
                .execute(&pool)
                .await
                .unwrap();
        }
        add_chunk(&pool, 2).await; // committed, deliberately not published
        persistence(&index, &pool, &path)
            .flush_if_dirty()
            .await
            .unwrap();
        let saved = read_manifest(&path).unwrap();
        assert_eq!(saved.vector_count, 2);
        assert_eq!(
            saved.stamp,
            read_source_stamp(&pool, GENERATION, DIM).await.unwrap()
        );
        drop(index); // no graceful shutdown/publication

        let (recovered, outcome) = start(&pool, &path).await;
        assert!(outcome.rebuilt(), "stale snapshot reused: {outcome:?}");
        assert_eq!(recovered.count(), if swap { 2 } else { 3 });
        let hits = VectorSearchPort::search(recovered.as_ref(), &vector(2), 10, 0.0).unwrap();
        assert!(hits.iter().any(|hit| hit.chunk_id == "chunk2"));
        if swap {
            assert!(hits.iter().all(|hit| hit.chunk_id != "chunk0"));
        }
        drop(recovered);
        let (_, outcome) = start(&pool, &path).await;
        assert!(!outcome.rebuilt(), "repaired snapshot should be reusable");
    }
}

#[tokio::test]
async fn interrupted_snapshot_has_no_old_manifest_to_certify_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("index.usearch");
    let pool = database().await;
    add_chunk(&pool, 0).await;
    let (index, _) = start(&pool, &path).await;
    assert!(read_manifest(&path).is_some());

    // Stop after writing the snapshot, before its new manifest is written.
    assert_eq!(index.save_snapshot().unwrap(), 1);
    assert!(read_manifest(&path).is_none());
    drop(index);
    let (_, outcome) = start(&pool, &path).await;
    assert!(outcome.rebuilt());
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
