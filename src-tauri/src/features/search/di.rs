//! Search feature dependency injection.
//!
//! Owns the shared USearch vector index (referenced by IndexingModule
//! too — the composition root passes `vector_search` across).

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::application::ports::{
    DocumentRepository, EmbeddingPort, TextSearchPort, VectorSearchPort,
};
use crate::domain::downloaded_model::DownloadedModel;
use crate::domain::embedding_constants::DEFAULT_EMBEDDING_DIM;
use crate::features::embedding::candle_service::{has_loadable_weights, CandleEmbeddingService};
use crate::features::embedding::late_chunking::{strategy_identity, EmbeddingStrategy};
use crate::features::embedding::service::DynamicEmbedding;
use crate::features::search::engine::bm25::BM25Search;
use crate::features::search::engine::hybrid::{HybridSearchService, SearchConfig, SearchMode};
use crate::features::search::engine::reranker::{LazyReranker, Reranker};
use crate::features::search::engine::text_search::SqliteTextSearch;
use crate::features::search::engine::vector_search::persistence::open_or_rebuild;
use crate::features::search::engine::vector_search::{
    IndexPersistence, USearchVectorIndex, VectorIndexCompression,
};
use crate::features::search::enrichment_service::SearchEnrichmentService;
use crate::features::search::use_cases::{HybridSearchUseCase, SemanticSearchUseCase};
use crate::features::search::{BM25SearchTrait, HybridSearchTrait, SearchServiceTrait};
use crate::infrastructure::persistence::repositories::DocumentRepositoryImpl;
use crate::infrastructure::services::traits::SearchEnrichmentServiceTrait;
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};

#[derive(Clone)]
pub struct SearchDi {
    pub runtime_index: Arc<super::engine::vector_search::runtime_index::RuntimeVectorIndex>,
    pub semantic_search_use_case: Arc<SemanticSearchUseCase>,
    pub hybrid_search_use_case: Arc<HybridSearchUseCase>,

    // Services
    pub search_service: Arc<dyn SearchServiceTrait>,
    pub bm25_search: Arc<dyn BM25SearchTrait>,
    pub hybrid_search_service: Arc<dyn HybridSearchTrait>,
    pub reranker: Arc<dyn Reranker>,
    pub search_enrichment_service: Arc<dyn SearchEnrichmentServiceTrait>,

    // Ports (exported so IndexingModule can write to the same USearch index)
    pub vector_search: Arc<dyn VectorSearchPort>,
    pub document_repo: Arc<dyn DocumentRepository>,

    /// Writes the index and its manifest. `None` until an embedding model is
    /// active, because there is no generation to stamp a manifest with before
    /// then. Held by the composition root so shutdown can flush.
    pub index_persistence: Option<Arc<IndexPersistence>>,
}

pub async fn build(
    db_pool: SqlitePool,
    usearch_index_path: std::path::PathBuf,
    models_path: std::path::PathBuf,
    model_provider: Arc<dyn crate::application::ports::LoadedEmbeddingModelPort>,
    active_embedding_dimension: Option<usize>,
) -> Result<SearchDi> {
    // Defaults: the historical uncompressed, chunk-first index. The composition
    // root (`src/interfaces/di/modules.rs`) reads the user's settings and calls
    // `build_with_compression` directly.
    build_with_compression(
        db_pool,
        usearch_index_path,
        models_path,
        model_provider,
        active_embedding_dimension,
        VectorIndexCompression::None,
        EmbeddingStrategy::default(),
    )
    .await
}

