// Production code denies these lints in Cargo.toml. Tests may use panics and
// unwraps as assertion failure signals.
#![cfg_attr(test, allow(clippy::unwrap_used))]
#![cfg_attr(test, allow(clippy::expect_used))]
#![cfg_attr(test, allow(clippy::unwrap_in_result))]
#![cfg_attr(test, allow(clippy::indexing_slicing))]
#![cfg_attr(test, allow(clippy::panic))]

//! Lattice's desktop backend.
//!
//! Product behavior lives in [`features`], grouped by capability (conversation,
//! embedding, search, learning, and so on). A feature owns its commands, use
//! cases, services, and repositories. Cross-feature contracts and ports live
//! in [`application`]; shared business types live in [`domain`].
//!
//! [`infrastructure`] contains reusable technical implementations: persistence,
//! messaging, model caching, filesystem access, security, and app setup.
//! [`interfaces`] wires dependencies and exposes shared command adapters;
//! [`plugins`] registers the feature-owned Tauri plugins. [`shared`] provides
//! foundational types and utilities used across these modules.
//!
//! Keep database access behind repositories and dependency direction enforced
//! by `scripts/check-rust-layer-boundaries.sh`. See `CONTRIBUTING.md` for the
//! module placement rules and verification commands.
//!
//! The desktop entry point is `main.rs`. Developer executables live in `bin/`;
//! `desktop_e2e/` belongs to the instrumented executable, not this library.

#![deny(unsafe_code)]

/// Foundation types, errors, and utilities shared across layers.
pub mod shared;

/// Product capabilities, each owning its feature-specific implementation.
pub mod features;

// Re-export commonly used shared types for convenience
pub use shared::{
    constants::*,
    encoding as alignment,
    error::{AppError, ErrorResponse, Result, ResultExt},
    resilience::retry,
    types::{ChunkId, DocumentId, MentionId, TagId, ValidatedFilePath},
};

/// Shared domain entities, value objects, and business rules.
pub mod domain;

pub use domain::{
    entities::{Chunk, SearchResult},
    services::ChunkingService,
    value_objects::{
        Checksum, ChunkingStrategy, FileMetadata, IndexingOutcome, SearchMode, SearchQuery,
    },
    Document, DocumentStatus,
};

/// Cross-feature contracts, ports, mappers, and orchestration.
#[cfg(feature = "indexing")]
pub mod application;

#[cfg(feature = "indexing")]
pub use crate::features::search::use_cases::{HybridSearchUseCase, SemanticSearchUseCase};
#[cfg(feature = "indexing")]
pub use application::{
    mappers,
    ports::{
        EmbeddingPort, FileStoragePort, LLMPort, NotificationPort, RepositoryPort, TextSearchPort,
        VectorSearchPort,
    },
};

/// Reusable technical implementations and application setup.
pub mod infrastructure;

pub use infrastructure::{
    audit, extraction, file_system, ml, observability, persistence, security, services,
};

/// Shared IPC adapters and dependency injection.
pub mod interfaces;

pub use interfaces::di::Container;

/// Registration of feature-owned Tauri plugins.
pub mod plugins;

use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// Get the directory where ML models are stored.
///
/// Returns the application data directory joined with "models".
///
/// # Errors
///
/// Returns an error if the app data directory cannot be determined.
pub fn get_model_dir(app: &AppHandle) -> Result<PathBuf> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("Failed to get app data directory: {}", e)))?;
    Ok(app_data.join("models"))
}

/// Get the path to the ONNX model file.
///
/// Returns the model directory joined with "model.onnx".
///
/// # Errors
///
/// Returns an error if the model directory cannot be determined.
pub fn get_model_path(app: &AppHandle) -> Result<PathBuf> {
    Ok(get_model_dir(app)?.join("model.onnx"))
}

/// Get the path to the tokenizer file.
///
/// Returns the model directory joined with "tokenizer.json".
///
/// # Errors
///
/// Returns an error if the model directory cannot be determined.
pub fn get_tokenizer_path(app: &AppHandle) -> Result<PathBuf> {
    Ok(get_model_dir(app)?.join("tokenizer.json"))
}

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Library name
pub const NAME: &str = env!("CARGO_PKG_NAME");

// Test modules (public for test utilities)
#[cfg(test)]
pub mod tests;

// Unit tests for lib.rs metadata
#[cfg(test)]
mod lib_tests {
    use super::*;

    #[test]
    fn test_version_exists() {
        assert!(!VERSION.is_empty());
        assert_eq!(NAME, "lattice-desktop");
    }

    #[test]
    fn test_public_api_re_exports() {
        // Test that commonly used types are re-exported at crate root
        let _: Result<()> = Ok(());
        let _: AppError = AppError::Other("test".to_string());
    }
}
