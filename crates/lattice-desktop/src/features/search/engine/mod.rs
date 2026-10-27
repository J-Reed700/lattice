//! # Search Module
//!
//! Provides comprehensive search capabilities including semantic vector search,
//! keyword-based BM25 search, and hybrid search combining both approaches.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────┐
//! │  Search API      │  ← Tauri Commands (commands/search.rs)
//! └────────┬─────────┘
//!          │
//!          ├─────────────────┐
//!          │                 │
//!   ┌──────▼────────┐ ┌─────▼──────┐
//!   │ Vector Search │ │ BM25 Search │
//!   │  (Semantic)   │ │ (Keyword)   │
//!   └──────┬────────┘ └─────┬──────┘
//!          │                 │
//!          └────────┬────────┘
//!                   │
//!            ┌──────▼─────────┐
//!            │ Fusion Service │  ← Reciprocal Rank Fusion
//!            └──────┬─────────┘
//!                   │
//!            ┌──────▼─────────┐
//!            │   Reranker     │  ← Optional reranking
//!            └────────────────┘
//! ```
//!
//! ## Components
//!
//! ### Vector Search
//! - **USearch HNSW**: Production-grade HNSW via USearch v2.x with persistence and SIMD
//!
//! ### Keyword Search
//! - **BM25**: Best Match 25 ranking over two SQLite FTS5 indexes: `porter
//!   unicode61` for space-separated scripts, `trigram` for CJK
//! - **File Search**: Fast filename and path matching
//!
//! ### Hybrid Search
//! - **Fusion**: Combines vector and keyword results using weighted Reciprocal Rank Fusion
//! - **Recency Scoring**: Boosts recent documents in search results
//!
//! ## Performance Tips
//!
//! 1. **Use HNSW for >10k documents**: Much faster than brute force
//! 2. **Enable caching**: Query cache reduces latency by 80% for repeated queries
//! 3. **Tune fusion weights**: Adjust based on your use case (semantic vs exact match)
//! 4. **Batch queries**: Use `search_batch()` for multiple queries
//! 5. **Profile with metrics**: Use `Profiler` to identify bottlenecks
//!
//! ## Metrics & Monitoring
//!
//! The search module emits metrics for:
//! - Search latency (p50, p95, p99)
//! - Cache hit rate
//! - Index size and memory usage
//! - Query throughput
//!
//! Access via `PerformanceMetrics`:
//!
//! ```rust,no_run
//! use lattice::features::search::engine::Profiler;
//!
//! let profiler = Profiler::new();
//! profiler.start("search");
//! // ... perform search ...
//! let metrics = profiler.finish("search");
//! println!("Search took: {:?}", metrics.duration);
//! ```

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