/// `build` with an explicit vector storage configuration and embedding strategy.
///
/// See [`VectorIndexCompression`] for what the non-default configurations cost
/// and buy. Turning compression on for an existing vault is safe: the index
/// path and the persisted sidecar both carry the configuration, so the old
/// index is left untouched on disk and a fresh one is rebuilt from the
/// f32 vectors SQLite already holds.
///
/// `strategy` is not used to embed anything here — it only names the vector
/// space. Late-chunked vectors are pooled from a shared forward pass and are
/// not interchangeable with chunk-first vectors of the same model, so the
/// strategy joins the artifact digest in the embedding identity that selects
/// which persisted vectors this index is rebuilt from.
#[allow(clippy::too_many_arguments)]
pub async fn build_with_compression(
    db_pool: SqlitePool,
    usearch_index_path: std::path::PathBuf,
    models_path: std::path::PathBuf,
    model_provider: Arc<dyn crate::application::ports::LoadedEmbeddingModelPort>,
    active_embedding_dimension: Option<usize>,
    compression: VectorIndexCompression,
    strategy: EmbeddingStrategy,
) -> Result<SearchDi> {
    let repository =
        crate::infrastructure::persistence::repositories::DownloadedModelRepository::new(
            db_pool.clone(),
        );
    let active = repository.get_active_embedding_model().await?;
    let identity = index_identity(active.as_ref(), strategy);
    // Every vector space has its own index files. A model switch cannot wipe its
    // predecessor — and neither can a compression switch, which reshapes the
    // stored vectors just as completely as a model change does. The compression
    // layout therefore joins the model identity in the generation key, so
    // toggling it rebuilds into its own files instead of mixing two vector
    // spaces in one index. `None` contributes no suffix, so existing vaults keep
    // their current filenames byte for byte.
    let generation = match &identity {
        Some(id) => id.replace(':', "-"),
        None => "unconfigured".to_string(),
    };
    let generation = match compression.layout_token() {
        Some(layout) => format!("{}-{}", generation, layout),
        None => generation,
    };
    // A backup restore leaves this marker because archives carry no vectors.
    let reembed_marker = usearch_index_path
        .parent()
        .map(|dir| dir.join(crate::shared::constants::REEMBED_MARKER_FILE));
    let usearch_index_path =
        usearch_index_path.with_file_name(format!("usearch-{}.usearch", generation));
    // Candle-era models vary in output dimension (384 / 768 / 1024). The
    // active model's true dimension comes from its config.json::hidden_size,
    // resolved by the composition root before this point. If the active
    // model is unset (or its config can't be read), fall back to the
    // persisted index dimension — that way we keep an existing index intact
    // until a model load actually proves its dimension.
    let dimension = active_embedding_dimension
        .or_else(|| {
            crate::features::search::engine::vector_search::read_dimension(&usearch_index_path)
        })
        .unwrap_or(DEFAULT_EMBEDDING_DIM);

    // Compare the persisted vector layout against the requested one. If the
    // dimension or the compression configuration differs, wipe the index
    // (USearch can't reshape at runtime) so the new configuration can start
    // from an empty index. The wipe is logged inside the helper. Loading the
    // index reads it from disk, so both steps run off the runtime thread.
    let open_path = usearch_index_path.clone();
    let open_compression = compression.clone();
    let usearch_index = tokio::task::spawn_blocking(move || {
        let _check = crate::features::search::engine::vector_search::ensure_index_layout_match(
            &open_path,
            dimension,
            &open_compression,
        )?;
        // USearch: single index that implements both VectorSearchPort and SearchServiceTrait.
        USearchVectorIndex::open_or_create_with_compression(dimension, open_path, open_compression)
            .map_err(|e| {
                AppError::InternalError(format!("Failed to initialize USearch index: {}", e))
            })
    })
    .await
    .map_err(|e| AppError::InternalError(format!("Vector index open task failed: {e}")))??;
    // Indexing publishes into this index a document at a time; saving the
    // whole corpus after each one is what made indexing a folder quadratic.
    let usearch_index = Arc::new(usearch_index.with_coalesced_saves());

    // SQLite stays authoritative, but agreeing with it is now something the
    // manifest can establish without reading every vector back. A rebuild is
    // still what recovers from a crash between an SQLite commit and a save —
    // it just no longer happens on launches where nothing changed.
    let mut coverage: Option<(usize, usize)> = None;
    let mut index_persistence: Option<Arc<IndexPersistence>> = None;
    if let Some(identity) = &identity {
        let restore = || {
            let pool = db_pool.clone();
            let identity = identity.clone();
            async move {
                crate::features::embedding::generation::restore(&pool, &identity, dimension).await
            }
        };
        open_or_rebuild(
            &usearch_index,
            &db_pool,
            &usearch_index_path,
            identity,
            &generation,
            dimension,
            restore,
        )
        .await?;

        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM text_chunks WHERE content != ''")
            .fetch_one(&db_pool)
            .await?;
        // The backfill embeds one stored chunk at a time. That *is* the
        // chunk-first path, so it can only fill a chunk-first generation —
        // running it under the late-chunking identity would stamp chunk-first
        // vectors with a label promising they were pooled from a shared forward
        // pass, and the index would then mix the two. Late chunking therefore
        // starts from whatever that generation already holds and fills in as
        // documents are indexed.
        if usearch_index.count() < total as usize && !strategy.is_late_chunking() {
            // `identity` is only set for a local model with a recorded
            // artifact identity, so this always matches here.
            let stored = active.as_ref().and_then(|m| {
                Some((
                    m.location().enclosing_dir()?,
                    m.embedding_artifact_identity()?.clone(),
                ))
            });
            if let Some((dir, artifact)) = stored {
                let model = tokio::task::spawn_blocking(move || {
                    CandleEmbeddingService::open(&dir, artifact).map(|s| s.with_strategy(strategy))
                })
                .await
                .map_err(|e| {
                    AppError::InternalError(format!("Embedding model load task failed: {e}"))
                })??;
                crate::features::embedding::generation::prepare(&db_pool, &model).await?;
                // The backfill moved SQLite, so this pass always rebuilds.
                open_or_rebuild(
                    &usearch_index,
                    &db_pool,
                    &usearch_index_path,
                    identity,
                    &generation,
                    dimension,
                    restore,
                )
                .await?;
            }
        } else if usearch_index.count() < total as usize {
            tracing::info!(
                present = usearch_index.count(),
                total,
                "Late chunking is on and this generation is incomplete; the missing chunks get \
                 their vectors when their documents are next indexed"
            );
        }
        coverage = Some((usearch_index.count(), usize::try_from(total).unwrap_or(0)));

        let persistence = Arc::new(IndexPersistence::new(
            Arc::clone(&usearch_index),
            db_pool.clone(),
            identity.clone(),
            generation.clone(),
            dimension,
            usearch_index_path.clone(),
        ));
        tokio::spawn(Arc::clone(&persistence).run());
        index_persistence = Some(persistence);
    }

    if let Some(marker) = reembed_marker.as_deref().filter(|m| m.exists()) {
        settle_reembed_marker(marker, coverage, strategy.is_late_chunking());
    }

    tracing::info!(
        size = usearch_index.count(),
        path = %usearch_index_path.display(),
        compression = ?compression,
        stored_bytes_per_vector = compression.bytes_per_vector(dimension),
        "USearch vector index ready"
    );

    let runtime_index = Arc::new(
        super::engine::vector_search::runtime_index::RuntimeVectorIndex::new(
            usearch_index,
            identity,
            usearch_index_path,
        ),
    );
    let vector_search = runtime_index.clone() as Arc<dyn VectorSearchPort>;
    let search_service = runtime_index.clone() as Arc<dyn SearchServiceTrait>;

    let text_search = Arc::new(SqliteTextSearch::new(db_pool.clone())) as Arc<dyn TextSearchPort>;
    let document_repo =
        Arc::new(DocumentRepositoryImpl::new(db_pool.clone())) as Arc<dyn DocumentRepository>;

    let bm25_search = Arc::new(BM25Search::new(db_pool.clone())) as Arc<dyn BM25SearchTrait>;
    let search_enrichment_service = Arc::new(SearchEnrichmentService::new(db_pool.clone()))
        as Arc<dyn SearchEnrichmentServiceTrait>;
    let reranker = Arc::new(LazyReranker::new(
        models_path.join("reranker").join("model.safetensors"),
    )) as Arc<dyn Reranker>;

    let hybrid_config = SearchConfig {
        mode: SearchMode::Hybrid,
        vector_weight: crate::shared::constants::DEFAULT_VECTOR_FUSION_WEIGHT,
        keyword_weight: crate::shared::constants::DEFAULT_KEYWORD_FUSION_WEIGHT,
        min_score: crate::shared::constants::MIN_SIMILARITY_SCORE,
        // Keep direct-search reranking opt-in until a representative corpus
        // demonstrates a quality gain that justifies its latency. Chat uses
        // the same provider through its existing retrieval tuning setting.
        enable_reranking: false,
        recency_boost: 1.0,
        max_results: 100,
        // Retained only for controlled experiments. The production evaluation
        // found that the third branch added cost without improving the good
        // two-way fusion configuration.
        sparse_enabled: false,
    };

    let dynamic_embedding =
        Arc::new(DynamicEmbedding::new(model_provider.clone())) as Arc<dyn EmbeddingPort>;
    let hybrid_search_service = Arc::new(
        HybridSearchService::new(
            search_service.clone(),
            bm25_search.clone(),
            db_pool,
            search_enrichment_service.clone(),
            hybrid_config,
        )
        .with_reranker(Arc::clone(&reranker)),
    ) as Arc<dyn HybridSearchTrait>;

    let semantic_search_use_case = Arc::new(SemanticSearchUseCase::new(
        dynamic_embedding.clone(),
        vector_search.clone(),
    ));
    let hybrid_search_use_case = Arc::new(HybridSearchUseCase::new(
        dynamic_embedding,
        vector_search.clone(),
        text_search,
    ));

    Ok(SearchDi {
        runtime_index,
        semantic_search_use_case,
        hybrid_search_use_case,
        search_service,
        bm25_search,
        hybrid_search_service,
        reranker,
        search_enrichment_service,
        vector_search,
        document_repo,
        index_persistence,
    })
}

