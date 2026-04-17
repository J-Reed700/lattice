//! # Custom model feature
//!
//! User-added LLM models (from local file or URL) with validation and
//! architecture inference. Self-contained vertical slice.
//!
//! ## Public surface
//!
//! - `crate::features::custom_model::domain` — CustomModel, ModelArchitecture,
//!   TaskType, ValidationStatus, ModelSource
//! - `crate::features::custom_model::use_cases` — add_from_file,
//!   add_from_url, validate, delete, list
//! - `crate::features::custom_model::repository` — CustomModelRepository,
//!   CustomModelRepositoryTrait, MockCustomModelRepository
//! - `crate::features::custom_model::services` —
//!   architecture_inference, file_validation, url_validation
//! - `crate::features::custom_model::commands` — Tauri command handlers
//!
//! No DTO, plugin, or port. Commands and domain types cover the
//! surface directly.

pub mod commands;
pub mod domain;
pub mod repository;
pub mod services;
pub mod use_cases;
