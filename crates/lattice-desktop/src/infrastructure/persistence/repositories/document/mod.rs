//! Document Repository Module
//!
//! Document persistence.
//!
//! # Architecture
//!
//! - `ops.rs` - SQL operations that accept any SQLite connection, so a caller
//!   can run them inside its own unit of work
//! - `implementation.rs` - Pool-based repository for standalone operations

pub mod implementation;
pub mod ops;

pub use implementation::SqliteDocumentRepository;
pub use lattice_core::application::ports::DocumentFilter;

#[cfg(test)]
mod tests;
