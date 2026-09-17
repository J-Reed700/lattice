//! # Domain Value Objects
//!
//! Immutable, validated domain primitives following DDD value object patterns.
//!
//! Value objects:
//! - Are immutable after creation
//! - Validate data on construction
//! - Implement equality by value
//! - Have no identity (defined by their attributes)

pub mod artifact_identity;
pub mod checksum;
pub mod chunking_strategy;
pub mod file_metadata;
pub mod indexing_outcome;
pub mod model_status;
pub mod search_mode;
pub mod search_query;
pub mod section_identifier;
pub mod source_context;
pub mod sparse_embedding;

pub use artifact_identity::{ArtifactIdentity, ArtifactIdentityError};
pub use checksum::Checksum;
pub use chunking_strategy::ChunkingStrategy;
pub use file_metadata::FileMetadata;
pub use indexing_outcome::IndexingOutcome;
pub use model_status::{FileStatus, ModelStatus};
pub use search_mode::SearchMode;
pub use search_query::SearchQuery;
pub use sparse_embedding::SparseEmbedding;
