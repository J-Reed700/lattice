//! # Activity feature
//!
//! Everything the app is doing in the background, in one list: jobs of
//! every kind, each model's queue of requests, models loading, downloads,
//! the library's indexer and the index turn order. Each part describes its
//! own live items through the core `ActivitySource` port; the composition
//! root hands this feature every source, and it only puts their items
//! together (`assemble`). It reads; it never starts, stops or reorders work.
//!
//! ## Public surface
//!
//! - `dto` — the snapshot the renderer reads, and its change event
//! - `assemble::assemble` — the items, paused and queued in words
//! - `plugin::init()` — the Tauri plugin and its `activity_snapshot` command

pub mod assemble;
pub mod dto;
pub mod plugin;

#[cfg(test)]
mod tests;
