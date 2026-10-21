//! Search feature dependency injection.
//!
//! Owns the shared USearch vector index (referenced by IndexingModule
//! too — the composition root passes `vector_search` across).

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::features::search::engine::bm25::BM25Search;
use crate::features::search::engine::reranker::{LazyReranker, Reranker};
use crate::features::search::engine::text_search::SqliteTextSearch;
use crate::features::search::engine::vector_search::persistence::open_or_rebuild_streaming;
use crate::features::search::engine::vector_search::IndexPersistence;
use crate::features::search::enrichment_service::SearchEnrichmentService;
use crate::features::search::trait_def::SearchEnrichmentServiceTrait;
use crate::features::search::use_cases::{HybridSearchUseCase, SemanticSearchUseCase};
use crate::infrastructure::persistence::repositories::DocumentRepositoryImpl;
use crate::infrastructure::vector_index::{USearchVectorIndex, VectorIndexCompression};
use crate::interfaces::di::Container;
use lattice_core::application::ports::{
    BM25SearchTrait, ChunkRepositoryPort, DocumentRepository, EmbeddingPort, SearchServiceTrait,
    TextSearchPort, VectorSearchPort,
};
use lattice_core::domain::models::embedding_defaults::DEFAULT_EMBEDDING_DIM;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::background::BackgroundTasks;

#[derive(Clone)]
pub struct SearchDi {
    pub runtime_index: Arc<super::engine::vector_search::runtime_index::RuntimeVectorIndex>,
    pub semantic_search_use_case: Arc<SemanticSearchUseCase>,
    pub hybrid_search_use_case: Arc<HybridSearchUseCase>,

    // Services
    pub search_service: Arc<dyn SearchServiceTrait>,
    pub bm25_search: Arc<dyn BM25SearchTrait>,
    pub search_enrichment_service: Arc<dyn SearchEnrichmentServiceTrait>,

    // Ports (exported so IndexingModule can write to the same USearch index)
    pub vector_search: Arc<dyn VectorSearchPort>,
    pub document_repo: Arc<dyn DocumentRepository>,
}

/// Opens the active embedding model for the backfill. It runs at most once,
/// and only when the index is missing vectors.
pub type BackfillModelLoader = Box<
    dyn FnOnce() -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<Box<dyn EmbeddingPort>>> + Send>,
        > + Send,
>;

/// The vector space the index opens for. The composition root resolves it
/// from the active embedding model.
pub struct VectorSpace {
    /// The identity of the persisted vectors. `None` means there is no usable
    /// local model: nothing active, a remote model, missing files, or a row
    /// without a recorded artifact identity.
    pub identity: Option<String>,
    /// Opens the active model for the backfill. Present whenever `identity` is.
    pub backfill_model: Option<BackfillModelLoader>,
}

