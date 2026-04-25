//! # Web Ingestion Use Cases
//!
//! - **Ingest Web URL**: fetch + index web content
//! - **Get URL Preview**: fetch metadata only
//!
//! `clean_article_content` was removed — `web/commands.rs` calls the
//! article-extractor service directly.

pub mod get_url_preview;
pub mod ingest_web_url;

pub use get_url_preview::GetUrlPreviewUseCase;
pub use ingest_web_url::IngestWebUrlUseCase;
