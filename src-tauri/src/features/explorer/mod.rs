//! # Explorer feature
//!
//! A folder from disk beside a chat. The viewer lists and opens files
//! through the plugin, and an Explorer conversation's model gets read-only
//! tools confined to the same folder. The folder is read live; the index in
//! `index` only adds search by meaning on top, kept outside the folder.
//!
//! ## Public surface
//!
//! - `dto` — wire types shared with the frontend
//! - `scope::Scope` — the folder boundary every path is resolved through
//! - `fs` — listing, reading, search and find, bounded and `.gitignore`-aware
//! - `tools` — the model's `list_directory` / `read_file` / `search_files` /
//!   `find_files`
//! - `prompt` — the folder block an Explorer turn's prompt carries
//! - `line_refs` — the `path:10-24` grammar answers use to point at lines
//! - `repository` — `conversations.explorer_root`
//! - `index` — the open folder's semantic index and `search_folder`
//! - `plugin::init()` — the `explorer` Tauri plugin

pub mod dto;
pub mod fs;
pub mod index;
pub mod line_refs;
pub mod plugin;
pub mod prompt;
pub mod repository;
pub mod scope;
pub mod tools;

#[cfg(test)]
mod tests;
