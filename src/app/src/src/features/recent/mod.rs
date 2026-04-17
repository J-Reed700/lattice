//! # Recent documents feature
//!
//! Track and list recently-accessed documents. No dedicated Tauri plugin.
//!
//! ## File layout
//!
//! | File             | Canonical module path                                                 |
//! |------------------|-----------------------------------------------------------------------|
//! | `dto.rs`         | `crate::application::dtos::recent_dto`                                |
//! | `mapper.rs`      | `crate::application::mappers::recent_document_mapper`                 |
//! | `use_cases/`     | `crate::application::use_cases::recent`                               |
//! | `repository.rs`  | `crate::infrastructure::persistence::repositories::recent_documents_repository` |
//! | `commands.rs`    | `crate::interfaces::commands::recent_documents` (aka `recent_commands`) |
//!
//! `RecentDocumentsRepositoryPort` stays in `application/ports/`.
