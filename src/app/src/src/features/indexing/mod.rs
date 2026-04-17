//! # Indexing feature
//!
//! Document indexing pipeline: content extraction (PDF, DOCX, HTML,
//! …), semantic chunking, metadata extraction, and storage of chunks
//! + embeddings. The actor-based service in `engine/actor.rs` is the
//! primary worker driving the pipeline.
//!
//! ## File layout
//!
//! | Path                                       | Canonical module path                                                       |
//! |--------------------------------------------|------------------------------------------------------------------------------|
//! | `dto.rs`                                   | `crate::application::dtos::indexing_dto`                                     |
//! | `mapper.rs`                                | `crate::application::mappers::indexing_mapper`                               |
//! | `outcome.rs`                               | `crate::domain::value_objects::indexing_outcome`                             |
//! | `commands.rs`                              | `crate::interfaces::commands::indexing_commands`                             |
//! | `trait_def.rs`                             | `crate::infrastructure::services::traits` (merged re-exports)                |
//! | `mocks.rs`                                 | `crate::infrastructure::services::mocks` (merged re-exports)                 |
//! | `use_cases/`                               | `crate::application::use_cases::indexing`                                    |
//! | `engine/`                                  | `crate::infrastructure::indexing` (the indexing pipeline)                    |
//! | `engine/{actor,builder,…,state}.rs`        | `crate::infrastructure::indexing::*` (formerly `infrastructure/indexing/modules/`) |
//! | `engine/transaction.rs`                    | `crate::infrastructure::indexing::transaction` (kept at engine root for SQLx) |
//! | `engine/extraction/`                       | `crate::infrastructure::indexing::extraction` (content extractors per file type) |
//! | `engine/storage/`                          | `crate::infrastructure::indexing::storage`                                   |
//! | `engine/tests/`                            | (orphan integration tests — not registered in `engine/mod.rs`)               |
//!
//! ## Deliberately NOT moved (shared infra)
//!
//! - `application/ports/` — there is no dedicated `IndexingPort`; the
//!   trait lives in `engine/trait_def.rs` (this feature). No port to
//!   relocate.
//! - `infrastructure/persistence/database/performance_indexes.rs` —
//!   shared DB performance config, used across features.
//! - `infrastructure/persistence/helpers/indexed_directories.rs` —
//!   small shared helper used by the DI container.
//! - `infrastructure/extraction/` — UNRELATED to indexing's extraction.
//!   That directory holds wikilink/title/markdown structure parsers
//!   shared by extraction and mentions features. Two namespaces with
//!   the same word: `infrastructure::extraction` (wikilinks) vs
//!   `infrastructure::indexing::extraction` (file content extractors).
//!
//! ## Engine flattening
//!
//! The legacy `infrastructure/indexing/modules/` filesystem-organization
//! wart was dissolved during this migration: 12 files now live as
//! direct children of `engine/`, and the `#[path = "modules/foo.rs"]`
//! attributes in `engine/mod.rs` are now plain `pub mod foo;`. The
//! `extraction/`, `storage/`, and `tests/` subdirectories are kept
//! nested because they group cohesive concerns.
//!
//! ## Cross-feature consumers
//!
//! `engine/extraction::ContentExtractor` is consumed by:
//! - `features/file/use_cases/read_content.rs`
//! - `features/batch/services/file_import.rs`
//! - `infrastructure/adapters/content_extraction_adapter.rs`
//!
//! These imports continue to resolve via the canonical Strangler Fig
//! path `crate::infrastructure::indexing::extraction::*`.
