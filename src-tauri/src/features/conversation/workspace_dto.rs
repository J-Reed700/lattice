//! Serializable conversation workspace projections and synthesis contracts.
use serde::{Deserialize, Serialize};

/// One document a chat in this space is allowed to read.
///
/// This is what `@` offers in the composer, so it is derived from
/// `space_document_scope` and nothing else: the picker must never name a
/// document retrieval could not reach, or the user pins a chat to a file it
/// then cannot answer from.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SpaceDocumentDto {
    pub document_id: String,
    pub file_name: String,
    pub category: Option<String>,
    pub modified_at: Option<String>,
}

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
    /// True when this file was attached to this chat rather than filed in the
    /// library. It is then the chat's alone — not listed in the library, not
    /// searchable from anywhere else, and deleted with the conversation — until
    /// "Add to library" releases it.
    pub attached_to_conversation: bool,
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

/// The archived text of a cited page — the permanent per-conversation record,
/// as opposed to the short `excerpt` prompt pointer. Internal to the chat
/// pipeline; never handed to the frontend wholesale.
#[derive(Debug, Clone)]
pub struct ConversationWebSourceSnapshotDto {
    pub title: Option<String>,
    pub content: String,
    pub fetched_at: Option<String>,
    pub truncated: bool,
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
    /// Source passages from the conversations and journal notes, numbered to
    /// match the `[n]` citations in `synthesis`.
    pub sources: Vec<crate::features::qa::dto::SourceDto>,
}
