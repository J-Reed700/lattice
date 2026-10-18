use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum CustomCollectionKind {
    Manual,
    Snapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CustomCollectionDto {
    pub id: String,
    pub name: String,
    pub kind: CustomCollectionKind,
    pub parent_id: Option<String>,
    pub document_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateCustomCollectionRequest {
    pub name: String,
    pub kind: CustomCollectionKind,
    pub parent_id: Option<String>,
    pub document_ids: Vec<String>,
}
