//! Tests for the USearch vector index.
use super::*;
use crate::infrastructure::vector_index::vector_ops::cosine_similarity_simd;
use lattice_core::application::ports::vector_search_port::VectorIndexEntry;
use lattice_core::application::ports::VectorSearchPort;
use std::cmp::Ordering as CmpOrdering;
use std::sync::atomic::Ordering;
// The scalar reference, so these assertions do not simply repeat whatever
// the SIMD kernel the rescore now uses happens to compute.
use crate::infrastructure::vector_index::vector_ops::cosine_similarity_naive;

fn make_test_index(dim: usize) -> USearchVectorIndex {
    USearchVectorIndex::new(dim, None).unwrap_or_else(|e| {
        panic!("Failed to create test index: {}", e);
    })
}

/// Inserting more vectors than the initially reserved capacity used to walk
/// off the end of USearch's node array and take the whole process down with
/// SIGSEGV. `ensure_capacity` must grow the reservation transparently.
#[test]
fn growing_past_initial_capacity_does_not_crash() {
    let dim = 4;
    let index = make_test_index(dim);

    let count = INITIAL_INDEX_CAPACITY + 64;
    for i in 0..count {
        let angle = i as f32;
        index
            .add_embedding(
                format!("doc{}", i),
                vec![angle.cos(), angle.sin(), 0.0, 0.0],
            )
            .unwrap_or_else(|e| panic!("add {} failed: {}", i, e));
    }

    assert_eq!(index.index.size(), count);
    assert!(
        index.index.capacity() >= count,
        "capacity {} should have grown to hold {} vectors",
        index.index.capacity(),
        count
    );

    let results = VectorSearchPort::search(&index, &[1.0, 0.0, 0.0, 0.0], 5, 0.0)
        .unwrap_or_else(|e| panic!("search failed: {}", e));
    assert_eq!(results.len(), 5, "search must still work after growth");
}

#[test]
fn test_add_and_search() {
    let dim = 4;
    let index = make_test_index(dim);

    index
        .add_embedding("doc1".into(), vec![1.0, 0.0, 0.0, 0.0])
        .unwrap_or_else(|e| panic!("add failed: {}", e));
    index
        .add_embedding("doc2".into(), vec![0.0, 1.0, 0.0, 0.0])
        .unwrap_or_else(|e| panic!("add failed: {}", e));
    index
        .add_embedding("doc3".into(), vec![0.9, 0.1, 0.0, 0.0])
        .unwrap_or_else(|e| panic!("add failed: {}", e));

    let results = VectorSearchPort::search(&index, &[1.0, 0.0, 0.0, 0.0], 2, 0.0)
        .unwrap_or_else(|e| panic!("search failed: {}", e));

    assert_eq!(results.len(), 2);
    // doc1 should be the closest match
    assert_eq!(results[0].doc_id, "doc1");
}

#[test]
fn test_add_with_content() {
    let dim = 4;
    let index = make_test_index(dim);

    index
        .add_embedding_with_content(
            "emb_1".into(),
            vec![1.0, 0.0, 0.0, 0.0],
            "Hello world".into(),
            "chunk_1".into(),
            "doc_1".into(),
        )
        .unwrap_or_else(|e| panic!("add_with_content failed: {}", e));

    let results = VectorSearchPort::search(&index, &[1.0, 0.0, 0.0, 0.0], 1, 0.0)
        .unwrap_or_else(|e| panic!("search failed: {}", e));

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].doc_id, "doc_1");
    assert_eq!(results[0].chunk_id, "chunk_1");
    assert_eq!(results[0].content, "");
}

#[test]
fn test_remove() {
    let dim = 4;
    let index = make_test_index(dim);

    index
        .add_embedding("doc1".into(), vec![1.0, 0.0, 0.0, 0.0])
        .unwrap_or_else(|e| panic!("add failed: {}", e));

    assert_eq!(index.count(), 1);

    index
        .remove_embedding("doc1")
        .unwrap_or_else(|e| panic!("remove failed: {}", e));

    // count() now uses key map length which accurately reflects live entries
    assert_eq!(index.count(), 0);
}

#[test]
fn batch_removal_persists_and_preserves_other_documents() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("removal.usearch");
    let index = USearchVectorIndex::new(4, Some(path.clone())).unwrap();
    index
        .publish_embeddings(
            (0..128)
                .map(|i| VectorIndexEntry {
                    id: format!("emb_{i}"),
                    embedding: vec![1.0, i as f32 / 128.0, 0.0, 0.0],
                    content: format!("Passage {i}"),
                    chunk_id: format!("chunk_{i}"),
                    document_id: if i < 127 { "deleted" } else { "kept" }.into(),
                })
                .collect(),
        )
        .unwrap();
    let mut ids: Vec<_> = (0..127).map(|i| format!("emb_{i}")).collect();
    ids.push("missing".into());
    ids.push("emb_0".into());
    index.remove_embeddings(&ids).unwrap();
    index.remove_embeddings(&ids).unwrap();
    let loaded = USearchVectorIndex::open_or_create(4, path).unwrap();
    assert_eq!(loaded.count(), 1);
    let hits = VectorSearchPort::search(&loaded, &[1.0, 0.0, 0.0, 0.0], 10, 0.0).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].doc_id, "kept");
}

