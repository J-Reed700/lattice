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

// Tag removed - migrated to DDD (MockTagRepository now in shared/traits.rs)
// Chunk removed - migrated to DDD (use lattice_core::domain::entities::chunk::Chunk)
// Document removed - migrated to DDD (use lattice_core::domain::entities::Document at line ~380)
// Mention types removed - migrated to DDD (use lattice_core::application::ports::mention_repository_port)
// use crate::infrastructure::persistence::repositories::mention_repository::{DocumentMention, Mention, MentionWithContext};

mod chunk;
mod document;

pub use chunk::MockChunkRepository;
pub use document::MockDocumentRepository;

// MockTagRepository REMOVED - migrated to DDD
// The old MockTagRepository using crate::models::tag::Tag has been removed.
// Use the DDD version instead:
// - Location: src/shared/traits.rs
// - Uses: lattice_core::domain::entities::tag::Tag (domain entity)
// - Trait: lattice_core::shared::traits::TagRepositoryTrait
// Migration note: This mock was obsolete and using the old Tag model.
// The canonical mock is in shared/traits.rs with proper DDD structure.
