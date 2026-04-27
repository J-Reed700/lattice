//! Content extraction infrastructure.
//!
//! Hosts `link_parser` (wikilink/title extraction), shared with the
//! extraction and mentions features.
//!
//! File-content extractors (PDF, DOCX, HTML, etc.) live in
//! `features/indexing/engine/extraction/` and are consumed via
//! `crate::infrastructure::indexing::extraction`.

pub mod link_parser;

pub use link_parser::{DocumentInfo, LinkParser, WikiLink};
