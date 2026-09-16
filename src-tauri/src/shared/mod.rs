//! Shared Kernel
//!
//! Foundation types and utilities used across all layers.

pub mod api_result;
pub mod constants;
pub mod domain_types;
pub mod error;
pub mod path_confinement;
pub mod result;
pub mod sql_like;
pub mod text_utils;
pub mod time;
// REMOVED: pub mod traits; (5,125-line god object eliminated - traits migrated to infrastructure/services/traits/)
pub mod utils;

pub use api_result::{ApiError, ApiResult, ErrorCode};
pub use constants::*;
pub use domain_types::*;
pub use error::{AppError, Result};
