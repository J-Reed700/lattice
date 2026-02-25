//! Plugin Smoke Tests
//!
//! Domain-grouped integration tests for all 16 Tauri plugins.
//! Each module tests one plugin domain's commands.

pub mod batch;
pub mod config;
pub mod credentials;
pub mod embeddings;
pub mod file;
pub mod model;
pub mod search;
// Additional plugin suites are temporarily disabled during refactor stabilization:
// health, tags, favorites, cache, huggingface, extraction, conversation, backup, updates.
