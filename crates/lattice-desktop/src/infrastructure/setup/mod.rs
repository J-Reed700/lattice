//! Setup steps with no feature imports: app and model directories, the
//! database, tracing, and the renderer shutdown hook.
//! The startup sequence that calls them is in `crate::bootstrap`.

pub mod database;
pub mod directories;
pub mod indexing_config;
pub mod microphone;
pub mod observability;
pub mod renderer_shutdown;

#[cfg(test)]
mod tests;

pub use database::setup_database;
pub use directories::{setup_app_directories, setup_model_directory};
pub use observability::setup_tracing;
