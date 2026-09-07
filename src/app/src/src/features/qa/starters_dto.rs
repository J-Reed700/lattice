//! DTOs for corpus-derived chat starters (BRIEF rank 11, contract §4.7).

use serde::{Deserialize, Serialize};

/// One suggested opening question.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatStarterDto {
    /// <= 90 characters, no trailing whitespace.
    pub question: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

/// The whole starter payload for the Chat empty state.
///
/// `starters` is empty when no LLM is available. The frontend renders **no**
/// questions in that case — inventing a plausible-sounding question about
/// documents nobody has read is the one thing this feature must never do.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatStartersDto {
    pub fingerprint: String,
    /// RFC 3339.
    pub generated_at: String,
    /// Empty when no LLM is available — the frontend must render no questions.
    pub starters: Vec<ChatStarterDto>,
    pub document_count: i64,
}
