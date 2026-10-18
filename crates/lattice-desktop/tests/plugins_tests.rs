//! Plugin smoke tests entry point
//!
//! This file serves as the entry point for all plugin smoke tests.
//! Individual plugin tests are organized in the plugins/ subdirectory.

// A panic is the assertion signal for this integration-test crate. The package
// denies these operations in production targets.
#![allow(clippy::expect_used)]
#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic)]
#![allow(clippy::unwrap_in_result)]
#![allow(clippy::unwrap_used)]

mod plugins;
