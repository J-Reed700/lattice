//! # Folder index
//!
//! A semantic index of the folder open in Explorer, so "where do we handle
//! retries?" finds the code even when it never says "retry". The grep tools
//! find what the model can guess; this finds what it cannot.
//!
//! Each indexed folder has its own directory under `<data_dir>/folder-index/`
//! holding `chunks.db` and a vectors file keyed by the embedding identity.
//! Nothing is written to `lattice.db`. Indexing starts when a folder is
//! picked, resumes incrementally when it is opened again, and follows edits
//! through a watcher while it stays open.
//!
//! ## Public surface
//!
//! - `dto` — `FolderIndexStatusDto` and its states; the folders list's
//!   `FolderIndexSummaryDto`
//! - `chunker` — line-based, structure-aware passages
//! - `store` — the per-folder SQLite store and the directory registry
//! - `run` — the incremental indexing run and per-path updates, with
//!   passage progress and a time left
//! - `search::FolderSearch` — dense + `bm25`, fused by reciprocal rank
//! - `manager` — open, close, rebuild, delete; refused roots, size cap,
//!   own-index preference and sub-folder reuse, identity switch, summaries
//! - `tool` — the model's `search_folder`

pub mod chunker;
pub mod dto;
pub mod manager;
pub mod run;
pub mod search;
pub mod store;
pub mod tool;
mod watcher;

#[cfg(test)]
mod tests;

/// The event that carries a `FolderIndexStatusDto` to the frontend.
pub const STATUS_EVENT: &str = "explorer-index://status";
