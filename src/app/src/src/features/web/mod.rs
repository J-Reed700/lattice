//! # Web feature
//!
//! Web URL ingestion and preview: fetch a URL, extract main content,
//! produce a document. Plus web archive (browser-extension) capture.
//!
//! 30 files, ~12K LOC — the largest feature slice so far.
//!
//! ## File layout
//!
//! | Path                                | Canonical module path                                        |
//! |-------------------------------------|--------------------------------------------------------------|
//! | `dto.rs`                            | `crate::application::dtos::web_dto`                          |
//! | `domain.rs`                         | `crate::domain::web_archive`                                 |
//! | `use_cases/`                        | `crate::application::use_cases::web`                         |
//! | `commands.rs`                       | `crate::interfaces::commands::web_ingest`                    |
//! | `plugin.rs`                         | `crate::plugins::web`                                        |
//! | `infra_mod.rs`                      | `crate::infrastructure::web` (entry point)                   |
//! | `infra_modules/article_detector.rs` | `crate::infrastructure::web::article_detector`               |
//! | `infra_modules/{content_extractor,metadata,web_fetcher}.rs` | migration-placeholder stubs (not loaded outside infra_mod)  |
//! | `ingestion/*`                       | orphaned — not wired into any mod.rs, kept for historical reference |
//! | `services/archive.rs`               | `crate::infrastructure::services::web_archive_service`       |
//! | `services/capture.rs`               | `crate::infrastructure::services::web_capture`               |
//! | `services/ingestion.rs`             | `crate::infrastructure::services::web_ingestion`             |
//! | `services/web.rs`                   | `crate::infrastructure::services::web_service`               |
//! | `traits/{archive,capture,web}.rs`   | merged into `crate::infrastructure::services::traits` re-exports |
//! | `mocks/{archive,capture,web}.rs`    | merged into `crate::infrastructure::services::mocks` re-exports |
//!
//! No application-level `WebPort` trait — web operations flow through
//! the service traits (`WebServiceTrait`, `WebArchiveServiceTrait`,
//! `WebCaptureServiceTrait`) which live in `services/traits/`.
//!
//! The `ingestion/` subdirectory (8 files, ~2 KB of types/fetcher/extractor/metadata)
//! was discovered to be *unused* — no `pub mod ingestion;` declaration
//! exists anywhere in the tree. Files colocated here for reference;
//! could be deleted in a separate cleanup pass.