#[test]
fn test_duplicate_rejected() {
    let dim = 4;
    let index = make_test_index(dim);

    index
        .add_embedding("doc1".into(), vec![1.0, 0.0, 0.0, 0.0])
        .unwrap_or_else(|e| panic!("add failed: {}", e));

    let result = index.add_embedding("doc1".into(), vec![0.0, 1.0, 0.0, 0.0]);
    assert!(result.is_err());
}

#[test]
fn test_dimension_mismatch() {
    let index = make_test_index(4);

    let result = index.add_embedding("doc1".into(), vec![1.0, 0.0]); // wrong dim
    assert!(result.is_err());
}

#[test]
fn test_clear() {
    let dim = 4;
    let index = make_test_index(dim);

    index
        .add_embedding("doc1".into(), vec![1.0, 0.0, 0.0, 0.0])
        .unwrap_or_else(|e| panic!("add failed: {}", e));
    index
        .add_embedding("doc2".into(), vec![0.0, 1.0, 0.0, 0.0])
        .unwrap_or_else(|e| panic!("add failed: {}", e));

    index
        .clear()
        .unwrap_or_else(|e| panic!("clear failed: {}", e));

    assert_eq!(index.count(), 0);
    assert!(index.state.read().id_to_key.is_empty());
}

#[test]
fn test_persistence() {
    let dim = 4;
    let temp_dir = tempfile::TempDir::new().unwrap_or_else(|e| panic!("tempdir: {}", e));
    let index_path = temp_dir.path().join("test_index.usearch");

    {
        let index = USearchVectorIndex::open_or_create(dim, index_path.clone())
            .unwrap_or_else(|e| panic!("open_or_create failed: {}", e));

        index
            .add_embedding_with_content(
                "emb_1".into(),
                vec![1.0, 0.0, 0.0, 0.0],
                "test content".into(),
                "chunk_1".into(),
                "doc_1".into(),
            )
            .unwrap_or_else(|e| panic!("add failed: {}", e));

        // save_to_disk is called by add_embedding_with_content, but also by Drop
    }

    // Reload
    let index2 = USearchVectorIndex::open_or_create(dim, index_path)
        .unwrap_or_else(|e| panic!("reload failed: {}", e));

    assert_eq!(index2.count(), 1);

    let results = VectorSearchPort::search(&index2, &[1.0, 0.0, 0.0, 0.0], 1, 0.0)
        .unwrap_or_else(|e| panic!("search failed: {}", e));

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].chunk_id, "chunk_1");
    assert_eq!(results[0].doc_id, "doc_1");
    // Chunk text is not persisted with the key map — SQLite already has
    // it, and a copy of the whole corpus is what made every save expensive.
    assert_eq!(results[0].content, "");

    assert_eq!(index2.hydrate_content(["emb_1".to_string()]), 1);
    let rehydrated = VectorSearchPort::search(&index2, &[1.0, 0.0, 0.0, 0.0], 1, 0.0).unwrap();
    assert_eq!(rehydrated[0].content, "");
}

#[test]
fn hydration_ignores_chunks_this_generation_has_no_vector_for() {
    let index = make_test_index(4);
    index
        .add_embedding_with_content(
            "emb_1".into(),
            vec![1.0, 0.0, 0.0, 0.0],
            String::new(),
            "chunk_1".into(),
            "doc_1".into(),
        )
        .unwrap();
    assert_eq!(
        index.hydrate_content(["emb_1".to_string(), "emb_absent".to_string()]),
        1
    );
    let hits = VectorSearchPort::search(&index, &[1.0, 0.0, 0.0, 0.0], 1, 0.0).unwrap();
    assert_eq!(hits[0].content, "");
}

#[test]
fn batch_publication_is_retryable_and_persists_all_source_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("batch.usearch");
    let index = USearchVectorIndex::new(4, Some(path.clone())).unwrap();
    let entries = || {
        (0..128)
            .map(|i| VectorIndexEntry {
                id: format!("emb_{i}"),
                embedding: vec![1.0, i as f32 / 128.0, 0.0, 0.0],
                content: format!("Source passage {i}"),
                chunk_id: format!("chunk_{i}"),
                document_id: "pdf".into(),
            })
            .collect()
    };
    index.publish_embeddings(entries()).unwrap();
    index.publish_embeddings(entries()).unwrap();
    assert_eq!(index.count(), 128);
    // Open before Drop to prove publication itself flushed the entire batch.
    let loaded = USearchVectorIndex::open_or_create(4, path).unwrap();
    assert_eq!(loaded.count(), 128);
    let hits = VectorSearchPort::search(&loaded, &[1.0, 0.0, 0.0, 0.0], 128, 0.0).unwrap();
    assert_eq!(hits.len(), 128);
    assert!(hits
        .iter()
        .all(|hit| hit.doc_id == "pdf" && hit.chunk_id.starts_with("chunk_")));
}