/// Open the vector index for `vector_space` with an explicit storage
/// configuration, and build the search services over it.
///
/// See [`VectorIndexCompression`] for what the non-default configurations cost
/// and buy. Turning compression on for an existing vault is safe: the index
/// path and the persisted sidecar both carry the configuration, so the old
/// index is left untouched on disk and a fresh one is rebuilt from the
/// f32 vectors SQLite already holds.
pub async fn build_with_compression(
    db_pool: SqlitePool,
    usearch_index_path: std::path::PathBuf,
    models_path: std::path::PathBuf,
    embedding: Arc<dyn EmbeddingPort>,
    active_embedding_dimension: Option<usize>,
    compression: VectorIndexCompression,
    vector_space: VectorSpace,
    background: Arc<BackgroundTasks>,
) -> Result<SearchDi> {
    let VectorSpace {
        identity,
        backfill_model,
    } = vector_space;
    // Every vector space has its own index files. A model switch cannot wipe its
    // predecessor — and neither can a compression switch, which reshapes the
    // stored vectors just as completely as a model change does. The compression
    // layout therefore joins the model identity in the generation key, so
    // toggling it rebuilds into its own files instead of mixing two vector
    // spaces in one index. `None` contributes no suffix, so existing vaults keep
    // their current filenames byte for byte.
    let generation = super::engine::vector_search::runtime_index::generation_name(
        identity.as_deref(),
        &compression,
    );
    // A backup restore leaves this marker because archives carry no vectors.
    let reembed_marker = usearch_index_path
        .parent()
        .map(|dir| dir.join(lattice_core::shared::constants::REEMBED_MARKER_FILE));
    let usearch_index_path =
        usearch_index_path.with_file_name(format!("usearch-{}.usearch", generation));
    // Models vary in output dimension. The active model's true dimension
    // comes from its GGUF header, read by the composition root before this
    // point. If the active model is unset (or its header can't be read), fall
    // back to the persisted index dimension — that way we keep an existing
    // index intact until a model load actually proves its dimension.
    let dimension = active_embedding_dimension
        .or_else(|| crate::infrastructure::vector_index::read_dimension(&usearch_index_path))
        .unwrap_or(DEFAULT_EMBEDDING_DIM);

    // Compare the persisted vector layout against the requested one. If the
    // dimension or the compression configuration differs, wipe the index
    // (USearch can't reshape at runtime) so the new configuration can start
    // from an empty index. The wipe is logged inside the helper. Loading the
    // index reads it from disk, so both steps run off the runtime thread.
    let open_path = usearch_index_path.clone();
    let open_compression = compression.clone();
    let usearch_index = tokio::task::spawn_blocking(move || {
        let _check = crate::infrastructure::vector_index::ensure_index_layout_match(
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
        let restore = |cursor: String| {
            let pool = db_pool.clone();
            let identity = identity.clone();
            async move {
                crate::infrastructure::persistence::repositories::embedding::generation::restore_page(
                    &pool, &identity, dimension, &cursor, 128,
                )
                .await
            }
        };
        open_or_rebuild_streaming(
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
        if usearch_index.count() < total as usize {
            // `identity` is only set for a local model with a recorded
            // artifact identity, so the loader is always present here.
            if let Some(open_model) = backfill_model {
                let model = open_model().await?;
                crate::infrastructure::persistence::repositories::embedding::generation::prepare(
                    &db_pool,
                    model.as_ref(),
                )
                .await?;
                // The backfill moved SQLite, so this pass always rebuilds.
                open_or_rebuild_streaming(
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
        background.spawn(Arc::clone(&persistence).run(background.token()));
        index_persistence = Some(persistence);
    }

    if let Some(marker) = reembed_marker.as_deref().filter(|m| m.exists()) {
        settle_reembed_marker(marker, coverage);
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
            compression,
            index_persistence,
            db_pool.clone(),
            background,
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
        models_path
            .join(lattice_core::shared::constants::RERANKER_MODEL_DIR)
            .join("model.safetensors"),
    )) as Arc<dyn Reranker>;

    let dynamic_embedding = embedding;
    let chunk_repository = Arc::new(
        crate::infrastructure::persistence::repositories::ChunkRepository::new(db_pool.clone()),
    ) as Arc<dyn ChunkRepositoryPort>;
    let semantic_search_use_case = Arc::new(
        SemanticSearchUseCase::new(dynamic_embedding.clone(), vector_search.clone())
            .with_chunk_repository(Arc::clone(&chunk_repository)),
    );
    let hybrid_search_use_case = Arc::new(
        HybridSearchUseCase::new(dynamic_embedding, vector_search.clone(), text_search)
            .with_chunk_repository(chunk_repository)
            // Chat turns it on per turn through its retrieval tuning; direct
            // search and the tool executor never call the stage.
            .with_reranker(reranker),
    );

    Ok(SearchDi {
        runtime_index,
        semantic_search_use_case,
        hybrid_search_use_case,
        search_service,
        bm25_search,
        search_enrichment_service,
        vector_search,
        document_repo,
    })
}

/// Search's registrar surface on `Container`.
impl Container {
    /// The app's search result cache: the search commands read it, the cache
    /// commands report and clear it, and every indexer invalidates it.
    pub fn query_cache(&self) -> &Arc<crate::features::search::query_cache::QueryCache> {
        &self.query_cache
    }

    // Search (from SearchModule)
    pub fn semantic_search_use_case(&self) -> Arc<SemanticSearchUseCase> {
        Arc::clone(self.search.semantic_search_use_case())
    }

    pub fn hybrid_search_use_case(&self) -> Arc<HybridSearchUseCase> {
        Arc::clone(self.search.hybrid_search_use_case())
    }

    /// The reranker setup's toggle. The container itself answers it, reading
    /// the user's `search.enableReranking` setting on demand, so flipping the
    /// switch takes effect without a restart.
    pub fn reranking_toggle(
        &self,
    ) -> Arc<dyn crate::features::search::reranker_setup::RerankingToggle> {
        Arc::new(self.clone())
    }

    /// Get vector search service (for legacy search commands, from SearchModule)
    pub fn search_service(&self) -> Arc<dyn SearchServiceTrait> {
        Arc::clone(self.search.search_service())
    }

    /// Get search enrichment service (for legacy search commands, from SearchModule)
    pub fn search_enrichment_service(&self) -> Arc<dyn SearchEnrichmentServiceTrait> {
        Arc::clone(self.search.search_enrichment_service())
    }

    /// Repository accessor for access tracking (from SearchModule)
    pub fn document_repository(&self) -> Arc<dyn DocumentRepository> {
        Arc::clone(self.search.document_repo())
    }

    /// Library chunks and scoped hybrid search for a feature that keeps its
    /// own copy of library documents.
    pub fn library_passages(
        &self,
    ) -> Arc<dyn lattice_core::application::ports::LibraryPassagesPort> {
        Arc::new(crate::features::search::use_cases::LibraryPassages::new(
            self.hybrid_search_use_case(),
            self.document_repository(),
            self.chunk_repository(),
        ))
    }
}

/// Clear the post-restore marker once every chunk has a vector again, or say
/// plainly what is still missing. Clusters have no startup backfill; they
/// come back as the corpus view is recomputed.
fn settle_reembed_marker(marker: &std::path::Path, coverage: Option<(usize, usize)>) {
    match coverage {
        Some((present, total)) if present >= total => match std::fs::remove_file(marker) {
            Ok(()) => tracing::info!(
                vectors = present,
                "Restored library is fully embedded again; cleared the re-embed marker"
            ),
            Err(e) => tracing::warn!(error = %e, "Could not clear the re-embed marker"),
        },
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
            .join(lattice_core::shared::constants::REEMBED_MARKER_FILE);
        std::fs::write(&path, b"2026-09-16T00:00:00Z").unwrap();
        (dir, path)
    }

    #[test]
    fn complete_coverage_clears_the_marker() {
        let (_dir, path) = marker();
        settle_reembed_marker(&path, Some((10, 10)));
        assert!(!path.exists());
    }

    #[test]
    fn an_empty_library_counts_as_complete() {
        let (_dir, path) = marker();
        settle_reembed_marker(&path, Some((0, 0)));
        assert!(!path.exists());
    }

    #[test]
    fn missing_vectors_keep_the_marker() {
        let (_dir, path) = marker();
        settle_reembed_marker(&path, Some((3, 10)));
        assert!(path.exists());
    }

    #[test]
    fn no_active_model_keeps_the_marker() {
        let (_dir, path) = marker();
        settle_reembed_marker(&path, None);
        assert!(path.exists());
    }
}
