//! # Search Use Cases
//!
//! - **Semantic Search**: vector-based similarity
//! - **Hybrid Search**: the library search orchestrator — vector, BM25 and
//!   optional sparse branches, rank fusion and the cross-encoder stage
//!
//! `file_search` and `recency_search` were removed — file lookup is
//! served by the file feature directly, and recency-aware search boosts the
//! hybrid search's results in the search command.

pub mod hybrid_search;
pub mod semantic_search;

pub use hybrid_search::{
    BranchKind, HybridSearchUseCase, QueryBranches, RankedBranch, RerankOptions, Reranked,
};
pub use semantic_search::SemanticSearchUseCase;
