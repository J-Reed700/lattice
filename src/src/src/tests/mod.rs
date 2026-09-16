//! Test Module
//!
//! Contains integration tests and test utilities for the Lattice crate.

// Common test utilities module (used by all tests)
pub mod common;

// Hexagonal test infrastructure module (optional).
// TODO: Restore when test phase resumes and hexagonal module is present.

// Plugin smoke tests
#[cfg(test)]
pub mod plugins;

// Integration tests module
#[cfg(test)]
pub mod integration;
