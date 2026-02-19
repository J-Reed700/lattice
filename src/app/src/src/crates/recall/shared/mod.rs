//! Shared Kernel
//!
//! Foundation types and utilities used across all layers.

pub mod api_result;
pub mod constants;
pub mod domain_types;
pub mod error;
pub mod gateway_helpers;
pub mod result;
pub mod text_utils;
// REMOVED: pub mod traits; (5,125-line god object eliminated - traits migrated to infrastructure/services/traits/)
pub mod utils;

// Re-export commonly used types
pub use api_result::{ApiError, ApiResult, ErrorCode};
pub use constants::*;
pub use domain_types::*;
pub use error::{AppError, Result};
pub use gateway_helpers::{into_api_result, IntoApiResult};
pub use result::Result as StdResult;
