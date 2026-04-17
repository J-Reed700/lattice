//! # Features
//!
//! Vertical feature slices. Each feature is a self-contained module
//! that groups its use cases, adapters, DTOs, commands, and Tauri plugin
//! into a single directory.
//!
//! Features consume shared ports defined in `application/ports/` and
//! shared primitives from `core/` / `shared/`. They do not own cross-cutting
//! abstractions.

pub mod backup;
pub mod qa;
pub mod updates;
