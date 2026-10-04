//! Cross-cutting primitives grouped by responsibility.
//!
//! Product rules belong in `domain` or their owning feature. Shared modules
//! provide types, errors, IPC responses, encoding, filesystem and HTTP helpers,
//! persistence formatting, runtime support, and resilience policies.

pub mod constants;
pub mod encoding;
pub mod error;
pub mod fs;
pub mod http;
pub mod ipc;
pub mod persistence;
pub mod resilience;
pub mod runtime;
#[cfg(test)]
pub mod testing;
pub mod text;
pub mod types;

pub use constants::*;
pub use error::{AppError, Result};
pub use ipc::{ApiError, ApiResult, ErrorCode};
pub use types::*;