/// The whole point of coalescing: publishing does not touch the disk, and
/// one save afterwards captures everything that was published.
#[test]
fn coalesced_publication_writes_once_instead_of_once_per_document() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("coalesced.usearch");
    let index = USearchVectorIndex::new(4, Some(path.clone()))
        .unwrap()
        .with_coalesced_saves();

    for document in 0..16 {
        index
            .publish_embeddings(
                (0..4)
                    .map(|chunk| VectorIndexEntry {
                        id: format!("emb_{document}_{chunk}"),
                        embedding: vec![1.0, document as f32 / 16.0, chunk as f32, 0.0],
                        content: String::new(),
                        chunk_id: format!("chunk_{document}_{chunk}"),
                        document_id: format!("doc_{document}"),
                    })
                    .collect(),
            )
            .unwrap();
        assert!(
            !path.exists(),
            "document {document} wrote the whole index to disk"
        );
    }
    assert!(index.is_dirty());

    index.save_to_disk().unwrap();
    assert!(!index.is_dirty());
    let loaded = USearchVectorIndex::open_or_create(4, path).unwrap();
    assert_eq!(
        loaded.count(),
        64,
        "one save must capture every publication"
    );
}

/// An index without a flusher — the summaries index, the eval harness, the
/// tests above — keeps writing on every mutation.
#[test]
fn the_default_policy_still_saves_immediately() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("immediate.usearch");
    let index = USearchVectorIndex::new(4, Some(path.clone())).unwrap();
    index
        .add_embedding("a".into(), vec![1.0, 0.0, 0.0, 0.0])
        .unwrap();
    assert!(path.exists());
    assert!(!index.is_dirty());
}

#[test]
fn a_save_replaces_the_index_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("atomic.usearch");
    let index = USearchVectorIndex::new(4, Some(path.clone()))
        .unwrap()
        .with_coalesced_saves();
    index
        .add_embedding("a".into(), vec![1.0, 0.0, 0.0, 0.0])
        .unwrap();
    index.save_to_disk().unwrap();

    let temp = PathBuf::from({
        let mut p = path.clone().into_os_string();
        p.push(".tmp");
        p
    });
    assert!(
        !temp.exists(),
        "a finished save must not leave its staging file behind"
    );

    // What a crash between `save` and `rename` leaves. It must never be
    // mistaken for the index itself.
    std::fs::write(&temp, b"half a graph").unwrap();
    let reopened = USearchVectorIndex::open_or_create(4, path).unwrap();
    assert_eq!(reopened.count(), 1);
    assert!(!temp.exists(), "a stale staging file should be cleaned up");
}

#[test]
fn test_concurrent_add_no_desync() {
    use std::sync::Arc;
    use std::thread;

    let dim = 4;
    let index = Arc::new(make_test_index(dim));

    let mut handles = Vec::new();
    for i in 0..10 {
        let idx = index.clone();
        handles.push(thread::spawn(move || {
            let id = format!("doc_{}", i);
            let mut embedding = vec![0.0f32; 4];
            embedding[i % 4] = 1.0;
            idx.add_internal(id, embedding, None, None, None)
        }));
    }

    let mut successes = 0;
    for h in handles {
        if h.join().unwrap().is_ok() {
            successes += 1;
        }
    }

    assert_eq!(successes, 10);
    assert_eq!(index.count(), 10);

    let state = index.state.read();
    assert_eq!(state.id_to_key.len(), 10);
    assert_eq!(state.key_to_id.len(), 10);
    for (id, &key) in &state.id_to_key {
        assert_eq!(state.key_to_id.get(&key), Some(id));
    }
}

#[test]
fn test_keymap_path_derivation() {
    let dim = 4;
    let path = PathBuf::from("/data/usearch_index.usearch");
    let index = USearchVectorIndex::new(dim, Some(path)).unwrap();
    assert_eq!(
        index.keymap_path.as_deref(),
        Some(Path::new("/data/usearch_index.keymap.json"))
    );

    // Multi-dotted filename
    let path2 = PathBuf::from("/data/index.v2.usearch");
    let index2 = USearchVectorIndex::new(dim, Some(path2)).unwrap();
    assert_eq!(
        index2.keymap_path.as_deref(),
        Some(Path::new("/data/index.v2.keymap.json"))
    );
}

// ---------------------------------------------------------------------
// Compression
// ---------------------------------------------------------------------

fn unit(values: &[f32]) -> Vec<f32> {
    let norm = values.iter().map(|v| v * v).sum::<f32>().sqrt();
    values.iter().map(|v| v / norm).collect()
}

/// Deterministic stand-in for a Matryoshka embedding: pseudo-random
/// directions whose energy decays along the axis, which is the property
/// truncation relies on.
fn synthetic_vector(seed: u64, dim: usize, decay: f32) -> Vec<f32> {
    let mut x = seed
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407)
        | 1;
    let mut values = Vec::with_capacity(dim);
    for i in 0..dim {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let unit_interval = (x >> 40) as f32 / (1u64 << 24) as f32;
        values.push((unit_interval * 2.0 - 1.0) * decay.powi(i as i32));
    }
    unit(&values)
}

