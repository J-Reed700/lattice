//! # Application Layer
//!
//! The application layer orchestrates use cases and coordinates domain operations.
//!
//! ## Responsibilities
//!
//! - **Use Case Orchestration**: Coordinate domain operations for specific workflows
//! - **DTO Management**: Define data structures for crossing boundaries
//! - **Mapping**: Convert between domain models and DTOs
//! - **Transaction Boundaries**: Define transactional operations
//!
//! ## Architecture
//!
//! ```text
//! Commands (Tauri IPC)
//!     ↓
//! Application Layer (DTOs + Mappers)
//!     ↓
//! Domain Layer (Aggregates + Services)
//!     ↓
//! Infrastructure Layer (Repositories + External Services)
//! ```
//!
//! ## Organization
//!
//! - `mappers/` - Domain ↔ DTO conversion logic
//! - `ports/` - Interface definitions for infrastructure (Hexagonal Architecture)
//! - `services/` - Application services (FileType, ContextWindowBuilder, …)

pub mod error;
pub mod factories;
pub mod mappers;
pub mod ports;
pub mod services;

// Re-export commonly used items
pub use error::ApplicationError;
pub use factories::{ChecksumFactory, FileMetadataFactory};
pub use mappers::*;
pub use services::FileType;

// Re-export port traits for convenience
pub use ports::{
    EmbeddingPort, FavoritesRepositoryPort, FileStoragePort, LLMPort, NotificationPort,
    RecentDocumentsRepositoryPort, RepositoryPort, TextSearchPort, VectorSearchPort,
};
