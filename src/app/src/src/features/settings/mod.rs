//! # Settings feature
//!
//! Application settings: get / update / reset / import / export / validate.
//! The plugin also handles direct HTTP request proxying for frontend
//! (OpenAI-compatible model listing).
//!
//! ## File layout
//!
//! | File              | Canonical module path                                            |
//! |-------------------|------------------------------------------------------------------|
//! | `dto/`            | `crate::application::dtos::settings` (directory-shaped DTO)      |
//! | `mapper.rs`       | `crate::application::mappers::settings_mapper`                   |
//! | `use_cases/`      | `crate::application::use_cases::settings`                        |
//! | `repository.rs`   | `crate::infrastructure::persistence::repositories::settings_repository` |
//! | `plugin.rs`       | `crate::plugins::settings_plugin`                                |
//!
//! `SettingsRepositoryPort` stays in `application/ports/`.
//!
//! No separate `commands.rs` — all Tauri command implementations live
//! inside `plugin.rs` directly.
//!
//! `interfaces/commands/domains/hf_settings.rs` is *not* part of this
//! feature despite the name — it belongs with the HuggingFace
//! concern and will graduate with that feature.
