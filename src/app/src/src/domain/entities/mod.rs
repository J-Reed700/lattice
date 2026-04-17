//! # Domain Entities
//!
//! Core business entities representing domain concepts.
//!
//! Entities have identity and lifecycle, distinct from value objects which are
//! defined by their attributes.

pub mod chunk;
pub mod document;
pub mod embedding;
// Vertical-slice migration (mentions): entity lives in features/mentions/entity.rs.
#[path = "../../features/mentions/entity.rs"]
pub mod mention;
pub mod model;
pub mod model_file;
pub mod search_result;
// Vertical-slice migration (tags): entity lives in features/tags/entity.rs.
#[path = "../../features/tags/entity.rs"]
pub mod tag;

// Re-export public types
pub use chunk::Chunk;
pub use document::{Document, DocumentStatus};
pub use embedding::Embedding;
pub use mention::{Mention, MentionType};
pub use model::Model;
pub use model_file::ModelFile;
pub use search_result::SearchResult;
pub use tag::Tag;
