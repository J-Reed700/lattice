//! Study wire types, exported to TypeScript by the bindings generator.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StudySourceDto {
    pub chunk_id: String,
    pub document_id: String,
    pub file_name: String,
    pub file_path: String,
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StudyCardDto {
    pub id: String,
    pub deck_id: String,
    pub question: String,
    pub answer: String,
    pub options: Vec<String>,
    pub correct_index: usize,
    pub explanation: String,
    pub source: StudySourceDto,
    /// Every passage cited by the answer. `source` remains the primary passage
    /// for compatibility with decks created before multi-citation cards.
    #[serde(default)]
    pub citations: Vec<StudySourceDto>,
    pub topic: String,
    pub due_at: i64,
    pub interval_days: i64,
    pub review_count: i64,
    pub lapses: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StudyDeckDto {
    pub id: String,
    pub title: String,
    pub focus: String,
    #[serde(default)]
    pub study_goal: String,
    pub model_name: String,
    pub created_at: i64,
    pub cards: Vec<StudyCardDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StudyDeckSummaryDto {
    pub id: String,
    pub title: String,
    pub focus: String,
    #[serde(default)]
    pub study_goal: String,
    pub created_at: i64,
    pub card_count: i64,
    pub due_count: i64,
    pub quiz_attempts: i64,
    pub quiz_correct: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GenerateStudyDeckRequestDto {
    pub title: String,
    pub document_ids: Vec<String>,
    pub focus: String,
    #[serde(default)]
    pub study_goal: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GenerateConversationStudyDeckRequestDto {
    pub conversation_id: String,
    pub title: String,
}

#[derive(Debug, Clone)]
pub(super) struct VerifiedConversationClaim {
    pub answer: String,
    pub citations: Vec<StudySourceDto>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum StudyRating {
    Again,
    Hard,
    Good,
    Easy,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReviewStudyCardRequestDto {
    pub review_id: String,
    pub card_id: String,
    pub expected_reviews: i64,
    /// A selected option records a quiz answer; otherwise the rating records recall.
    pub selected_option: Option<usize>,
    pub rating: StudyRating,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStudyCardRequestDto {
    pub card_id: String,
    pub question: String,
    pub answer: String,
    pub explanation: String,
}
