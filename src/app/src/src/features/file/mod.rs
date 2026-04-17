//! # File feature
//!
//! Tauri command surface for frontend-driven file operations:
//! open / show-in-folder / read-content / read-bytes / get-metadata /
//! update-metadata / get-path-by-id / open-by-id.
//! Self-contained vertical slice.
//!
//! ## Public surface
//!
//! - `crate::features::file::dto` — file DTOs (FileMetadataDto, etc.)
//! - `crate::features::file::use_cases` — file operation use cases
//! - `crate::features::file::commands` — Tauri command handlers
//! - `crate::features::file::plugin::init()` — Tauri plugin (directory-shaped)
//!
//! Shared file infrastructure (FileStoragePort, SecureFileStorage,
//! FileMetadata value object, FileWatcher, FileTypeDetector, etc.)
//! stays outside this slice — consumed by multiple features.

pub mod commands;
pub mod dto;
pub mod plugin;
pub mod use_cases;
