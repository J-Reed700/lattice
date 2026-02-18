//! # Search Use Cases
//!
//! Use cases for searching documents and content.
//!
//! This module provides different search strategies:
//! - **Semantic Search**: Vector-based similarity search
//! - **Hybrid Search**: Combines vector and BM25 keyword search
//! - **File Search**: Search by filename and path
//! - **Recency Search**: Time-aware search with recency weighting

pub mod file_search;
pub mod hybrid_search;
pub mod recency_search;
pub mod semantic_search;

// Re-export use cases for convenience
pub use file_search::FileSearchUseCase;
pub use hybrid_search::HybridSearchUseCase;
pub use recency_search::RecencySearchUseCase;
pub use semantic_search::SemanticSearchUseCase;
