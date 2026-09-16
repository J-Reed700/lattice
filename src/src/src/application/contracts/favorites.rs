//! Favorites records exchanged through application ports.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FavoriteRecord {
    pub id: String,
    pub document_id: String,
    pub document_name: String,
    pub document_path: String,
    pub file_type: Option<String>,
    pub added_at: String,
}