fn i8_compression(dims: usize) -> VectorIndexCompression {
    VectorIndexCompression::truncated(dims, VectorQuantization::I8)
}

fn exact_cosine_ranking(query: &[f32], corpus: &[(String, Vec<f32>)]) -> Vec<String> {
    let mut scored: Vec<(String, f32)> = corpus
        .iter()
        .map(|(id, v)| (id.clone(), cosine_similarity_naive(query, v)))
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(CmpOrdering::Equal));
    scored.into_iter().map(|(id, _)| id).collect()
}

#[test]
#[ignore = "builds a 20,001-vector index to exercise the actual production threshold"]
fn recall_across_the_production_exact_to_graph_boundary() {
    const DIM: usize = 64;
    const TOP_K: usize = 10;
    let corpus: Vec<_> = (0..=EXACT_SEARCH_MAX_VECTORS)
        .map(|i| (format!("chunk{i}"), synthetic_vector(i as u64, DIM, 0.98)))
        .collect();
    let index = USearchVectorIndex::new(DIM, None).unwrap();
    let build_started = std::time::Instant::now();
    for (id, vector) in corpus.iter().take(EXACT_SEARCH_MAX_VECTORS) {
        index
            .add_embedding_with_content(
                id.clone(),
                vector.clone(),
                id.clone(),
                id.clone(),
                id.clone(),
            )
            .unwrap();
    }
    let queries: Vec<_> = (0..32)
        .map(|i| synthetic_vector(100_000 + i, DIM, 0.98))
        .collect();
    let started = std::time::Instant::now();
    for query in &queries {
        let expected = exact_cosine_ranking(query, &corpus[..EXACT_SEARCH_MAX_VECTORS]);
        let hits = VectorSearchPort::search(&index, query, TOP_K, 0.0).unwrap();
        assert_eq!(
            hits.iter().map(|h| &h.chunk_id).collect::<Vec<_>>(),
            expected.iter().take(TOP_K).collect::<Vec<_>>()
        );
    }
    let exact_elapsed = started.elapsed();
    let (id, vector) = corpus.last().unwrap();
    index
        .add_embedding_with_content(
            id.clone(),
            vector.clone(),
            id.clone(),
            id.clone(),
            id.clone(),
        )
        .unwrap();
    assert!(index.count() > index.exact_search_max);
    let mut found = 0;
    let mut search_elapsed = std::time::Duration::ZERO;
    for query in &queries {
        let expected: HashSet<_> = exact_cosine_ranking(query, &corpus)
            .into_iter()
            .take(TOP_K)
            .collect();
        let started = std::time::Instant::now();
        let hits = VectorSearchPort::search(&index, query, TOP_K, 0.0).unwrap();
        search_elapsed += started.elapsed();
        assert_eq!(hits.len(), TOP_K);
        found += hits
            .iter()
            .filter(|h| expected.contains(&h.chunk_id))
            .count();
    }
    let recall = found as f64 / (queries.len() * TOP_K) as f64;
    eprintln!(
        "20,001 vectors, 64 dims, {} held-out synthetic queries: recall@10={recall:.4}, mean graph query={:?}, exact checks including oracle={exact_elapsed:?}, total={:?}",
        queries.len(),
        search_elapsed / queries.len() as u32,
        build_started.elapsed()
    );
    assert!(
        recall >= 0.95,
        "graph recall below the fixture's regression floor: {recall}"
    );
}

#[test]
fn compression_shrinks_the_usearch_vector_but_not_the_reported_dimension() {
    let index = USearchVectorIndex::with_compression(1024, None, i8_compression(512)).unwrap();
    assert_eq!(
        VectorSearchPort::dimension(&index),
        1024,
        "callers still hand in full embeddings"
    );
    assert_eq!(index.index.dimensions(), 512);
    assert_eq!(index.compression().bytes_per_vector(1024), 512);
}

#[test]
fn a_configuration_wider_than_the_embedding_is_rejected() {
    assert!(USearchVectorIndex::with_compression(384, None, i8_compression(512)).is_err());
    assert!(USearchVectorIndex::with_compression(384, None, i8_compression(0)).is_err());
}

