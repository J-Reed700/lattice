//! # Web Ingestion Use Cases
//!
//! Use cases for web content operations:
//! - **Ingest Web URL**: Fetch and index web content
//! - **Get URL Preview**: Fetch metadata without indexing
//! - **Clean Article Content**: Extract clean content from HTML
//!
//! ## Module Organization
//!
//! Each use case is self-contained with:
//! - Business logic orchestration
//! - Input validation
//! - Service coordination
//! - Comprehensive tests

pub mod clean_article_content;
pub mod get_url_preview;
pub mod ingest_web_url;

// Re-export use cases
pub use clean_article_content::CleanArticleContentUseCase;
pub use get_url_preview::GetUrlPreviewUseCase;
pub use ingest_web_url::IngestWebUrlUseCase;
