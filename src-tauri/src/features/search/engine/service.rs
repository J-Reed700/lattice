#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub score: f32,
    pub index: usize,

    pub filename: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub created_at: Option<String>,
    pub content: Option<String>,

    pub file_id: Option<String>,
    pub file_path: Option<String>,
    pub file_name: Option<String>,
    pub file_extension: Option<String>,
    pub file_category: Option<String>,
    pub is_indexed: Option<bool>,

    // Additional fields for search commands
    pub document_id: Option<String>,
    pub snippet: Option<String>,
    pub chunk_index: Option<usize>,
    pub updated_at: Option<String>,
}
