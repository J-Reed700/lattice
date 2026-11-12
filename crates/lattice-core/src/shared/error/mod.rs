//! Serializable error contracts shared by application, domain, and IPC code.
//!
//! `DomainError` and `ApplicationError` describe failures in those layers;
//! `AppError` wraps them and carries infrastructure failures across the app.
//! Adapters implement conversions from database, keyring, download, and LLM
//! errors so these contracts do not depend on those implementations.
//!
//! ```rust
//! use lattice_core::shared::error::{AppError, Result};
//!
//! fn require_document(document: Option<String>) -> Result<String> {
//!     document.ok_or_else(|| AppError::NotFound("Document not found".into()))
//! }
//! ```

mod application;
mod class;
mod domain;
pub use application::ApplicationError;
pub use class::ErrorClass;
pub use domain::DomainError;

// Main error definitions
mod types;
pub use types::{server_cannot_start, AppError, ErrorResponse, Result};
mod context;
pub use context::ResultExt;
