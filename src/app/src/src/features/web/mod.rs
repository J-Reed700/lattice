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
//! - `crate::features::web::plugin::init()` — Tauri plugin
//! - `crate::features::web::{WebServiceTrait, WebIngestionServiceTrait,
//!   WebArchiveServiceTrait, WebCaptureServiceTrait}` — service traits
//!
//! ## Kept as shared
//!
//! - `crate::infrastructure::web` (entry point + article_detector/
//!   content_extractor/metadata/web_fetcher stubs) — registered via
//!   `infrastructure/mod.rs` #[path] redirect so consumers can still
//!   use `crate::infrastructure::web::article_detector::*` etc.
//!
//! No application-level `WebPort` — web operations flow through
//! the service traits.

pub mod commands;
pub mod domain;
pub mod di;
pub mod dto;
pub mod plugin;
pub mod services;
pub mod traits;
pub mod use_cases;

#[cfg(test)]
pub mod mocks;

pub use traits::{
    WebArchiveServiceTrait, WebCaptureServiceTrait, WebIngestionResult, WebIngestionServiceTrait,
    WebServiceTrait,
};
