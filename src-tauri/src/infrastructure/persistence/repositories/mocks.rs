//! Mock repository implementations for testing
//!
//! This module provides in-memory mock implementations of all repository traits.
//! These mocks enable fast, isolated unit testing without requiring a database.
//!
//! # Architecture
//!
//! Reusable repository test doubles:
//! - **Studs (Public Interface)**: Implement the same traits as production repositories
//! - **Bricks (Implementation)**: In-memory HashMaps for storage
//! - **Regeneratable**: Can be swapped with real repositories without code changes
//!
//! # Usage
//!
//! ```rust
//! use crate::infrastructure::persistence::repositories::mocks::MockDocumentRepository;
//! use crate::infrastructure::persistence::repositories::traits::DocumentRepositoryTrait;
//!
//! #[tokio::test]
//! async fn test_with_mock() {
//!     let repo = MockDocumentRepository::new();
//!     let doc = repo.create("/test.txt", "test.txt", "text/plain", 100, "2024-01-01", "abc123").await.unwrap();
//!     assert_eq!(doc.file_name, "test.txt");
//! }
//! ```

use crate::features::embedding::entity::Embedding as DomainEmbedding;
use crate::features::embedding::persistence_mapper::{EmbeddingDTO, EmbeddingMapper};
use crate::features::embedding::repository::Embedding;
use crate::features::mentions::entity::Mention;
// Tag removed - migrated to DDD (MockTagRepository now in shared/traits.rs)
// Chunk removed - migrated to DDD (use crate::domain::entities::chunk::Chunk)
// Document removed - migrated to DDD (use crate::domain::entities::Document at line ~380)
// Mention types removed - migrated to DDD (use crate::application::ports::mention_repository_port)
// use crate::infrastructure::persistence::repositories::mention_repository::{DocumentMention, Mention, MentionWithContext};

mod chunk;
mod document;
mod embedding;
mod mention;

pub use chunk::MockChunkRepository;
pub use document::MockDocumentRepository;
pub use embedding::MockEmbeddingRepository;
pub use mention::MockMentionRepository;

// MockTagRepository REMOVED - migrated to DDD
// The old MockTagRepository using crate::models::tag::Tag has been removed.
// Use the DDD version instead:
// - Location: src/shared/traits.rs
// - Uses: crate::domain::entities::tag::Tag (domain entity)
// - Trait: crate::shared::traits::TagRepositoryTrait
// Migration note: This mock was obsolete and using the old Tag model.
// The canonical mock is in shared/traits.rs with proper DDD structure.
