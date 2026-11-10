//! Chunk Repository Module
//!
//! Chunk persistence.
//!
//! # Architecture
//!
//! - `ops.rs` - SQL operations that accept any SQLite connection, so a caller
//!   can run them inside its own unit of work
//! - `implementation.rs` - Pool-based repository for standalone operations

pub mod implementation;
pub mod ops;
#[cfg(test)]
mod repository_tests;

pub use implementation::{ChunkFilter, SqliteChunkRepository};
