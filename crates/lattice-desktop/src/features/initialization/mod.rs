//! # Initialization feature
//!
//! First-run setup and subsystem initialization (database schema).
//! Self-contained vertical slice.
//!
//! ## Public surface
//!
//! - `crate::features::initialization::dto` — response DTOs
//! - `crate::features::initialization::use_cases` — initialize_database,
//!   first_run_setup
//!
//! No dedicated plugin — the `health` plugin exposes the `initialize_database`
//! command during app startup.

pub mod di;
pub mod dto;
mod repository;
pub mod use_cases;