/// The whole point of rescoring: the quantized, truncated index ranks `a`
/// first because their prefixes are collinear, but `b` is the better match
/// on the full vector. The exact score must win.
#[test]
fn rescoring_overrides_the_approximate_order() {
    let index = USearchVectorIndex::with_compression(4, None, i8_compression(2)).unwrap();
    let a = unit(&[1.0, 0.0, 0.0, 0.0]);
    let b = unit(&[0.8, 0.6, 1.0, 0.0]);
    index.add_embedding("a".into(), a.clone()).unwrap();
    index.add_embedding("b".into(), b.clone()).unwrap();

    let query = unit(&[1.0, 0.0, 1.0, 0.0]);

    // Precondition: in the truncated space `a` is the nearer neighbour.
    let projected = index.compression().project(&query).unwrap();
    let approximate = index.index.search(projected.as_ref(), 2).unwrap();
    let approximate_first = approximate.keys.first().copied().unwrap();
    assert_eq!(
        index
            .state
            .read()
            .key_to_id
            .get(&approximate_first)
            .map(String::as_str),
        Some("a"),
        "precondition: the lossy index prefers `a`"
    );

    let results = VectorSearchPort::search(&index, &query, 2, 0.0).unwrap();
    assert_eq!(
        results
            .iter()
            .map(|r| r.doc_id.as_str())
            .collect::<Vec<_>>(),
        vec!["b", "a"],
        "rescoring must reorder the candidate window"
    );

    // …and the reported score is the exact cosine, not the quantized one.
    let exact_b = cosine_similarity_naive(&query, &b);
    let exact_a = cosine_similarity_naive(&query, &a);
    assert!((results[0].score - exact_b).abs() < 1e-5, "{:?}", results);
    assert!((results[1].score - exact_a).abs() < 1e-5, "{:?}", results);
    assert!(exact_b > exact_a);
}

/// The contract users actually care about: turning compression on must not
/// change what comes back.
#[test]
fn compressed_search_returns_the_same_top_k_as_an_f32_index() {
    const DIM: usize = 16;
    const CORPUS: u64 = 96;
    const TOP_K: usize = 5;

    let corpus: Vec<(String, Vec<f32>)> = (0..CORPUS)
        .map(|i| (format!("doc{i}"), synthetic_vector(i, DIM, 0.8)))
        .collect();

    let plain = USearchVectorIndex::new(DIM, None).unwrap();
    let compressed = USearchVectorIndex::with_compression(DIM, None, i8_compression(8)).unwrap();
    for (id, vector) in &corpus {
        plain.add_embedding(id.clone(), vector.clone()).unwrap();
        compressed
            .add_embedding(id.clone(), vector.clone())
            .unwrap();
    }

    for seed in 1_000..1_010u64 {
        let query = synthetic_vector(seed, DIM, 0.8);

        let expected = exact_cosine_ranking(&query, &corpus);
        let expected: Vec<&str> = expected.iter().take(TOP_K).map(String::as_str).collect();

        let plain_hits = VectorSearchPort::search(&plain, &query, TOP_K, 0.0).unwrap();
        let plain_ids: Vec<&str> = plain_hits.iter().map(|r| r.doc_id.as_str()).collect();
        assert_eq!(plain_ids, expected, "f32 baseline drifted for seed {seed}");

        let compressed_hits = VectorSearchPort::search(&compressed, &query, TOP_K, 0.0).unwrap();
        let compressed_ids: Vec<&str> = compressed_hits.iter().map(|r| r.doc_id.as_str()).collect();
        assert_eq!(
            compressed_ids, plain_ids,
            "compressed top-{TOP_K} diverged for seed {seed}"
        );

        for (compressed_hit, plain_hit) in compressed_hits.iter().zip(plain_hits.iter()) {
            assert!(
                (compressed_hit.score - plain_hit.score).abs() < 1e-3,
                "score {} vs {} for {}",
                compressed_hit.score,
                plain_hit.score,
                compressed_hit.doc_id
            );
        }
    }
}

/// Scope post-filtering widens the candidate window in a loop; the rescore
/// has to run on every pass, not just the first.
///
/// The scope is deliberately the three *worst* matches in the corpus, so
/// the first candidate window cannot satisfy it and the loop must widen to
/// exhaustion. Because the scope holds exactly `top_k` documents the
/// expected answer is fully determined: those three, in exact-cosine order.
#[test]
fn scoped_search_rescores_inside_the_widening_loop() {
    const DIM: usize = 16;
    const TOP_K: usize = 3;

    let corpus: Vec<(String, Vec<f32>)> = (0..128u64)
        .map(|i| (format!("chunk{i}"), synthetic_vector(i, DIM, 0.8)))
        .collect();
    let query = synthetic_vector(7_777, DIM, 0.8);

    let index = USearchVectorIndex::with_compression(DIM, None, i8_compression(8)).unwrap();
    for (i, (id, vector)) in corpus.iter().enumerate() {
        index
            .add_embedding_with_content(
                id.clone(),
                vector.clone(),
                format!("passage {i}"),
                id.clone(),
                format!("doc{i}"),
            )
            .unwrap();
    }

    // Rank every chunk exactly, then scope to the weakest three that still
    // clear a 0.0 threshold (a negative cosine is filtered out by design).
    let ranked = exact_cosine_ranking(&query, &corpus);
    let positive: Vec<String> = ranked
        .into_iter()
        .filter(|id| {
            corpus
                .iter()
                .find(|(candidate, _)| candidate == id)
                .is_some_and(|(_, v)| cosine_similarity_naive(&query, v) > 0.05)
        })
        .collect();
    let worst: Vec<String> = positive.iter().rev().take(TOP_K).cloned().collect();
    let expected: Vec<String> = worst.iter().rev().cloned().collect();

    let scope: HashSet<String> = worst
        .iter()
        .map(|chunk| {
            let i = chunk.trim_start_matches("chunk");
            format!("doc{i}")
        })
        .collect();

    let hits = VectorSearchPort::search_scoped(&index, &query, TOP_K, 0.0, Some(&scope)).unwrap();
    assert_eq!(
        hits.iter().map(|h| h.chunk_id.as_str()).collect::<Vec<_>>(),
        expected.iter().map(String::as_str).collect::<Vec<_>>(),
        "scoped results must come back in exact-cosine order"
    );
    assert!(hits.iter().all(|h| scope.contains(&h.doc_id)));

    // Every score is the exact cosine, which is only possible if the
    // rescore ran on the widened pass that finally found these chunks.
    for hit in &hits {
        let (_, vector) = corpus
            .iter()
            .find(|(id, _)| *id == hit.chunk_id)
            .expect("hit must come from the corpus");
        assert!(
            (hit.score - cosine_similarity_naive(&query, vector)).abs() < 1e-5,
            "{} scored {} approximately",
            hit.chunk_id,
            hit.score
        );
    }
}

