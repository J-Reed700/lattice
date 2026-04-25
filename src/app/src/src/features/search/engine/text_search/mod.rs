//! Text search implementations.
//!
//! Currently only the SQLite FTS5 backed `SqliteTextSearch`. Earlier
//! placeholder modules (`bm25.rs`, `file_search.rs`) for a never-completed
//! migration were removed.

pub mod sqlite_text_search;

pub use sqlite_text_search::SqliteTextSearch;
