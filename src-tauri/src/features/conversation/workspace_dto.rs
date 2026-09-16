//! Serializable conversation workspace projections and synthesis contracts.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConversationLinkedDocumentDto {
    pub document_id: String,
    pub file_name: String,
    pub file_path: String,
    pub file_type: String,
    pub category: String,
    pub indexed_at: String,
    pub last_referenced_at: String,
    pub reference_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConversationWebSourceDto {
    pub id: String,
    pub url: String,
    pub normalized_url: String,
    pub title: Option<String>,
    pub excerpt: Option<String>,
    pub relevance_score: Option<f32>,
    pub added_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSpaceMembershipDto {
    pub space_id: String,
    pub space_name: String,
    pub is_archived: bool,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SynthesizeJournalEntriesRequestDto {
    pub conversation_ids: Vec<String>,
    pub scope: Option<String>,
    pub max_entries: Option<usize>,
}

/// One source a synthesis drew on. `kind` is "conversation", "reference" or
/// "note"; `id` is that source's own id.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SynthesisCitationDto {
    pub kind: String,
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SynthesizeJournalEntriesResponseDto {
    pub synthesis: String,
    pub scope: String,
    pub entry_count: usize,
    pub chunk_count: usize,
    pub conversation_ids: Vec<String>,
    pub citations: Vec<SynthesisCitationDto>,
}
