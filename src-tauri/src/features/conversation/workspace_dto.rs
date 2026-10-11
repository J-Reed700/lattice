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

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SynthesisStage {
    Gathering,
    Reading,
    Writing,
}

/// Where a synthesis job is, as its activity: actual work boundaries, rather
/// than an estimated completion percent.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SynthesisActivityDto {
    pub stage: SynthesisStage,
    pub entry_count: Option<usize>,
    pub chunk_index: Option<usize>,
    pub chunk_count: Option<usize>,
}

/// Where a finished synthesis is saved. Fixed when the synthesis starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SynthesisDestinationDto {
    /// A new quick-capture page.
    Capture,
    /// The end of an existing journal page.
    #[serde(rename_all = "camelCase")]
    Note { note_id: String },
    /// The week's page, by title, created when it does not exist.
    Week { title: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StartJournalSynthesisRequestDto {
    pub request: SynthesizeJournalEntriesRequestDto,
    pub destination: SynthesisDestinationDto,
    /// What the progress panel calls it.
    pub title: String,
    /// The heading the saved block opens with.
    pub heading: String,
}

/// A synthesis job and what it was asked: live, finished and waiting to be
/// saved, or ended without a result.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct JournalSynthesisDto {
    pub job: crate::shared::runtime::jobs::JobDto,
    pub title: String,
    pub heading: String,
    pub destination: SynthesisDestinationDto,
    pub conversation_ids: Vec<String>,
    /// The job's activity, typed; later changes arrive on `jobs://status`.
    pub activity: Option<SynthesisActivityDto>,
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
