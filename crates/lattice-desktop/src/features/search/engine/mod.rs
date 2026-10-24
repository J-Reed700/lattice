//! Search engine components including vector search, BM25, and reranking.

// Engine sub-modules (flattened from former `modules/` subdirectory).
pub mod bm25;
pub mod fts_query;
pub mod profiler;
pub mod qwen3_reranker;
pub mod recency;
pub mod reranker;
pub mod snippet;

// Directory-backed sub-modules.
pub mod text_search;
pub mod vector_search;

pub use bm25::BM25Search;
pub use profiler::{PerformanceMetrics, Profiler};
pub use qwen3_reranker::Qwen3RerankerService;
pub use recency::{RecencyConfig, RecencyScorer};
pub use reranker::{
    blend_rerank_scores, load_reranker, LazyReranker, MiniLmRerankerService, RerankResult,
    Reranker, RerankerService,
};
