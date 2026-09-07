//! Search feature dependency injection.
//!
//! Owns the shared USearch vector index (referenced by IndexingModule
//! too — the composition root passes `vector_search` across).

use std::sync::{Arc, RwLock};

use sqlx::SqlitePool;

use crate::application::ports::{
    DocumentRepository, EmbeddingPort, TextSearchPort, VectorSearchPort,
};
use crate::domain::embedding_constants::DEFAULT_EMBEDDING_DIM;
use crate::features::embedding::service::DynamicEmbedding;
use crate::features::search::use_cases::{HybridSearchUseCase, SemanticSearchUseCase};
use crate::features::search::{BM25SearchTrait, HybridSearchTrait, SearchServiceTrait};
use crate::infrastructure::persistence::repositories::DocumentRepositoryImpl;
use crate::infrastructure::search::bm25::BM25Search;
use crate::infrastructure::search::hybrid::{HybridSearchService, SearchConfig, SearchMode};
use crate::infrastructure::search::text_search::SqliteTextSearch;
use crate::infrastructure::search::vector_search::USearchVectorIndex;
use crate::infrastructure::services::search_enrichment_service::SearchEnrichmentService;
use crate::infrastructure::services::traits::SearchEnrichmentServiceTrait;
use crate::shared::error::{AppError, Result};

#[derive(Clone)]
pub struct SearchDi {
    // Use cases
    pub semantic_search_use_case: Arc<SemanticSearchUseCase>,
    pub hybrid_search_use_case: Arc<HybridSearchUseCase>,

    // Services
    pub search_service: Arc<dyn SearchServiceTrait>,
    pub bm25_search: Arc<dyn BM25SearchTrait>,
    pub hybrid_search_service: Arc<dyn HybridSearchTrait>,
    pub search_enrichment_service: Arc<dyn SearchEnrichmentServiceTrait>,

    // Ports (exported so IndexingModule can write to the same USearch index)
    pub vector_search: Arc<dyn VectorSearchPort>,
    pub document_repo: Arc<dyn DocumentRepository>,
    pub embedding_cache: Arc<RwLock<Option<Arc<dyn EmbeddingPort>>>>,
}

