//! Lesson notes, card drafts and the learner's memory state.
use super::*;
use crate::features::learning::recall::study_dto::StudyDeckDto;
use lattice_core::application::contracts::notes::WorkspaceNoteDto;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningCardDraftOrigin {
    Generated,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningLessonNoteDto {
    pub lesson_id: String,
    pub note: WorkspaceNoteDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningCardDraftDto {
    pub id: String,
    pub lesson_id: String,
    pub question: String,
    pub answer: String,
    pub explanation: String,
    pub source_ids: Vec<String>,
    pub origin: LearningCardDraftOrigin,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningCardOriginDto {
    pub card_id: String,
    pub lesson_id: String,
    pub origin: LearningCardDraftOrigin,
    pub source_ids: Vec<String>,
    pub accepted_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningMemoryDto {
    pub program_id: String,
    pub journal_id: Option<String>,
    pub lesson_notes: Vec<LearningLessonNoteDto>,
    pub study_deck: Option<StudyDeckDto>,
    pub drafts: Vec<LearningCardDraftDto>,
    pub accepted_cards: Vec<LearningCardOriginDto>,
    pub due_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EnsureLearningLessonNoteRequestDto {
    pub program_id: String,
    pub lesson_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GenerateLearningCardDraftsRequestDto {
    pub program_id: String,
    pub lesson_id: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveLearningCardDraftRequestDto {
    pub draft_id: Option<String>,
    pub program_id: String,
    pub lesson_id: String,
    pub question: String,
    pub answer: String,
    pub explanation: String,
    pub source_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningCardDraftActionRequestDto {
    pub program_id: String,
    pub draft_id: String,
}

#[derive(Debug, Clone)]
pub struct LearningMemoryState {
    pub journal_id: Option<String>,
    pub deck_id: Option<String>,
    pub lesson_notes: Vec<(String, String)>,
    pub drafts: Vec<LearningCardDraftDto>,
    pub accepted_cards: Vec<LearningCardOriginDto>,
}
