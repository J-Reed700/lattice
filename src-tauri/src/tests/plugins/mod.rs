//! Plugin Smoke Tests
//!
//! Smoke tests for plugin domains that use the crate's test container.
//! DTO tests for other plugin domains live in the `plugins_tests` integration target.

pub mod batch;
pub mod credentials;
pub mod embeddings;
pub mod file;
pub mod model;
pub mod search;
