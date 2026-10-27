//! Web ingestion: extract an article from a URL, archive it as markdown, then
//! chunk, embed and store it.

mod config;
mod extract;
mod fetch;
mod index;
mod service;

#[cfg(test)]
mod tests;

pub use config::{WebIngestionConfig, WebIngestionServiceBuilder};
pub use service::WebIngestionService;
