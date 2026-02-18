//! Text Search Implementations
//!
//! This module contains text-based search algorithms.
//!
//! # Migration Status
//! - [ ] bm25.rs - Will move from `search/bm25.rs`
//! - [ ] file_search.rs - Will move from `search/file_search.rs`
//!
//! # Algorithms
//! - BM25 (Best Matching 25) for keyword relevance scoring
//! - Full-text search with tokenization

pub mod bm25;
pub mod file_search;
pub mod sqlite_text_search;

pub use sqlite_text_search::SqliteTextSearch;
