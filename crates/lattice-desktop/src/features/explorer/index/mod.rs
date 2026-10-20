//! # Folder index
//!
//! A semantic index of the folder open in Explorer, so "where do we handle
//! retries?" finds the code even when it never says "retry". The grep tools
//! find what the model can guess; this finds what it cannot.
//!
//! Each indexed folder has its own directory under `<data_dir>/folder-index/`
//! holding a passage index (`crate::infrastructure::passage_index`):
//! `passages.db` and a vectors file keyed by the embedding identity. The
//! store, the line chunker, the embedding loop and the live directories are
//! shared there; what a folder is (its walk, ignore rules, file bound,
//! sub-folder reuse, watcher and status) is here. The index itself is not
//! written to `lattice.db`; its builds are jobs
//! there, which the renderer follows on `jobs://status`. A build starts when
//! a folder is picked, resumes incrementally when it is opened again or the
//! app restarts, and the open folder follows edits through a watcher.
//!
//! ## Public surface
//!
//! - `dto` — `FolderIndexStatusDto` and its states; the folders list's
//!   `FolderIndexSummaryDto`
//! - `store` — the folder's keys over the passage store: its directory, its
//!   meta (root, last opened, too large, complete) and the registry read
//! - `run` — the incremental indexing run and per-path updates, with
//!   passage progress and a time left
//! - `search::FolderSearch` — the one hybrid search over the folder's index
//! - `build` — a build as a job: its request, progress and activity
//! - `manager` — open, close, rebuild, delete; refused roots, size cap,
//!   own-index preference and sub-folder reuse, identity switch, summaries
//! - `tool` — the model's `search_folder`

mod build;
pub mod dto;
pub mod manager;
pub mod run;
pub mod search;
pub mod store;
pub mod tool;
mod watcher;

#[cfg(test)]
mod tests;

pub use build::FOLDER_INDEX;
