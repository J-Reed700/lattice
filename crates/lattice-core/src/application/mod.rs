//! Application Layer
//!
//! Orchestrates use cases and coordinates domain operations.

pub mod contracts;
pub mod error;
pub mod factories;
pub mod mappers;
pub mod ports;
pub mod services;

pub use error::ApplicationError;
pub use factories::{ChecksumFactory, FileMetadataFactory};
pub use services::FileType;

pub use ports::{
    EmbeddingPort, FavoritesRepositoryPort, FileStoragePort, LLMPort,
    RecentDocumentsRepositoryPort, RepositoryPort, TextSearchPort, VectorSearchPort,
};
