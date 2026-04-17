//! # Extraction feature
//!
//! Wikilink parsing and document-title extraction. Despite the name,
//! this feature is NOT about file-content extraction (PDF/DOCX/etc.).
//! Those concerns live in the shared `infrastructure/extraction/` and
//! `infrastructure/indexing/extraction/` modules used by the indexing
//! pipeline.
//!
//! ## File layout
//!
//! | File          | Canonical module path                                |
//! |---------------|------------------------------------------------------|
//! | `dto.rs`      | `crate::application::dtos::extraction_dto`           |
//! | `use_cases/`  | `crate::application::use_cases::extraction`          |
//! | `commands.rs` | `crate::interfaces::commands::extraction`            |
//! | `plugin.rs`   | `crate::plugins::extraction`                         |
//!
//! Shared content-extraction infrastructure deliberately stays in
//! `infrastructure/extraction/` (link_parser, pdf/docx/html extractors)
//! and `infrastructure/indexing/extraction/` because they are consumed
//! by indexing and mentions, not only by this feature.
//!
//! `ContentExtractionPort` stays in `application/ports/`.
