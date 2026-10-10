//! Library search for a feature that keeps its own copy of a library
//! document's text. The feature reads the document's chunks once, then asks
//! the library to rank them, so it reuses the library's chunking, vectors and
//! fusion instead of embedding the text a second time.

use std::collections::HashSet;

use async_trait::async_trait;

use crate::shared::error::Result;

/// One chunk of a library document, in reading order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryChunk {
    pub id: String,
    pub text: String,
}

/// An indexed library document's name, location and every chunk in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryDocumentText {
    pub title: String,
    pub file_path: String,
    pub chunks: Vec<LibraryChunk>,
}

/// One ranked chunk, best first in the list it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryPassageHit {
    pub document_id: String,
    pub chunk_id: String,
    pub score: f32,
}

#[async_trait]
pub trait LibraryPassagesPort: Send + Sync {
    /// The embedding model the library's vectors come from. Meaningful once
    /// the caller has loaded the embedding model.
    fn model_identity(&self) -> String;

    /// Every chunk of an indexed document. `None` when the library has no such
    /// document; an error when it exists but is not fully indexed yet, so a
    /// caller never captures part of a document.
    async fn document_text(&self, document_id: &str) -> Result<Option<LibraryDocumentText>>;

    /// The library's hybrid search, confined to `document_ids`.
    async fn search(
        &self,
        query: &str,
        document_ids: &HashSet<String>,
        limit: usize,
    ) -> Result<Vec<LibraryPassageHit>>;
}
