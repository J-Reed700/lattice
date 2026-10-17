//! Setup steps with no feature imports: app and model directories, the
//! database, tracing, the optional tokenizer, and the renderer shutdown hook.
//! The startup sequence that calls them is in `crate::bootstrap`.

pub mod database;
pub mod directories;
pub mod embedding;
pub mod indexing_config;
pub mod observability;
pub mod renderer_shutdown;

#[cfg(test)]
mod tests;

pub use database::setup_database;
pub use directories::{setup_app_directories, setup_model_directory};
pub use embedding::setup_tokenizer;
pub use observability::setup_tracing;
