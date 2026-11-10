//! Model SQL operations, run inside the caller's unit of work.
//!
//! - `ops.rs` - SQL operations that accept any SQLite connection

pub mod ops;
#[cfg(test)]
mod ops_tests;
