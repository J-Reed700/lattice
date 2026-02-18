//! Mock Chunk Repository - REMOVED (migrated to DDD)
//!
//! The old MockChunkRepository has been removed as part of the DDD migration.
//!
//! # Migration Path
//!
//! For testing with chunks, use the new DDD architecture:
//!
//! ```rust,ignore
//! use crate::domain::entities::chunk::Chunk;
//! use crate::application::ports::{RepositoryPort, ChunkRepositoryPort};
//!
//! // Create test chunks using domain entity
//! let chunk = Chunk::new(
//!     "doc-123".to_string(),
//!     "Test content".to_string(),
//!     0
//! );
//!
//! // Use in-memory repository for testing
//! // (Implementation in infrastructure/persistence/repositories/chunk_repository.rs)
//! ```
//!
//! See `infrastructure/persistence/repositories/chunk_repository.rs` for the new DDD implementation.
