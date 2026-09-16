//! # Compare feature
//!
//! Builds a comparison table across a handful of documents: one row per
//! document, one column per plain-language field the user named, and a
//! citation under every answer that has one.
//!
//! ## Cost shape
//!
//! Retrieval is per (document, column) — cheap, no LLM. Generation is **one
//! call per document**, covering every column at once, so a 12 × 6 table costs
//! 12 generations rather than 72. Documents run two at a time.
//!
//! ## Public surface
//!
//! - `dto` — request and table DTOs
//! - `use_case::compare_documents_impl` — the whole pipeline
//! - `plugin::init()` — the `compare` Tauri plugin
//!
//! Retrieval goes through the semantic search use case and chunk repository;
//! this feature does not inspect the filesystem directly.

pub mod commands;
pub mod dto;
pub mod parser;
pub mod plugin;
pub mod prompt;
pub mod retrieval;
pub mod use_case;

#[cfg(test)]
mod tests;
