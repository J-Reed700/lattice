//! Unified edit pipeline for correcting existing content in place.

pub mod kind;
mod notices;
mod ops;
pub mod registry;
mod revision;
mod rules;
pub mod service;
pub mod types;

#[cfg(test)]
mod migration_tests;
#[cfg(test)]
mod registry_tests;
#[cfg(test)]
mod types_tests;

pub use kind::*;
pub use registry::*;
pub use revision::revision_of;
pub use service::*;
pub use types::*;
