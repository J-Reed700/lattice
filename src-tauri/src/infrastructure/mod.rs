//! Reusable technical implementations and application lifecycle support.
//!
//! - `adapters`: implementations of shared application ports
//! - `events`: in-process commands, broadcast notifications, and event bridges
//! - `ml`: shared model caching and embedding exports
//! - `persistence`: SQLite setup, migrations, and cross-feature repositories
//! - `setup`: startup, background workers, and shutdown
//! - Other named modules own filesystem, security, audit, and observability work.
//!
//! Feature-specific loaders and repositories belong with their owning feature.
//! Domain and application modules depend on ports rather than these drivers.

pub mod adapters;
pub mod audit;
pub mod crash;
mod error_conversions;
pub mod events;
pub mod extraction;
pub mod file_system;
pub mod ml;
pub mod observability;
pub mod persistence;
pub mod sagas;
pub mod security;
pub mod services;
pub mod setup;
pub mod storage;
