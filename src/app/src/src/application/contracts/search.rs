//! Search records exchanged through application ports.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultRecord {
    pub doc_id: String,
    pub chunk_id: String,
    pub score: f32,
    pub content: String,
}
