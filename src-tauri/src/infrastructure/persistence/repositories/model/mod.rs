//! Model repository: the transaction-scoped SQLite adapter for
//! `ModelRepositoryPort`, handed out by the unit of work.
//!
//! - `ops.rs` - SQL operations that accept any SQLite executor
//! - `tx.rs` - Transaction-based repository

pub mod ops;
pub mod tx;

pub use tx::SqliteModelRepositoryTx;
