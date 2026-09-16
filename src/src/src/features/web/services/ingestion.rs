//! Web ingestion service implementation
//!
//! Orchestrates the complete workflow for importing web articles:
//! 1. Extract article content from URL
//! 2. Archive article as markdown file in ~/.lattice/web-archive
//! 3. Chunk text content
//! 4. Generate embeddings
//! 5. Store document + chunks + embeddings

mod config;
mod extract;
mod fetch;
mod index;
mod service;

#[cfg(test)]
mod tests;

pub use config::{WebIngestionConfig, WebIngestionServiceBuilder};
pub use service::WebIngestionService;
