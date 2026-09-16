//! Wire types for passage references.

use serde::{Deserialize, Serialize};

/// A saved excerpt from a document.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PassageReferenceDto {
    pub id: String,
    pub document_id: String,
    pub chunk_id: Option<String>,
    pub file_path: String,
    pub file_name: String,
    /// Human-readable position: "p. 12", "12:40", "§ Methods".
    pub locator: Option<String>,
    pub text: String,
    pub title: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreatePassageReferenceRequestDto {
    pub document_id: String,
    pub chunk_id: Option<String>,
    pub file_path: String,
    pub file_name: String,
    pub locator: Option<String>,
    pub text: String,
    pub title: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePassageReferenceRequestDto {
    pub id: String,
    pub title: Option<String>,
    pub note: Option<String>,
}