/// The vector-space identity of the active embedding model, read from its
/// row: the recorded artifact identity plus the strategy. `None` means there
/// is no usable local model — nothing active, a remote model, missing files,
/// or a row without an identity. Hashes nothing.
fn index_identity(active: Option<&DownloadedModel>, strategy: EmbeddingStrategy) -> Option<String> {
    let model = active?;
    let dir = model.location().enclosing_dir()?;
    let Some(artifact) = model.embedding_artifact_identity() else {
        // The repository refuses to activate a local model without an
        // identity. Startup does not fail over it; the embedding loader
        // refuses the model and asks the user to activate it again.
        tracing::error!(
            model_id = %model.model_id(),
            "Active embedding model has no recorded artifact identity; treating it as unconfigured"
        );
        return None;
    };
    // Weights may be safetensors or, for a checkpoint that never
    // published a conversion, the `pytorch_model.bin` pickle.
    let files_present = ["config.json", "tokenizer.json"]
        .iter()
        .all(|name| dir.join(name).is_file())
        && has_loadable_weights(&dir);
    files_present.then(|| strategy_identity(artifact.as_str(), strategy))
}

/// Search's registrar surface on `Container`.
impl Container {
    // Search (from SearchModule)
    pub fn semantic_search_use_case(&self) -> Arc<SemanticSearchUseCase> {
        Arc::clone(self.search.semantic_search_use_case())
    }

