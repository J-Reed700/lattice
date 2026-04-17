//! # QA feature
//!
//! Retrieval-Augmented Generation (RAG) question answering. Combines
//! HyDE-based query interpretation, vector retrieval, and LLM completion
//! to answer questions with inline citations.
//!
//! ## File layout
//!
//! All files in this directory are loaded via `#[path]` redirects from their
//! legacy module locations so existing imports keep working (Strangler Fig).
//! Each file has exactly ONE canonical module path during migration:
//!
//! | File                                | Canonical module path                                              |
//! |-------------------------------------|--------------------------------------------------------------------|
//! | `dto.rs`                            | `crate::application::dtos::qa_dto`                                 |
//! | `use_cases/`                        | `crate::application::use_cases::qa`                                |
//! | `domain/`                           | `crate::domain::qa`                                                |
//! | `engine/`                           | `crate::infrastructure::qa`                                        |
//! | `hyde/`                             | `crate::infrastructure::services::hyde`                            |
//! | `conversational_service.rs`         | `crate::infrastructure::services::conversational_qa_service`       |
//! | `traits.rs`                         | `crate::infrastructure::services::traits` (merged into trait re-exports) |
//! | `mocks.rs`                          | `crate::infrastructure::services::mocks` (merged into mock re-exports) |
//! | `tests/conversational_service.rs`   | `crate::infrastructure::services::tests::test_conversational_qa_service` |
//! | `commands.rs`                       | `crate::interfaces::commands::qa_commands`                         |
//! | `plugin.rs`                         | `crate::plugins::qa_plugin`                                        |
//!
//! Shared ports (`EmbeddingPort`, `LLMPort`, `VectorSearchPort`,
//! `ChunkRepositoryPort`, `DocumentRepositoryPort`) remain in
//! `application/ports/` — features consume shared ports, they don't own them.
//!
//! This `features/qa/mod.rs` exists only so the directory is recognized by
//! `cargo` as a module; it intentionally does not declare submodules to
//! avoid loading each file under two module paths and duplicating types.
