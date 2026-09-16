//! Wire types for comparison tables.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CompareDocumentsRequestDto {
    pub document_ids: Vec<String>,
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CompareCitationDto {
    pub chunk_id: String,
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CompareCellDto {
    pub value: Option<String>,
    pub citation: Option<CompareCitationDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CompareRowDto {
    pub document_id: String,
    pub title: String,
    pub file_path: String,
    /// One cell per entry in `CompareTableDto::columns`, same order, same length.
    pub cells: Vec<CompareCellDto>,
    /// Set when this document could not be processed (timeout, no chunks, LLM
    /// unavailable). Cells are all-null in that case. Rendered as a muted row note.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CompareTableDto {
    pub columns: Vec<String>,
    pub rows: Vec<CompareRowDto>,
    pub model_name: String,
    pub generated_at: String,
}
