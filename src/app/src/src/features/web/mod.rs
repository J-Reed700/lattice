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
//!
//! ## Kept as shared (redirects retained)
//!
//! - `crate::infrastructure::web` (entry point + article_detector/
//!   content_extractor/metadata/web_fetcher stubs) — registered via
//!   `infrastructure/mod.rs` #[path] redirect so consumers can still
//!   use `crate::infrastructure::web::article_detector::*` etc.
//! - `trait_def` (traits.rs per-trait files) and `mocks` remain loaded
//!   via shared `infrastructure::services::{traits,mocks}` aggregators.
//!
//! No application-level `WebPort` — web operations flow through
//! the service traits (`WebServiceTrait`, `WebArchiveServiceTrait`,
//! `WebCaptureServiceTrait`) which live in `services/traits/`.

pub mod commands;
pub mod domain;
pub mod dto;
pub mod plugin;
pub mod services;
pub mod use_cases;
