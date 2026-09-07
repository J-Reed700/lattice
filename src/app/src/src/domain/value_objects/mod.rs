//! # Domain Value Objects
//!
//! Immutable, validated domain primitives following DDD value object patterns.
//!
//! Value objects:
//! - Are immutable after creation
//! - Validate data on construction
//! - Implement equality by value
//! - Have no identity (defined by their attributes)

pub mod checksum;
pub mod chunking_strategy;
pub mod file_metadata;
pub mod indexing_outcome;
pub mod model_status;
// Vertical-slice migration (search): value objects live in features/search/value_objects/.
#[path = "../../features/search/value_objects/mode.rs"]
pub mod search_mode;
#[path = "../../features/search/value_objects/query.rs"]
pub mod search_query;

// Re-export public types
pub use checksum::Checksum;
pub use chunking_strategy::ChunkingStrategy;
pub use file_metadata::FileMetadata;
pub use indexing_outcome::IndexingOutcome;
pub use model_status::{FileStatus, ModelStatus};
pub use search_mode::SearchMode;
pub use search_query::SearchQuery;
