//! Content Extraction Infrastructure
//!
//! This module contains content extractors for various file formats:
//! - **PDF Extractor**: Extract text from PDF files
//! - **DOCX Extractor**: Extract text from Word documents
//! - **Text Extractor**: Plain text file handling
//! - **HTML Extractor**: Extract text from HTML documents
//!
//! # Migration Status
//! - [ ] pdf_extractor.rs - Will move from `extraction/pdf.rs`
//! - [ ] docx_extractor.rs - Will move from `extraction/docx.rs`
//! - [ ] text_extractor.rs - Will move from `extraction/text.rs`
//! - [ ] html_extractor.rs - Will move from `extraction/html.rs`
//!
//! # Supported Formats
//! - PDF (via pdf-extract or similar)
//! - DOCX (via docx-rs)
//! - Plain text
//! - HTML/Markdown

pub mod docx_extractor;
pub mod html_extractor;
pub mod link_parser;
pub mod pdf_extractor;
pub mod text_extractor;

pub mod tag_generator;

// Re-export public types
pub use link_parser::{DocumentInfo, LinkParser, WikiLink};
pub use tag_generator::TagGenerator;
