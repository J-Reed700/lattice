//! # Search Use Cases
//!
//! - **Semantic Search**: vector-based similarity
//! - **Hybrid Search**: vector + BM25 keyword
//!
//! `file_search` and `recency_search` were removed — file lookup is
//! served by the file feature directly, and recency-aware search is
//! performed inline by the hybrid search service.

pub mod hybrid_search;
pub mod semantic_search;

pub use hybrid_search::HybridSearchUseCase;
pub use semantic_search::SemanticSearchUseCase;
