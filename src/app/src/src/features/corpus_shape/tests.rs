//! Integration tests for the corpus-shape pipeline.
//!
//! These exercise the full RunClusteringUseCase against:
//! - in-memory SQLite (real schema, real repository),
//! - mock document + embedding repositories (from `support::mocks`),
//! - a programmable mock LLM (counts calls; always returns predictable JSON).
//!
//! Scope:
//!
//! - 5.3.1 — clustering end-to-end on a 3-blob synthetic lattice.
//! - 5.3.3 — re-running on identical input makes zero LLM calls.
//! - 5.3.4 — round-trip through SQLite preserves all fields.

#![cfg(test)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use futures::stream::Stream;
use parking_lot::Mutex;
use sqlx::SqlitePool;

use crate::application::ports::{
    GenerationOverride, LLMPort, RepositoryPort,
};
use crate::domain::embedding_constants::DEFAULT_EMBEDDING_MODEL_NAME;
use crate::domain::entities::Document;
use crate::domain::value_objects::Checksum;
use crate::features::corpus_shape::clustering::ClusteringParams;
use crate::features::corpus_shape::entity::LabelSource;
use crate::features::corpus_shape::labeling::RepresentativeDoc;
use crate::features::corpus_shape::repository::{
    ClusterRepositoryPort, SqliteClusterRepository,
};
use crate::features::corpus_shape::use_cases::run_clustering::DocumentContentPreviewPort;
use crate::features::corpus_shape::use_cases::RunClusteringUseCase;
use crate::features::embedding::entity::Embedding as EmbeddingEntity;
use crate::infrastructure::persistence::repositories::mocks::{
    MockDocumentRepository, MockEmbeddingRepository,
};
use crate::shared::domain_types::{ChunkId, ValidatedFilePath};
use crate::shared::error::Result;

// ============================================================================
// Mock LLM — deterministic + counts calls.
// ============================================================================

struct CountingMockLlm {
    calls: Arc<Mutex<usize>>,
}

impl CountingMockLlm {
    fn new() -> Self {
        Self {
            calls: Arc::new(Mutex::new(0)),
        }
    }
    fn call_count(&self) -> usize {
        *self.calls.lock()
    }
    fn counter(&self) -> Arc<Mutex<usize>> {
        Arc::clone(&self.calls)
    }
}

