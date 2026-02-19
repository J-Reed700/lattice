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
//! - **Brute Force**: Linear search for small datasets or exact results
//!
//! ### Keyword Search
//! - **BM25**: Best Match 25 ranking function using SQLite FTS5
//! - **File Search**: Fast filename and path matching
//!
//! ### Hybrid Search
//! - **Fusion**: Combines vector and keyword results using Reciprocal Rank Fusion
//! - **Recency Scoring**: Boosts recent documents in search results
//! - **Query Expansion**: Expands queries with synonyms and related terms
//!
//! ## Performance Characteristics
//!
//! | Search Type | Time Complexity | Memory | Accuracy |
//! |-------------|----------------|---------|----------|
//! | HNSW        | O(log N)       | High    | ~95%     |
//! | Brute Force | O(N)           | Medium  | 100%     |
//! | BM25        | O(k)           | Low     | Good     |
//! | Hybrid      | O(log N + k)   | High    | Excellent|
//!
//! Where:
//! - N = total number of embeddings
//! - k = number of matching documents for keyword search
//!
//! ## Usage Examples
//!
//! ### Basic Semantic Search
//!
//! ```rust,no_run
//! use vault_desktop::search::USearchVectorIndex;
//!
//! let index = USearchVectorIndex::new(768, None)?;
//!
//! let query_embedding = vec![0.1, 0.2, 0.3]; // From embedding service
//! let results = service.search(&query_embedding, 10).await?;
//! ```
//!
//! ### Hybrid Search (Recommended)
//!
//! ```rust,no_run
//! use vault_desktop::search::{
//!     HybridSearchService, SearchMode, SearchConfig
//! };
//!
//! let config = SearchConfig {
//!     mode: SearchMode::Hybrid,
//!     vector_weight: 0.7,
//!     keyword_weight: 0.3,
//!     min_score: 0.5,
//!     ..Default::default()
//! };
//!
//! let service = HybridSearchService::new(pool, embedder, config);
//! let results = service.search("machine learning", 20).await?;
//! ```
//!
//! ### With Query Expansion
//!
//! ```rust,no_run
//! use vault_desktop::search::{QueryExpander, QueryExpansionConfig};
//!
//! let expander = QueryExpander::new(QueryExpansionConfig::default());
//! let expanded = expander.expand("ML").await?;
//! // Returns: ["ML", "machine learning", "artificial intelligence"]
//!
//! let results = service.search_with_expansion(&expanded, 20).await?;
//! ```
//!
//! ## Configuration
//!
//! Search behavior can be tuned via `SearchConfig`:
//!
//! ```rust
//! use vault_desktop::search::{SearchConfig, SearchMode};
//!
//! let config = SearchConfig {
//!     mode: SearchMode::Hybrid,
//!     vector_weight: 0.7,        // 70% semantic relevance
//!     keyword_weight: 0.3,       // 30% keyword relevance
//!     min_score: 0.5,            // Minimum similarity threshold
//!     enable_reranking: true,    // Post-process results
//!     recency_boost: 0.1,        // Boost recent documents 10%
//!     max_results: 100,          // Fetch top 100 before filtering
//! };
//! ```
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
//! use vault_desktop::search::Profiler;
//!
//! let profiler = Profiler::new();
//! profiler.start("search");
//! // ... perform search ...
//! let metrics = profiler.finish("search");
//! println!("Search took: {:?}", metrics.duration);
//! ```

pub mod bm25;
pub mod builder;
pub mod file_search;
pub mod fusion;
pub mod hybrid;
pub mod index;
pub mod profiler;
pub mod recency;
pub mod reranker;
pub mod service;
pub mod strategies;
pub mod text_search;
pub mod vector_ops;
pub mod vector_search;

pub use bm25::{BM25Result, BM25Search};
pub use builder::{
    HybridSearchBuilder, Ready as SearchReady, Uninitialized as SearchUninitialized,
};
pub use fusion::{FusionResult, ReciprocalRankFusion, WeightedFusion};
pub use hybrid::{HybridSearchResult, HybridSearchService, SearchConfig, SearchMode};
pub use index::EmbeddingIndex;
pub use profiler::{PerformanceMetrics, Profiler};
pub use reranker::{RerankResult, RerankerService};
pub use service::{BruteForceSearch, SearchResult};
pub use vector_ops::{cosine_similarity_naive, cosine_similarity_simd, normalize_vector};
pub use vector_search::USearchVectorIndex;
pub mod query_expansion;
pub mod snippet;
pub use file_search::{FileSearch, FileSearchResult};
pub use query_expansion::{QueryExpander, QueryExpansion, QueryExpansionConfig};
pub use recency::{RecencyConfig, RecencyScorer};

#[cfg(test)]
mod vector_ops_test;

#[cfg(test)]
mod index_tests;
