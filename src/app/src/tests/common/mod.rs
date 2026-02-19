//! # Common Test Utilities
//!
//! Oracle-approved test infrastructure following async best practices.
//!
//! ## Modules
//!
//! - `download_helpers` - Async test helpers for download operations
//! - `mock_http` - Mock HTTP client for Range header verification (Test 26)

pub mod download_helpers;
pub mod mock_http;
