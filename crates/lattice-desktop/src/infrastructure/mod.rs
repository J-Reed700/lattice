//! Reusable technical implementations and application lifecycle support.
//!
//! - `adapters`: implementations of shared application ports
//! - `events`: in-process commands, broadcast notifications, and event bridges
//! - `ml`: shared model caching and embedding exports
//! - `persistence`: SQLite setup, migrations, and cross-feature repositories
//! - `setup`: directories, database, tracing and tokenizer setup steps, and the
//!   renderer shutdown hook; the startup sequence is in `crate::bootstrap`
//! - Other named modules own filesystem, security, and observability work.
//!
//! Feature-specific loaders and repositories belong with their owning feature.
//! Domain and application modules depend on ports rather than these drivers.

pub mod adapters;
pub mod crash;
pub mod events;
pub mod extraction;
pub mod file_system;
pub mod ml;
pub mod observability;
pub mod persistence;
pub mod security;
pub mod services;
pub mod setup;
pub mod storage;
pub mod vector_index;
