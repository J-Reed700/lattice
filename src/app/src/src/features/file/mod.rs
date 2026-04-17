//! # File feature
//!
//! Tauri command surface for frontend-driven file operations:
//! open / show-in-folder / read-content / read-bytes / get-metadata /
//! update-metadata / get-path-by-id / open-by-id.
//!
//! ## File layout
//!
//! | File          | Canonical module path                                 |
//! |---------------|-------------------------------------------------------|
//! | `dto.rs`      | `crate::application::dtos::file_dto`                  |
//! | `use_cases/`  | `crate::application::use_cases::file`                 |
//! | `commands.rs` | `crate::interfaces::commands::file` (aka `file_commands`) |
//! | `plugin/`     | `crate::plugins::file` (directory plugin)             |
//!
//! ## Shared infrastructure (deliberately NOT part of this slice)
//!
//! These live outside `features/file/` because they are genuinely
//! cross-cutting and consumed by multiple features (indexing, batch,
//! function_calling, llm, etc.):
//!
//! - `FileStoragePort` + `SecureFileStorage` (`infrastructure/services/file_storage/`)
//! - `FileMetadata` value object (`domain/value_objects/`)
//! - `FileWatcher` (`infrastructure/file_system/`)
//! - `FileTypeDetector` (`application/services/` + infra service)
//! - `FileAccessConfig` + `file_access` domain port (security)
//! - `FileMetadataFactory` (`application/factories/`)
//! - `file_cleanup`, `file_watch` services
//!
//! Those can graduate into this feature later if usage patterns shift,
//! but today they're shared infra and stay put.
