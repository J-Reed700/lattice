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
//! - `dto` — the request and table DTOs (GROUND-RULES §4.10)
//! - `use_case::compare_documents_impl` — the whole pipeline
//! - `plugin::init()` — the `compare` Tauri plugin
//!
//! Nothing here touches the filesystem: retrieval goes through the semantic
//! search use case and the chunk repository (CLAUDE.md Repository Barrier).

pub mod commands;
pub mod dto;
pub mod parser;
pub mod plugin;
pub mod prompt;
pub mod retrieval;
pub mod use_case;

#[cfg(test)]
mod tests;