#[async_trait]
impl LLMPort for CountingMockLlm {
    async fn generate(
        &self,
        _prompt: &str,
        _context: &[String],
        _images: Option<Vec<String>>,
    ) -> Result<String> {
        *self.calls.lock() += 1;
        Ok(r#"{"label":"Mock Cluster","description":"A mocked description."}"#.to_string())
    }

    async fn generate_with_overrides(
        &self,
        _prompt: &str,
        _context: &[String],
        _images: Option<Vec<String>>,
        _overrides: GenerationOverride,
    ) -> Result<String> {
        *self.calls.lock() += 1;
        Ok(r#"{"label":"Mock Cluster","description":"A mocked description."}"#.to_string())
    }

    async fn generate_streaming(
        &self,
        _prompt: &str,
        _context: &[String],
        _images: Option<Vec<String>>,
    ) -> Result<Box<dyn Stream<Item = Result<String>> + Send + Unpin + '_>> {
        unimplemented!("streaming not used in corpus_shape tests")
    }

    fn model_name(&self) -> &str {
        "mock-llm"
    }
    fn max_context_tokens(&self) -> usize {
        4096
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

// ============================================================================
// Stub content-preview port.
// ============================================================================

struct StubPreview;

#[async_trait]
impl DocumentContentPreviewPort for StubPreview {
    async fn preview(&self, document_id: &str) -> Result<Option<RepresentativeDoc>> {
        Ok(Some(RepresentativeDoc {
            title: format!("doc-title-{}", document_id),
            content_preview: format!("Preview content for {}", document_id),
        }))
    }
}

// ============================================================================
// Helpers
// ============================================================================

async fn fresh_sqlite_pool() -> SqlitePool {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

fn make_document(id: &str, file_name: &str) -> Document {
    let path = ValidatedFilePath::new(std::path::PathBuf::from(format!(
        "/tmp/corpus-shape-tests/{}",
        file_name
    )))
    .unwrap();
    let checksum = Checksum::from_bytes(file_name.as_bytes());
    let now = Utc::now();
    Document::with_id(
        crate::shared::domain_types::DocumentId::from(id.to_string()),
        path,
        file_name.to_string(),
        None,
        "text/plain".to_string(),
        1024,
        now,
        now,
        checksum,
        crate::domain::entities::document::DocumentStatus::Indexed,
        None,
        crate::domain::entities::document::Language::default(),
        crate::domain::entities::document::Category::default(),
        0.5,
        0,
        None,
        0,
        String::new(),
    )
}

/// Seed the 3-blob synthetic lattice: 15 docs per blob + 3 stragglers.
///
/// Returns a pool with document + embedding rows populated via mock repos
/// (real SQLite holds the cluster_runs tables; embeddings are in-memory).
async fn seed_three_blob_vault() -> (
    SqlitePool,
    Arc<MockDocumentRepository>,
    Arc<MockEmbeddingRepository>,
) {
    let pool = fresh_sqlite_pool().await;
    let doc_repo = Arc::new(MockDocumentRepository::new());
    let emb_repo = Arc::new(MockEmbeddingRepository::new());

    let centers = [
        vec![0.0_f32, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        vec![10.0_f32, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0],
        vec![-10.0_f32, 10.0, -10.0, 10.0, -10.0, 10.0, -10.0, 10.0],
    ];
    let per_blob = 15;

    for (bi, center) in centers.iter().enumerate() {
        for pi in 0..per_blob {
            let doc_id = format!("doc-{}-{}", bi, pi);
            let file_name = format!("file-{}-{}.txt", bi, pi);

            // Doc row.
            doc_repo.add_document(make_document(&doc_id, &file_name));

            // One chunk per doc + one embedding per chunk.
            let chunk_id = format!("chunk-{}", doc_id);
            emb_repo.register_chunk_document(&chunk_id, &doc_id);

            let seed = (bi * 10_000 + pi) as u64;
            let vector = jittered(center, 0.15, seed);

            let embedding = EmbeddingEntity::new(
                ChunkId::from(chunk_id.clone()),
                DEFAULT_EMBEDDING_MODEL_NAME.to_string(),
                vector.len(),
            );
            use crate::application::ports::EmbeddingRepositoryPort;
            emb_repo.save(&embedding, vector).await.unwrap();
        }
    }

    (pool, doc_repo, emb_repo)
}

fn jittered(center: &[f32], jitter: f32, seed: u64) -> Vec<f32> {
    let mut out = Vec::with_capacity(center.len());
    let mut state = seed
        .wrapping_add(0x9E37_79B9_7F4A_7C15)
        .wrapping_mul(0xBF58_476D_1CE4_E5B9);
    for &c in center.iter() {
        state ^= state >> 30;
        state = state.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        state ^= state >> 27;
        state = state.wrapping_mul(0x94D0_49BB_1331_11EB);
        state ^= state >> 31;
        let unit = (state as u32 as f32) / (u32::MAX as f32);
        let n = (unit - 0.5) * 2.0 * jitter;
        out.push(c + n);
    }
    out
}

fn build_use_case(
    pool: SqlitePool,
    doc_repo: Arc<MockDocumentRepository>,
    emb_repo: Arc<MockEmbeddingRepository>,
    llm: Arc<dyn LLMPort>,
) -> RunClusteringUseCase {
    let doc_port: Arc<dyn RepositoryPort<Document>> = doc_repo;
    let emb_port: Arc<dyn crate::application::ports::EmbeddingRepositoryPort> = emb_repo;
    let cluster_repo: Arc<dyn ClusterRepositoryPort> =
        Arc::new(SqliteClusterRepository::new(pool));
    let preview: Arc<dyn DocumentContentPreviewPort> = Arc::new(StubPreview);

    RunClusteringUseCase::new(doc_port, emb_port, cluster_repo, preview, Some(llm))
        .with_params(ClusteringParams {
            min_cluster_size: 5,
            min_samples: 3,
        })
}

// ============================================================================
// Tests
// ============================================================================

#[tokio::test]
async fn full_pipeline_finds_three_clusters_on_synthetic_vault() {
    let (pool, doc_repo, emb_repo) = seed_three_blob_vault().await;
    let llm = CountingMockLlm::new();
    let counter = llm.counter();
    let llm_arc: Arc<dyn LLMPort> = Arc::new(llm);

    let use_case = build_use_case(pool.clone(), doc_repo, emb_repo, llm_arc);
    let outcome = use_case.execute().await.unwrap();

    assert_eq!(
        outcome.clusters.len(),
        3,
        "expected 3 clusters, got {}",
        outcome.clusters.len()
    );
    // First run: every cluster needed an LLM call.
    assert_eq!(*counter.lock(), 3);
    // Each cluster got a real label from the mock LLM.
    for cluster in &outcome.clusters {
        assert!(!cluster.label.is_empty());
        assert_eq!(cluster.label_source, LabelSource::Llm);
    }
}

#[tokio::test]
async fn rerun_on_identical_input_triggers_zero_llm_calls() {
    let (pool, doc_repo, emb_repo) = seed_three_blob_vault().await;

    let llm1 = Arc::new(CountingMockLlm::new());
    let counter1 = llm1.counter();
    let llm1_port: Arc<dyn LLMPort> = llm1;
    let uc1 = build_use_case(pool.clone(), doc_repo.clone(), emb_repo.clone(), llm1_port);
    let first = uc1.execute().await.unwrap();
    assert_eq!(*counter1.lock(), first.clusters.len());

    // Same data, second pass → fingerprints should match exactly, no LLM calls.
    let llm2 = Arc::new(CountingMockLlm::new());
    let counter2 = llm2.counter();
    let llm2_port: Arc<dyn LLMPort> = llm2;
    let uc2 = build_use_case(pool.clone(), doc_repo, emb_repo, llm2_port);
    let second = uc2.execute().await.unwrap();
    assert_eq!(
        *counter2.lock(),
        0,
        "re-running on identical data must not call the LLM again"
    );
    // Labels should be preserved exactly (inherited_exact path).
    assert_eq!(first.clusters.len(), second.clusters.len());
    for cluster in &second.clusters {
        assert_eq!(cluster.label_source, LabelSource::InheritedExact);
    }
}

#[tokio::test]
async fn sqlite_round_trip_preserves_clusters() {
    let (pool, doc_repo, emb_repo) = seed_three_blob_vault().await;
    let llm = Arc::new(CountingMockLlm::new());
    let llm_port: Arc<dyn LLMPort> = llm;

    let uc = build_use_case(pool.clone(), doc_repo, emb_repo, llm_port);
    let outcome = uc.execute().await.unwrap();

    let repo = SqliteClusterRepository::new(pool);
    let latest = repo.get_latest_run().await.unwrap().unwrap();
    assert_eq!(latest.id, outcome.run.id);
    assert_eq!(latest.cluster_count, outcome.clusters.len() as i64);

    let persisted = repo.get_clusters_for_run(&latest.id).await.unwrap();
    assert_eq!(persisted.len(), outcome.clusters.len());

    // Rebuild a mapping by fingerprint so we can cross-compare without relying on insertion order.
    let mut by_fp: std::collections::HashMap<String, _> = std::collections::HashMap::new();
    for c in persisted {
        by_fp.insert(c.fingerprint.clone(), c);
    }
    for original in &outcome.clusters {
        let reloaded = by_fp.get(&original.fingerprint).unwrap();
        assert_eq!(reloaded.label, original.label);
        assert_eq!(reloaded.description, original.description);
        assert_eq!(reloaded.member_doc_ids.len(), original.member_doc_ids.len());
        assert_eq!(reloaded.centroid.len(), original.centroid.len());
        assert_eq!(reloaded.label_source, original.label_source);
    }
}
