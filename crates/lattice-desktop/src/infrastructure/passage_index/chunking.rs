//! How a document becomes passages.
//!
//! Two chunkers sit side by side, and the feature that owns an index picks
//! one: [`lines`] cuts a text file at structure (a folder's files, numbered
//! by line), and [`spans`] cuts prose into overlapping windows of whole
//! sentences (a captured source, addressed by byte). Both hand the store the
//! same [`NewPassage`], so the store, the embedding loop and the search never
//! know which made a passage.

pub mod lines;
pub mod spans;

/// One passage of a document, before it is stored.
///
/// `start` and `end` are in the owner's units, which the store keeps without
/// reading them: 1-based inclusive lines for a folder's files, a byte range
/// for a captured source. `text` is exactly what is full-text indexed and,
/// through the index's `embed_text`, embedded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPassage {
    pub start: usize,
    pub end: usize,
    pub text: String,
}
