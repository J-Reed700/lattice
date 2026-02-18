//! Application error types for Recall Desktop.
//!
//! This module provides a unified error type that is:
//! - **Serializable** for Tauri IPC boundaries
//! - **Comprehensive** with 23+ semantic error variants
//! - **Compatible** with Result-based error handling
//!
//! # Example
//!
//! ```rust
//! use crate::shared::error::{AppError, Result};
//!
//! fn find_document(id: &str) -> Result<Document> {
//!     database::get(id)
//!         .ok_or_else(|| AppError::NotFound {
//!             resource_type: "document".into(),
//!             resource_id: id.into(),
//!         })
//! }
//! ```
//!
//! # Error Variants
//!
//! The `AppError` enum provides semantic error types organized by domain:
//!
//! - **Infrastructure**: `Io`, `Database`, `Serialization`, `Migration`, `Configuration`
//! - **Domain**: `NotFound`, `AlreadyExists`, `InvalidInput`, `InvalidOperation`
//! - **Security**: `Unauthorized`, `Forbidden`, `RateLimitExceeded`, `ValidationFailed`
//! - **External Services**: `EmbeddingError`, `SearchError`, `OnnxError`
//! - **System**: `Internal`, `NotImplemented`, `Timeout`, `ResourceExhausted`
//!
//! All variants are serializable and provide user-friendly error messages.

// Main error definitions
mod types;
pub use types::{AppError, ErrorResponse, Result, ResultExt};

// Examples module
#[cfg(any(test, doc))]
pub mod examples;

// Tests module
// Temporarily disabled due to refactoring
// #[cfg(test)]
// mod tests;