    pub fn hybrid_search_use_case(&self) -> Arc<HybridSearchUseCase> {
        Arc::clone(self.search.hybrid_search_use_case())
    }

    /// Get vector search service (for legacy search commands, from SearchModule)
    pub fn search_service(&self) -> Arc<dyn SearchServiceTrait> {
        Arc::clone(self.search.search_service())
    }

    /// Get hybrid search service (for legacy search commands, from SearchModule)
    pub fn hybrid_search(&self) -> Arc<dyn HybridSearchTrait> {
        Arc::clone(self.search.hybrid_search_service())
    }

    /// Get the shared lazy reranker used by every retrieval path.
    pub fn reranker(&self) -> Arc<dyn Reranker> {
        Arc::clone(self.search.reranker())
    }

    /// Get search enrichment service (for legacy search commands, from SearchModule)
    pub fn search_enrichment_service(&self) -> Arc<dyn SearchEnrichmentServiceTrait> {
        Arc::clone(self.search.search_enrichment_service())
    }

    /// Repository accessor for access tracking (from SearchModule)
    pub fn document_repository(&self) -> Arc<dyn DocumentRepository> {
        Arc::clone(self.search.document_repo())
    }
}

/// Clear the post-restore marker once every chunk has a vector again, or say
/// plainly what is still missing. Sparse postings and clusters have no
/// startup backfill; they come back as documents are re-indexed and as the
/// corpus view is recomputed.
fn settle_reembed_marker(
    marker: &std::path::Path,
    coverage: Option<(usize, usize)>,
    late_chunking: bool,
) {
    match coverage {
        Some((present, total)) if present >= total => match std::fs::remove_file(marker) {
            Ok(()) => tracing::info!(
                vectors = present,
                "Restored library is fully embedded again; cleared the re-embed marker"
            ),
            Err(e) => tracing::warn!(error = %e, "Could not clear the re-embed marker"),
        },
        Some((present, total)) if late_chunking => tracing::warn!(
            present,
            total,
            "Restored library is missing vectors and late chunking does not backfill; \
             re-index documents to restore semantic search"
        ),
        Some((present, total)) => tracing::warn!(
            present,
            total,
            "Restored library is still missing vectors after the backfill; keeping the \
             re-embed marker for the next launch"
        ),
        None => tracing::info!(
            "Restored library needs embeddings; they will be generated once an \
             embedding model is active"
        ),
    }
}

