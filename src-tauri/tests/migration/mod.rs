//! Migration Test Infrastructure
//!
//! This module provides comprehensive testing infrastructure for DDD migration.
//!
//! # Test Organization
//!
//! - `container_helpers` - Utilities for creating and testing ServiceContainer
//! - `health_tests` - Comprehensive health command tests
//! - `tag_tests` - Comprehensive tag command tests
//! - `integration_tests` - Tests for migration coexistence
//! - `validation` - Migration validation suite
//!
//! # Usage
//!
//! ```rust
//! use crate::tests::migration::container_helpers::create_test_container;
//!
//! #[tokio::test]
//! async fn test_with_ddd_container() {
//!     let container = create_test_container().await.unwrap();
//!     // Test using DDD container
//! }
//! ```

pub mod container_helpers;
pub mod health_tests;
pub mod tag_tests;
pub mod integration_tests;
pub mod validation;

pub use container_helpers::*;
