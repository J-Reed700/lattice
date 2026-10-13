//! A chunk's text alongside the document context it is embedded with.
//!
//! The chunker produces these and every embedding service consumes them, so
//! the shape lives here rather than in either feature.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextualizedChunk {
    pub original_content: String,
    pub contextualized_content: String,
    pub context_prefix: String,
    pub chunk_index: usize,
    pub token_count: usize,
    pub start_idx: usize,
    pub end_idx: usize,
}
