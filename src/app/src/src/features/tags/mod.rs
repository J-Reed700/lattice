//! # Tags feature
//!
//! Document tagging. Create / update / delete / apply / remove / search /
//! auto-generate tags. Includes AI-powered tag generation via LLM.
//!
//! ## File layout
//!
//! | File                     | Canonical module path                                                      |
//! |--------------------------|----------------------------------------------------------------------------|
//! | `entity.rs`              | `crate::domain::entities::tag` (Tag)                                       |
//! | `dto.rs`                 | `crate::application::dtos::tag_dto`                                        |
//! | `mapper.rs`              | `crate::application::mappers::tag_mapper` (domain ↔ DTO)                   |
//! | `use_cases/`             | `crate::application::use_cases::tags`                                      |
//! | `generator.rs`           | `crate::infrastructure::extraction::tag_generator` (LLM prompt helpers)    |
//! | `persistence_mapper.rs`  | `crate::infrastructure::persistence::mappers::tag_mapper` (DB model ↔ entity) |
//! | `repository.rs`          | `crate::infrastructure::persistence::repositories::tag_repository`         |
//! | `service.rs`             | `crate::infrastructure::services::tag_service` (DocumentLockGuard helper)  |
//! | `service_impl.rs`        | `crate::infrastructure::services::tag_service_impl` (TagServiceImpl)       |
//! | `trait_def.rs`           | `crate::infrastructure::services::traits` (merged into re-exports; TagServiceTrait) |
//! | `mocks.rs`               | `crate::infrastructure::services::mocks` (merged into mock re-exports)     |
//! | `commands.rs`            | `crate::interfaces::commands::tag_commands_full` (aka `tags`)              |
//! | `plugin.rs`              | `crate::plugins::tags_plugin`                                              |
//!
//! Notes:
//! - This feature has TWO mappers: the application-level `mapper.rs`
//!   (domain↔DTO) and the infrastructure-level `persistence_mapper.rs`
//!   (DB model↔entity). Both are tag-feature-private, so both colocate.
//! - `service.rs` and `service_impl.rs` split an internal implementation
//!   detail (`DocumentLockGuard` helper vs the actual service impl).
//!   Kept as-is during migration to minimize risk.
//! - `TagServiceTrait` is defined in `trait_def.rs` but has no entry in
//!   `application/ports/` — it was never moved. Shape is unusual compared
//!   to other features and could be cleaned up later.
//!
//! `crate::models::tag::Tag` (gitignored DDD migration shim) is unrelated
//! to this vertical-slice migration and left alone.
