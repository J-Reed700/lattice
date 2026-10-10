//! # Jobs feature
//!
//! The renderer's view of the shared job runtime
//! (`crate::shared::runtime::jobs`): the work every feature has in flight, in
//! one list, with `jobs://status` carrying each change after it.
//!
//! ## Public surface
//!
//! - `plugin::init()` — the Tauri plugin and its `list_jobs` command

pub mod plugin;