#[cfg(test)]
mod reembed_marker_tests {
    use super::settle_reembed_marker;

    fn marker() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir
            .path()
            .join(crate::shared::constants::REEMBED_MARKER_FILE);
        std::fs::write(&path, b"2026-09-16T00:00:00Z").unwrap();
        (dir, path)
    }

    #[test]
    fn complete_coverage_clears_the_marker() {
        let (_dir, path) = marker();
        settle_reembed_marker(&path, Some((10, 10)), false);
        assert!(!path.exists());
    }

    #[test]
    fn an_empty_library_counts_as_complete() {
        let (_dir, path) = marker();
        settle_reembed_marker(&path, Some((0, 0)), true);
        assert!(!path.exists());
    }

    #[test]
    fn missing_vectors_keep_the_marker_for_either_strategy() {
        let (_dir, path) = marker();
        settle_reembed_marker(&path, Some((3, 10)), false);
        assert!(path.exists());
        settle_reembed_marker(&path, Some((3, 10)), true);
        assert!(path.exists());
    }

    #[test]
    fn no_active_model_keeps_the_marker() {
        let (_dir, path) = marker();
        settle_reembed_marker(&path, None, false);
        assert!(path.exists());
    }
}

#[cfg(test)]
mod index_identity_tests {
    use super::index_identity;
    use crate::domain::downloaded_model::{DownloadedModel, ModelLocation};
    use crate::domain::model_metadata::ModelType;
    use crate::domain::value_objects::ArtifactIdentity;
    use crate::features::embedding::candle_service::WEIGHTS_SAFETENSORS;
    use crate::features::embedding::late_chunking::{strategy_identity, EmbeddingStrategy};
    use std::path::Path;

    fn recorded() -> ArtifactIdentity {
        ArtifactIdentity::from_digest(&[3; 32])
    }

    fn model(location: ModelLocation, identity: Option<ArtifactIdentity>) -> DownloadedModel {
        DownloadedModel::from_db(
            "row-1".into(),
            "embedder".into(),
            "org/embedder".into(),
            location,
            0,
            ModelType::TextEmbeddings,
            "bert".into(),
            chrono::Utc::now(),
            None,
            0,
            false,
            true,
            None,
            false,
            identity,
        )
    }

    fn local(dir: &Path) -> ModelLocation {
        ModelLocation::LocalDirectory {
            path: dir.to_path_buf(),
        }
    }

    fn model_dir(files: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for name in files {
            std::fs::write(dir.path().join(name), b"{}").unwrap();
        }
        dir
    }

    const COMPLETE: [&str; 3] = ["config.json", "tokenizer.json", WEIGHTS_SAFETENSORS];

    #[test]
    fn no_active_model_is_unconfigured() {
        assert_eq!(index_identity(None, EmbeddingStrategy::ChunkFirst), None);
    }

    #[test]
    fn local_model_is_named_by_its_recorded_identity_and_strategy() {
        let dir = model_dir(&COMPLETE);
        let active = model(local(dir.path()), Some(recorded()));
        for strategy in [
            EmbeddingStrategy::ChunkFirst,
            EmbeddingStrategy::LateChunking,
        ] {
            assert_eq!(
                index_identity(Some(&active), strategy),
                Some(strategy_identity(recorded().as_str(), strategy))
            );
        }
    }

    #[test]
    fn local_model_without_a_recorded_identity_is_unconfigured() {
        let dir = model_dir(&COMPLETE);
        let active = model(local(dir.path()), None);
        assert_eq!(
            index_identity(Some(&active), EmbeddingStrategy::ChunkFirst),
            None
        );
    }

    #[test]
    fn remote_model_is_unconfigured() {
        let active = model(ModelLocation::RemoteOllama, None);
        assert_eq!(
            index_identity(Some(&active), EmbeddingStrategy::ChunkFirst),
            None
        );
    }

    #[test]
    fn local_model_with_missing_files_is_unconfigured() {
        for files in [
            &["config.json", WEIGHTS_SAFETENSORS][..],
            &["config.json", "tokenizer.json"][..],
        ] {
            let dir = model_dir(files);
            let active = model(local(dir.path()), Some(recorded()));
            assert_eq!(
                index_identity(Some(&active), EmbeddingStrategy::ChunkFirst),
                None,
                "files present: {files:?}"
            );
        }
    }
}
