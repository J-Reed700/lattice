//! # Domain Entities
//!
//! Core business entities representing domain concepts.
//!
//! Entities have identity and lifecycle, distinct from value objects which are
//! defined by their attributes.

pub mod chunk;
pub mod document;
// Vertical-slice migration (embedding): entity lives in features/embedding/entity.rs.
#[path = "../../features/embedding/entity.rs"]
pub mod embedding;
pub mod model;
pub mod model_file;
// Vertical-slice migration (search): entity lives in features/search/entity.rs.
#[path = "../../features/search/entity.rs"]
pub mod search_result;

// Re-export public types
pub use chunk::Chunk;
pub use document::{Document, DocumentStatus};
pub use embedding::Embedding;
pub use model::Model;
pub use model_file::ModelFile;
pub use search_result::SearchResult;
