//! # Web feature
//!
//! Web URL ingestion and preview: fetch a URL, extract main content,
//! produce a document. Plus web archive (browser-extension) capture.
//! Self-contained vertical slice.
//!
//! ## Public surface
//!
//! - `crate::features::web::dto` — web DTOs
//! - `crate::features::web::domain` — WebArchive domain
//! - `crate::features::web::use_cases` — web URL/preview/archive use cases
//! - `crate::features::web::services` — WebService, WebCaptureService,
//!   WebArchiveService, WebIngestionService
//! - `crate::features::web::commands` — Tauri command handlers
//! - `crate::features::web::images` — chat pictures: find, fetch, store,
//!   lease, serve over `lattice-image`; implements `ChatImagesPort`
//! - `crate::features::web::plugin::init()` — Tauri plugin
//! - `crate::features::web::{WebIngestionServiceTrait,
//!   WebArchiveServiceTrait, WebCaptureServiceTrait}` — service traits
//!   (`WebServiceTrait` lives in `lattice_core::application::ports`)
//!
//! No application-level `WebPort` — web operations flow through
//! the service traits.

pub mod commands;
pub mod di;
pub mod domain;
pub mod dto;
pub mod images;
pub mod plugin;
pub mod services;
pub mod snapshots;
pub mod traits;
pub mod use_cases;

#[cfg(test)]
pub mod mocks;

pub use traits::{
    ArticleExtractorServiceTrait, WebArchiveServiceTrait, WebCaptureServiceTrait,
    WebIngestionResult, WebIngestionServiceTrait,
};
