//! Tag records the library and the tools exchange.

use serde::{Deserialize, Serialize};

/// Tag with document count.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct TagWithCountDto {
    /// Tag identifier
    pub id: String,

    /// Tag name
    pub name: String,

    /// Optional color for UI display (hex color code)
    pub color: Option<String>,

    /// Creation timestamp
    pub created_at: String,

    /// Number of documents with this tag
    pub document_count: i64,
}

impl TagWithCountDto {
    /// Get tag ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get tag name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get document count
    pub fn count(&self) -> i64 {
        self.document_count
    }
}
