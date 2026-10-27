//! # Web feature
//!
//! Web URL ingestion and preview (fetch a URL, extract its main content,
//! produce a document), web archive capture, and chat pictures (`images`,
//! which implements `ChatImagesPort`). Web operations flow through the service
//! traits; `WebServiceTrait` lives in `lattice_core::application::ports`.

pub mod commands;
pub mod di;
pub mod domain;
pub mod dto;
pub mod images;
pub mod plugin;
pub mod services;
pub mod snapshots;
pub mod sources;
pub mod traits;
pub mod use_cases;

#[cfg(test)]
pub mod mocks;

pub use traits::{
    ArticleExtractorServiceTrait, WebArchiveServiceTrait, WebCaptureServiceTrait,
    WebIngestionResult, WebIngestionServiceTrait,
};
