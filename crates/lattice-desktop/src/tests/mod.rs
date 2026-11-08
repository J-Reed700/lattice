//! Test Module
//!
//! Contains integration tests and test utilities for the Lattice crate.

// Common test utilities module (used by all tests)
pub mod common;
pub mod sqlite;

// Plugin smoke tests
#[cfg(test)]
pub mod plugins;
