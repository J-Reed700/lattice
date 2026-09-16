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
//!
//! No dedicated plugin — the `health` and `embedding` plugins expose the
//! `initialize_database` and `initialize_models` commands during app startup.

pub mod di;
pub mod dto;
pub mod use_cases;
