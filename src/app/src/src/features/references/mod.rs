//! # References feature
//!
//! Passage references: a saved excerpt from a document, with the locator needed
//! to reopen the source where it was found. Written by the reading-selection
//! toolbar and surfaced in the Reference inbox alongside message bookmarks.
//!
//! ## Public surface
//!
//! - `dto` — `PassageReferenceDto` and the request DTOs
//! - `repository::PassageReferenceRepository` — SSOT for passage reference state
//! - `commands` — pure `*_impl(container, …)` functions
//! - `plugin::init()` — the `references` Tauri plugin
//!
//! The repository is constructed on demand from `container.db_pool()`, matching
//! `features::daily_notes`. It is the only reader/writer of `passage_references`.

pub mod commands;
pub mod dto;
pub mod plugin;
pub mod repository;

#[cfg(test)]
mod tests;
