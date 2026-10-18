// Production code denies these lints in the workspace manifest. Tests may use
// panics and unwraps as assertion failure signals.
#![cfg_attr(test, allow(clippy::unwrap_used))]
#![cfg_attr(test, allow(clippy::expect_used))]
#![cfg_attr(test, allow(clippy::unwrap_in_result))]
#![cfg_attr(test, allow(clippy::indexing_slicing))]
#![cfg_attr(test, allow(clippy::panic))]

//! Lattice's core: the layers every feature builds on.
//!
//! [`domain`] holds the business entities, value objects, and rules.
//! [`application`] holds the cross-feature contracts, ports, mappers, and the
//! services that orchestrate them. [`shared`] provides the foundational
//! errors, types, and runtime utilities both use.
//!
//! This crate does not depend on Tauri or on the desktop crate: adapters,
//! features, and wiring live there and depend on this crate, never the other
//! way round. The `test-support` feature exposes the test doubles and helpers
//! that the desktop crate's tests use.

#![deny(unsafe_code)]

/// Foundation types, errors, and utilities shared across layers.
pub mod shared;

/// Shared domain entities, value objects, and business rules.
pub mod domain;

/// Cross-feature contracts, ports, mappers, and orchestration.
pub mod application;
