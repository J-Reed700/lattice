//! # Extraction feature
//!
//! Wikilink parsing and document-title extraction. Despite the name,
//! this feature is NOT about file-content extraction (PDF/DOCX/etc.).
//! Those concerns live in the shared `infrastructure/extraction/` and
//! `features/indexing/engine/extraction/` modules used by the indexing
//! pipeline. Self-contained vertical slice.
//!
//! ## Public surface
//!
//! - `crate::features::extraction::dto` — extraction DTOs
//! - `crate::features::extraction::use_cases` — wikilink / title use cases
//! - `crate::features::extraction::commands` — Tauri command handlers
//! - `crate::features::extraction::plugin::init()` — Tauri plugin
//!
//! `ContentExtractionPort` stays in `application/ports/`.

pub mod commands;
pub mod dto;
pub mod plugin;
pub mod use_cases;
