//! Embedding persistence:
//!
//! - `generation.rs` - Resumable preparation and restore of a vector space
//! - `mapper.rs` - Maps the domain `Embedding` to and from its row
//! - `ops.rs` - SQL operations that accept any SQLite connection

pub mod generation;
pub mod mapper;
pub mod ops;
