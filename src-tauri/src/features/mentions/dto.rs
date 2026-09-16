use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MentionDto {
    pub id: String,
    pub name: String,
    pub mention_type: String,
    pub metadata: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MentionWithContextDto {
    pub id: String,
    pub name: String,
    pub mention_type: String,
    pub document_id: String,
    pub context: String,
    pub position: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExtractMentionsResultDto {
    pub mentions: Vec<MentionWithContextDto>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchMentionsResultDto {
    pub mentions: Vec<MentionDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BacklinksResultDto {
    pub document_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GetMentionsForDocumentResultDto {
    pub document_id: String,
    pub mentions: Vec<MentionWithContextDto>,
    pub count: usize,
}
