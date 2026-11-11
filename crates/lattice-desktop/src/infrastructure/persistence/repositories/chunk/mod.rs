//! Chunk Repository Module
//!
//! Chunk persistence.
//!
//! # Architecture
//!
//! - `ops.rs` - SQL operations that accept any SQLite connection, so a caller
//!   can run them inside its own unit of work
//! - `repository.rs` - Pool-based repository for standalone operations

pub mod ops;
pub mod repository;
#[cfg(test)]
mod repository_tests;

pub use repository::ChunkRepository;
