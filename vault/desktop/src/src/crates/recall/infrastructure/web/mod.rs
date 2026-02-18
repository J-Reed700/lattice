//! Web Infrastructure
//!
//! This module contains web content ingestion:
//! - **Web Fetcher**: HTTP client for fetching web content
//! - **Content Extractor**: Extract main content from web pages
//! - **Metadata**: Extract metadata from web pages
//! - **Article Detector**: Detect and extract metadata from web archive articles
//!
//! # Migration Status
//! - [ ] web_fetcher.rs - Will move from `web_ingestion/fetcher.rs`
//! - [ ] content_extractor.rs - Will move from `web_ingestion/extractor.rs`
//! - [ ] metadata.rs - Will move from `web_ingestion/metadata.rs`
//! - [x] article_detector.rs - Implemented
//!
//! # Features
//! - HTTP/HTTPS fetching
//! - Content extraction (main text, headings, etc.)
//! - Metadata extraction (title, description, author)
//! - Web article detection (browser extension archives)

// pub mod ingestion;  // TODO: Fix syntax errors in extractor.rs tests before exposing
pub mod article_detector;
pub mod content_extractor;
pub mod metadata;
pub mod web_fetcher;

pub use article_detector::WebArticleDetector;