#[test]
fn compressed_index_survives_a_round_trip_through_disk() {
    const DIM: usize = 16;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("compressed.usearch");
    let compression = i8_compression(8);

    let corpus: Vec<(String, Vec<f32>)> = (0..32u64)
        .map(|i| (format!("emb_{i}"), synthetic_vector(i, DIM, 0.8)))
        .collect();

    {
        let index = USearchVectorIndex::open_or_create_with_compression(
            DIM,
            path.clone(),
            compression.clone(),
        )
        .unwrap();
        index
            .publish_embeddings(
                corpus
                    .iter()
                    .enumerate()
                    .map(|(i, (id, vector))| VectorIndexEntry {
                        id: id.clone(),
                        embedding: vector.clone(),
                        content: format!("Passage {i}"),
                        chunk_id: format!("chunk_{i}"),
                        document_id: "book".into(),
                    })
                    .collect(),
            )
            .unwrap();
    }

    assert!(
        super::super::rescore_store::vectors_path_for(&path).exists(),
        "full vectors must be persisted for rescoring after a restart"
    );

    let reloaded =
        USearchVectorIndex::open_or_create_with_compression(DIM, path, compression).unwrap();
    assert_eq!(reloaded.count(), 32);

    let query = synthetic_vector(4_242, DIM, 0.8);
    let expected: Vec<String> = exact_cosine_ranking(&query, &corpus)
        .into_iter()
        .take(3)
        .map(|id| id.replace("emb_", "chunk_"))
        .collect();
    let hits = VectorSearchPort::search(&reloaded, &query, 3, 0.0).unwrap();
    assert_eq!(
        hits.iter().map(|h| h.chunk_id.as_str()).collect::<Vec<_>>(),
        expected.iter().map(String::as_str).collect::<Vec<_>>(),
        "rescoring must still work against the reloaded side store"
    );
    assert!(hits.iter().all(|h| h.chunk_id.starts_with("chunk_")));
}

/// A deleted chunk must not be resurrected by a stale full-precision copy.
#[test]
fn removal_and_clear_drop_the_full_precision_copies() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("removal.usearch");
    let index = USearchVectorIndex::with_compression(4, Some(path), i8_compression(2)).unwrap();

    index
        .add_embedding("a".into(), unit(&[1.0, 0.0, 0.0, 0.0]))
        .unwrap();
    index
        .add_embedding("b".into(), unit(&[0.8, 0.6, 1.0, 0.0]))
        .unwrap();

    let key_a = *index.state.read().id_to_key.get("a").unwrap();
    let store = index.rescore.as_ref().unwrap();
    assert!(store.get(key_a).is_some());

    index.remove_embedding("a").unwrap();
    assert!(store.get(key_a).is_none(), "stale vector left behind");

    let key_b = *index.state.read().id_to_key.get("b").unwrap();
    index.clear().unwrap();
    assert!(
        store.get(key_b).is_none(),
        "clear must empty the side store"
    );
}

/// Losing the side store degrades to approximate scoring rather than
/// failing or dropping results — an index rebuild restores exactness.
#[test]
fn a_missing_full_vector_falls_back_to_the_approximate_score() {
    let index = USearchVectorIndex::with_compression(4, None, i8_compression(4)).unwrap();
    let vector = unit(&[1.0, 0.0, 0.0, 0.0]);
    index.add_embedding("a".into(), vector.clone()).unwrap();

    let key = *index.state.read().id_to_key.get("a").unwrap();
    index.rescore.as_ref().unwrap().remove(key).unwrap();

    let hits = VectorSearchPort::search(&index, &vector, 1, 0.0).unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].score > 0.99, "score was {}", hits[0].score);
}

