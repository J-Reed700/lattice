//! Embedding Repository Module
//!
//! Implements the Ephemeral Tx Wrapper Pattern for embedding persistence.
//!
//! # Architecture
//!
//! - `ops.rs` - Generic SQL operations that accept any SQLite executor
//! - `implementation.rs` - Pool-based repository for standalone operations
//! - `tx.rs` - Transaction-based repository for transactional operations
//!
//! Both implementations delegate to the same SQL operations in ops.rs,
//! ensuring consistency while enabling flexible transaction management.

pub mod implementation;
pub mod ops;
pub mod tx;

pub use implementation::SqliteEmbeddingRepository;
pub use tx::SqliteEmbeddingRepositoryTx;
