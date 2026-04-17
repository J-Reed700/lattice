//! # Initialization feature
//!
//! First-run setup and subsystem initialization (database schema,
//! model downloads).
//!
//! ## File layout
//!
//! | File          | Canonical module path                               |
//! |---------------|-----------------------------------------------------|
//! | `dto.rs`      | `crate::application::dtos::initialization_dto`      |
//! | `use_cases/`  | `crate::application::use_cases::initialization`     |
//! | `commands.rs` | `crate::interfaces::commands::initialization`       |
//!
//! No dedicated plugin — initialization commands are invoked through
//! the shared plugin infrastructure during app startup.
