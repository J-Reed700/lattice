//! Content extraction infrastructure.
//!
//! Hosts `link_parser` (wikilink/title extraction), shared with the
//! extraction and mentions features, and the markup reading every extractor
//! shares: `markup` (tag cursor, entity and byte decoding) and `html_text`
//! (HTML to readable text).
//!
//! File-content extractors (PDF, DOCX, HTML, etc.) live in
//! `features/indexing/engine/extraction/` and are consumed via
//! `crate::features::indexing::engine::extraction`.

pub mod html_text;
pub mod link_parser;
pub(crate) mod markup;

pub use link_parser::{DocumentInfo, LinkParser, WikiLink};
