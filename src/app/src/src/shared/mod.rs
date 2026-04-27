//! Shared Kernel
//!
//! Foundation types and utilities used across all layers.

#[path = "modules/api_result.rs"]
pub mod api_result;
#[path = "modules/constants.rs"]
pub mod constants;
#[path = "modules/domain_types.rs"]
pub mod domain_types;
pub mod error;
#[path = "modules/result.rs"]
pub mod result;
#[path = "modules/text_utils.rs"]
pub mod text_utils;
// REMOVED: pub mod traits; (5,125-line god object eliminated - traits migrated to infrastructure/services/traits/)
pub mod utils;

// Re-export commonly used types
pub use api_result::{ApiError, ApiResult, ErrorCode};
pub use constants::*;
pub use domain_types::*;
pub use error::{AppError, Result};
