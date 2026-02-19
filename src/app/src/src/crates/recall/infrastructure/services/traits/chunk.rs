//! Chunk Repository Trait - REMOVED (migrated to DDD)
//!
//! The old ChunkRepositoryTrait has been removed as part of the DDD migration.
//!
//! # Migration Path
//!
//! **Old (REMOVED)**:
//! ```rust,ignore
//! use crate::infrastructure::services::traits::ChunkRepositoryTrait;
//! use crate::repositories::chunk_repository::Chunk; // ← Type no longer exists
//!
//! async fn process(repo: &dyn ChunkRepositoryTrait) {
//!     let chunk = repo.create(doc_id, content, None, None, 0, None, None).await?;
//! }
//! ```
//!
//! **New (DDD Ports)**:
//! ```rust,ignore
//! use crate::application::ports::{RepositoryPort, ChunkRepositoryPort};
//! use crate::domain::entities::chunk::Chunk;
//!
//! async fn process<R>(repo: &R)
//! where
//!     R: RepositoryPort<Chunk> + ChunkRepositoryPort + ?Sized
//! {
//!     let chunk = Chunk::new(doc_id.clone(), content.to_string(), 0);
//!     repo.save(&chunk).await?;
//! }
//! ```
//!
//! # Implementation
//!
//! - **Domain Entity**: `crate::domain::entities::chunk::Chunk`
//! - **Repository**: `ChunkRepository` implements `RepositoryPort<Chunk>` + `ChunkRepositoryPort`
//! - **Ports**: See `crate::application::ports`
//!
//! See `infrastructure/persistence/repositories/chunk_repository.rs` for the new implementation.