pub async fn build(
    db_pool: SqlitePool,
    usearch_index_path: std::path::PathBuf,
    embedding_cache: Arc<RwLock<Option<Arc<dyn EmbeddingPort>>>>,
    active_embedding_dimension: Option<usize>,
) -> Result<SearchDi> {
    // Candle-era models vary in output dimension (384 / 768 / 1024). The
    // active model's true dimension comes from its config.json::hidden_size,
    // resolved by the composition root before this point. If the active
    // model is unset (or its config can't be read), fall back to the
    // persisted index dimension — that way we keep an existing index intact
    // until a model load actually proves its dimension.
    let dimension = active_embedding_dimension
        .or_else(|| {
            crate::infrastructure::search::vector_search::read_dimension(&usearch_index_path)
        })
        .unwrap_or(DEFAULT_EMBEDDING_DIM);

    // Compare persisted dimension against the requested one. If they differ,
    // wipe the index (USearch can't resize at runtime) so the new model can
    // start from an empty index. `DimensionCheck::Wiped` is logged inside
    // the helper.
    let _check = crate::infrastructure::search::vector_search::ensure_dimension_match(
        &usearch_index_path,
        dimension,
    )?;

    // USearch: single index that implements both VectorSearchPort and SearchServiceTrait.
    let usearch_index = Arc::new(
        USearchVectorIndex::open_or_create(dimension, usearch_index_path.clone()).map_err(|e| {
            AppError::InternalError(format!("Failed to initialize USearch index: {}", e))
        })?,
    );

    // One-time migration: rebuild from SQLite if USearch file is empty
    // but embeddings exist in the text_embeddings table.
    if usearch_index.count() == 0 {
        let rows = sqlx::query_as::<_, (String, Vec<u8>, String, String, String)>(
            "SELECT te.id, te.embedding, COALESCE(tc.content, ''), tc.id, COALESCE(tc.document_id, '') \
             FROM text_embeddings te \
             LEFT JOIN text_chunks tc ON te.chunk_id = tc.id",
        )
        .fetch_all(&db_pool)
        .await?;

        if !rows.is_empty() {
            let total_rows = rows.len();
            let enriched: Vec<(String, Vec<f32>, String, String, String)> = rows
                .into_iter()
                .map(|(_emb_id, bytes, content, chunk_id, doc_id)| {
                    let floats = crate::features::embedding::encoding::decode_embedding(&bytes)
                        .unwrap_or_default();
                    // Key off the chunk, not the stored `text_embeddings.id`.
                    // Engine-written rows historically used a bare UUID there,
                    // so an index built from that column could never be
                    // matched by deletion, which looks up `emb_{chunk_id}`.
                    let key = crate::features::embedding::encoding::vector_key(&chunk_id);
                    (key, floats, content, chunk_id, doc_id)
                })
                // Must compare against the *active* dimension, not the
                // default. Hard-coding 384 here meant that under any 768- or
                // 1024-dim model every row failed the filter and the rebuild
                // produced a silently empty index.
                .filter(|(_, v, _, _, _)| v.len() == dimension)
                .collect();

            let dropped = total_rows - enriched.len();
            if dropped > 0 {
                // A silent filter is what let the dimension bug hide. If rows
                // are being discarded, say so and say why.
                tracing::warn!(
                    dropped,
                    total = total_rows,
                    expected_dimension = dimension,
                    "Discarded embeddings during USearch rebuild: wrong dimension or undecodable. \
                     They will be absent from vector search until re-indexed."
                );
            }

            if !enriched.is_empty() {
                tracing::info!(
                    count = enriched.len(),
                    "Rebuilding USearch index from SQLite embeddings (one-time migration)"
                );
                match usearch_index.rebuild_from_embeddings(enriched) {
                    Ok(added) => tracing::info!(added, "USearch index rebuilt successfully"),
                    Err(e) => tracing::error!(error = %e, "Failed to rebuild USearch index"),
                }
            }
        }
    }

    tracing::info!(
        size = usearch_index.count(),
        path = %usearch_index_path.display(),
        "USearch vector index ready"
    );

    let vector_search = usearch_index.clone() as Arc<dyn VectorSearchPort>;
    let search_service = usearch_index.clone() as Arc<dyn SearchServiceTrait>;

    let text_search = Arc::new(SqliteTextSearch::new(db_pool.clone())) as Arc<dyn TextSearchPort>;
    let document_repo =
        Arc::new(DocumentRepositoryImpl::new(db_pool.clone())) as Arc<dyn DocumentRepository>;

    let bm25_search = Arc::new(BM25Search::new(db_pool.clone())) as Arc<dyn BM25SearchTrait>;
    let search_enrichment_service = Arc::new(SearchEnrichmentService::new(db_pool.clone()))
        as Arc<dyn SearchEnrichmentServiceTrait>;

    let hybrid_config = SearchConfig {
        mode: SearchMode::Hybrid,
        vector_weight: 0.7,
        keyword_weight: 0.3,
        min_score: crate::shared::constants::MIN_SIMILARITY_SCORE,
        enable_reranking: true,
        recency_boost: 1.0,
        max_results: 100,
    };
    let hybrid_search_service = Arc::new(HybridSearchService::new(
        search_service.clone(),
        bm25_search.clone(),
        db_pool,
        search_enrichment_service.clone(),
        hybrid_config,
    )) as Arc<dyn HybridSearchTrait>;

    let dynamic_embedding =
        Arc::new(DynamicEmbedding::new(embedding_cache.clone())) as Arc<dyn EmbeddingPort>;

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
        semantic_search_use_case,
        hybrid_search_use_case,
        search_service,
        bm25_search,
        hybrid_search_service,
        search_enrichment_service,
        vector_search,
        document_repo,
        embedding_cache,
    })
}
