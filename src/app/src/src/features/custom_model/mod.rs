//! # Custom model feature
//!
//! User-added LLM models (from local file or URL) with validation and
//! architecture inference.
//!
//! ## File layout
//!
//! | File                      | Canonical module path                                          |
//! |---------------------------|----------------------------------------------------------------|
//! | `domain.rs`               | `crate::domain::custom_model`                                  |
//! | `use_cases/`              | `crate::application::use_cases::custom_model`                  |
//! | `repository.rs`           | `crate::infrastructure::persistence::repositories::custom_model_repository` |
//! | `services/`               | `crate::infrastructure::services::custom_model`                |
//! |   architecture_inference  |   (+ file_validation, url_validation)                          |
//! | `commands.rs`             | `crate::interfaces::commands::custom_model_commands`           |
//!
//! No DTO, plugin, or port today. Commands and domain types cover the
//! surface directly.
