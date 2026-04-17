//! # Initialization feature
//!
//! First-run setup and subsystem initialization (database schema,
//! model downloads). Self-contained vertical slice.
//!
//! ## Public surface
//!
//! - `crate::features::initialization::dto` — response DTOs
//! - `crate::features::initialization::use_cases` — initialize_database,
//!   initialize_models, first_run_setup
//! - `crate::features::initialization::commands` — Tauri command handlers
//!
//! No dedicated plugin — initialization commands are invoked through
//! the shared plugin infrastructure during app startup.

pub mod commands;
pub mod dto;
pub mod use_cases;