/// The predicate runs inside the graph traversal now, so the properties
/// that used to fall out of post-filtering have to be pinned explicitly:
/// nothing out of scope survives, and a scope with enough members still
/// yields a full `top_k`.
#[test]
fn a_scoped_search_returns_only_allowed_documents_and_still_fills_top_k() {
    scoped_search_fills_top_k_with_the_best_in_scope_chunks(EXACT_SEARCH_MAX_VECTORS);
}

#[test]
fn small_scope_in_large_index_exact_scores_only_eligible_vectors_and_caps_k() {
    let mut index =
        USearchVectorIndex::with_compression(4, None, VectorIndexCompression::None).unwrap();
    index.exact_search_max = 8;
    let query = unit(&[1.0, 0.0, 0.0, 0.0]);
    for i in 0..80 {
        let document_id = if i < 3 { "allowed" } else { "other" };
        let mut vector = vec![1.0f32, i as f32 / 1000.0, 0.0, 0.0];
        let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
        vector.iter_mut().for_each(|value| *value /= norm);
        index
            .add_embedding_with_content(
                format!("emb_{i}"),
                vector,
                format!("text {i}"),
                format!("chunk_{i}"),
                document_id.to_string(),
            )
            .unwrap();
    }
    let scope = HashSet::from(["allowed".to_string()]);
    let results = VectorSearchPort::search_scoped(&index, &query, 10, 0.0, Some(&scope)).unwrap();
    assert_eq!(results.len(), 3, "top_k is capped by scoped cardinality");
    assert!(results.iter().all(|result| result.doc_id == "allowed"));
    assert!(results
        .windows(2)
        .all(|pair| pair[0].score >= pair[1].score));
}

#[test]
fn narrow_scope_cost_and_exact_ids_depend_on_scope_not_corpus_size() {
    const DIM: usize = 8;
    const CORPUS: usize = 10_000;
    const SCOPE: usize = 100;
    const TOP_K: usize = 7;
    let index =
        USearchVectorIndex::with_compression(DIM, None, VectorIndexCompression::None).unwrap();
    let query = synthetic_vector(100, DIM, 0.8);
    let mut eligible = Vec::with_capacity(SCOPE);
    for i in 0..CORPUS {
        let vector = synthetic_vector(i as u64 + 100, DIM, 0.8);
        let in_scope = i < SCOPE;
        let document_id = if in_scope {
            format!("allowed-{i}")
        } else {
            format!("other-{i}")
        };
        let id = format!("chunk-{i}");
        index
            .add_embedding_with_content(
                id.clone(),
                vector.clone(),
                String::new(),
                id.clone(),
                document_id,
            )
            .unwrap();
        if in_scope {
            eligible.push((id, cosine_similarity_simd(&query, &vector)));
        }
    }

    let scope: HashSet<String> = (0..SCOPE).map(|i| format!("allowed-{i}")).collect();
    let hits = VectorSearchPort::search_scoped(&index, &query, TOP_K, 0.0, Some(&scope)).unwrap();
    eligible.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let expected: Vec<_> = eligible.iter().take(TOP_K).map(|(id, _)| id).collect();
    let actual: Vec<_> = hits.iter().map(|hit| &hit.chunk_id).collect();

    assert_eq!(actual, expected, "scoped results match exact ground truth");
    assert_eq!(
        index.exact_scope_scored.load(Ordering::Relaxed),
        SCOPE,
        "the exact scorer visits only eligible vectors, independent of the 10k corpus"
    );
}

/// The same contract past the exact-scan size, where the scope runs as a
/// predicate inside the graph traversal instead.
#[test]
fn a_scoped_graph_search_returns_only_allowed_documents_and_still_fills_top_k() {
    scoped_search_fills_top_k_with_the_best_in_scope_chunks(0);
}

fn scoped_search_fills_top_k_with_the_best_in_scope_chunks(exact_search_max: usize) {
    const DIM: usize = 16;
    const TOP_K: usize = 10;
    let corpus = widening_corpus(DIM, 512);
    let mut index = widening_index(DIM, &corpus);
    index.exact_search_max = exact_search_max;
    let query = synthetic_vector(2_024, DIM, 0.8);

    // Every fifth document, so the scope is both large enough to fill
    // `top_k` and selective enough that post-filtering would have had to
    // widen.
    let scope: HashSet<String> = (0..512)
        .filter(|i| i % 5 == 0)
        .map(|i| format!("doc{i}"))
        .collect();

    let hits = VectorSearchPort::search_scoped(&index, &query, TOP_K, 0.0, Some(&scope)).unwrap();

    assert_eq!(hits.len(), TOP_K, "a satisfiable scope must fill top_k");
    assert!(
        hits.iter().all(|h| scope.contains(&h.doc_id)),
        "an out-of-scope document survived the predicate: {hits:?}"
    );

    // …and they are the best in-scope chunks, not merely in-scope ones.
    let mut in_scope: Vec<(String, f32)> = corpus
        .iter()
        .filter(|(id, _)| scope.contains(&format!("doc{}", id.trim_start_matches("chunk"))))
        .map(|(id, v)| (id.clone(), cosine_similarity_naive(&query, v)))
        .collect();
    in_scope.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(CmpOrdering::Equal));
    let expected: Vec<&str> = in_scope
        .iter()
        .take(TOP_K)
        .map(|(id, _)| id.as_str())
        .collect();
    assert_eq!(
        hits.iter().map(|h| h.chunk_id.as_str()).collect::<Vec<_>>(),
        expected
    );
}

