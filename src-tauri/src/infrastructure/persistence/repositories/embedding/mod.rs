//! Embedding repository: the transaction-scoped SQLite adapter for
//! `EmbeddingRepositoryPort`, handed out by the unit of work.
//!
//! - `generation.rs` - Resumable preparation and restore of a vector space
//! - `mapper.rs` - Maps the domain `Embedding` to and from its row
//! - `ops.rs` - SQL operations that accept any SQLite executor
//! - `tx.rs` - Transaction-based repository

pub mod generation;
pub mod mapper;
pub mod ops;
pub mod tx;

pub use tx::SqliteEmbeddingRepositoryTx;
