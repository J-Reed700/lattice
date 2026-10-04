//! Identifiers and validated primitives shared across feature boundaries.

mod error;
mod file_path;
mod ids;
mod tag_name;

pub use error::DomainTypeError;
pub use file_path::ValidatedFilePath;
pub use ids::{ChunkId, ConversationId, DocumentId, MentionId, TagId};
pub use tag_name::TagName;

#[cfg(test)]
mod tests;