/// A scope of one document out of five hundred: the first traversal will
/// not find it, and the single exhaustive retry must.
#[test]
fn a_scope_of_one_distant_document_is_still_reached() {
    const DIM: usize = 16;
    let corpus = widening_corpus(DIM, 512);
    let index = widening_index(DIM, &corpus);
    let query = synthetic_vector(2_024, DIM, 0.8);

    // The worst-ranked chunk that still clears the bar. A negative cosine
    // is filtered out by the threshold, so scoping to one of those would
    // be testing the threshold rather than the traversal.
    let worst = exact_cosine_ranking(&query, &corpus)
        .into_iter()
        .rfind(|id| {
            corpus
                .iter()
                .find(|(candidate, _)| candidate == id)
                .is_some_and(|(_, v)| cosine_similarity_naive(&query, v) > 0.05)
        })
        .expect("some chunk clears the bar");
    let doc = format!("doc{}", worst.trim_start_matches("chunk"));
    let scope: HashSet<String> = [doc.clone()].into();

    let hits = VectorSearchPort::search_scoped(&index, &query, 5, 0.0, Some(&scope)).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].doc_id, doc);
    assert_eq!(hits[0].chunk_id, worst);
}

#[test]
fn an_empty_scope_returns_nothing_without_searching() {
    let index = widening_index(16, &widening_corpus(16, 32));
    let scope: HashSet<String> = HashSet::new();
    assert!(VectorSearchPort::search_scoped(
        &index,
        &synthetic_vector(1, 16, 0.8),
        5,
        0.0,
        Some(&scope)
    )
    .unwrap()
    .is_empty());
}

fn widening_corpus(dim: usize, count: u64) -> Vec<(String, Vec<f32>)> {
    (0..count)
        .map(|i| (format!("chunk{i}"), synthetic_vector(i, dim, 0.8)))
        .collect()
}

/// Forced onto the graph path: these tests pin the predicate and the
/// exhaustive retry, which an exact scan never needs.
fn widening_index(dim: usize, corpus: &[(String, Vec<f32>)]) -> USearchVectorIndex {
    let mut index = USearchVectorIndex::new(dim, None).unwrap();
    index.exact_search_max = 0;
    for (i, (id, vector)) in corpus.iter().enumerate() {
        index
            .add_embedding_with_content(
                id.clone(),
                vector.clone(),
                format!("passage {i}"),
                id.clone(),
                format!("doc{i}"),
            )
            .unwrap();
    }
    index
}

/// The logged waste: a scoped search whose scope was satisfiable but whose
/// threshold nothing could clear widened nine times across the whole index
/// before returning the empty list its first pass had already established.
/// The answer must stay empty — this pins the result, while the widening
/// loop now gives up once the nearest in-scope vector misses the bar.
#[test]
fn an_unreachable_threshold_still_returns_nothing() {
    const DIM: usize = 16;
    let corpus = widening_corpus(DIM, 128);
    let index = widening_index(DIM, &corpus);
    let query = synthetic_vector(7_777, DIM, 0.8);
    let scope: HashSet<String> = (0..128).map(|i| format!("doc{i}")).collect();

    let hits = VectorSearchPort::search_scoped(&index, &query, 10, 0.999_9, Some(&scope)).unwrap();

    assert!(hits.is_empty());
}

/// The property the early stop must not break. Here the scope — not the
/// threshold — is what empties the first candidate windows, so widening is
/// the only way to reach the answer and it has to keep going.
#[test]
fn a_scope_that_only_distant_documents_satisfy_is_still_reached() {
    const DIM: usize = 16;
    let corpus = widening_corpus(DIM, 128);
    let index = widening_index(DIM, &corpus);
    let query = synthetic_vector(7_777, DIM, 0.8);

    // Scope to the single worst-ranked document that still clears the bar,
    // so every early candidate window is filtered away entirely.
    let ranked = exact_cosine_ranking(&query, &corpus);
    let worst = ranked
        .iter()
        .rev()
        .find(|id| {
            corpus
                .iter()
                .find(|(candidate, _)| candidate == *id)
                .is_some_and(|(_, v)| cosine_similarity_naive(&query, v) > 0.05)
        })
        .expect("some chunk clears the bar")
        .clone();
    let doc = format!("doc{}", worst.trim_start_matches("chunk"));
    let scope: HashSet<String> = [doc.clone()].into();

    let hits = VectorSearchPort::search_scoped(&index, &query, 3, 0.05, Some(&scope)).unwrap();

    assert_eq!(hits.len(), 1, "widening must still reach a distant scope");
    assert_eq!(hits[0].doc_id, doc);
}
