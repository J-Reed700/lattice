//! # Mentions feature
//!
//! Inline [[wiki-link]]-style references between documents. Extract /
//! create / update / delete / search / list-by-type / list-for-doc /
//! backlinks.
//!
//! ## File layout
//!
//! | File             | Canonical module path                                                     |
//! |------------------|---------------------------------------------------------------------------|
//! | `dto.rs`         | `crate::application::dtos::mention_dto`                                   |
//! | `mapper.rs`      | `crate::application::mappers::mention_mapper`                             |
//! | `entity.rs`      | `crate::domain::entities::mention` (Mention, MentionType)                 |
//! | `use_cases/`     | `crate::application::use_cases::mentions`                                 |
//! | `repository.rs`  | `crate::infrastructure::persistence::repositories::mention_repository`    |
//! | `trait_def.rs`   | `crate::infrastructure::services::traits` (merged into trait re-exports)  |
//! | `mocks.rs`       | `crate::infrastructure::services::mocks` (merged into mock re-exports)    |
//! | `commands.rs`    | `crate::interfaces::commands::mentions` (aka `mentions_commands`)         |
//! | `plugin.rs`      | `crate::plugins::mention_plugin`                                          |
//!
//! First migration to pull a *domain entity* into a feature slice.
//! `MentionRepositoryPort` stays in `application/ports/`.
