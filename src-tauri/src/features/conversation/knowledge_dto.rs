//! User-managed scope and temporal metadata on the existing facts ledger.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeRequestDto {
    pub conversation_id: String,
    /// list, remember, correct, scope, forget, verify, or dates.
    pub action: String,
    pub item_id: Option<String>,
    /// User-authored evidence for remember/correct, never a generated paraphrase.
    pub text: Option<String>,
    pub scope: Option<String>,
    pub kind: Option<String>,
    pub valid_from: Option<String>,
    pub valid_until: Option<String>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeItemDto {
    pub id: String,
    pub conversation_id: String,
    pub conversation_title: String,
    pub label: String,
    pub kind: String,
    pub state: String,
    pub scope: String,
    pub learned_at: String,
    pub valid_from: Option<String>,
    pub valid_until: Option<String>,
    pub verified_at: Option<String>,
    pub forgotten: bool,
    pub superseded_by: Option<String>,
    /// Exact original passages, resolved on every read.
    pub evidence: Vec<super::memory_dto::MemoryEvidenceDto>,
    /// Why this item is visible here; not a claim it appeared in the last prompt.
    pub availability_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeResponseDto {
    pub items: Vec<KnowledgeItemDto>,
    pub has_more: bool,
    /// Items supplied in the initial context of the latest completed answer.
    pub last_answer_memory_ids: Vec<String>,
}
