//! Studio's subject-neutral IPC contract. Answer keys are excluded from lesson DTOs.
use crate::features::{daily_notes::commands::WorkspaceNoteDto, study::dto::StudyDeckDto};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningProgramStatus {
    Draft,
    Active,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningPreparation {
    Outline,
    Ready,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningBlockKind {
    Explanation,
    WorkedExample,
    GuidedPractice,
    IndependentPractice,
    Recap,
    Reflection,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningAssessmentKind {
    Practice,
    Quiz,
    Test,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningSourceDto {
    pub id: String,
    pub title: String,
    pub url: Option<String>,
    pub excerpt: String,
    pub acquired_at: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningBlockDto {
    pub kind: LearningBlockKind,
    pub title: String,
    pub body: String,
    pub source_ids: Vec<String>,
    #[serde(default)]
    pub rubric: Vec<LearningPracticeCriterionDto>,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningQuestionDto {
    pub id: String,
    pub kind: LearningAssessmentKind,
    pub prompt: String,
    pub options: Vec<String>,
    pub source_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningLessonDto {
    pub id: String,
    pub title: String,
    pub objective: String,
    pub estimated_minutes: i64,
    pub preparation: LearningPreparation,
    pub blocks: Vec<LearningBlockDto>,
    pub questions: Vec<LearningQuestionDto>,
    pub completed: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningModuleDto {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub outcomes: Vec<String>,
    pub lessons: Vec<LearningLessonDto>,
    #[serde(default)]
    pub prerequisite_module_ids: Vec<String>,
    #[serde(default)]
    pub project: Option<LearningProjectMilestoneDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LearningProjectMilestoneDto {
    pub title: String,
    pub brief: String,
    pub deliverables: Vec<String>,
    pub success_criteria: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningProgramSummaryDto {
    pub id: String,
    pub title: String,
    pub goal: String,
    pub status: LearningProgramStatus,
    pub revision: i64,
    pub module_count: i64,
    pub lesson_count: i64,
    pub completed_lessons: i64,
    pub current_lesson_id: Option<String>,
    pub created_at: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningAnswerDto {
    pub question_id: String,
    pub selected_index: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningQuestionResultDto {
    pub question_id: String,
    pub prompt: String,
    pub options: Vec<String>,
    pub selected_index: usize,
    pub correct_index: usize,
    pub explanation: String,
    pub source_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningAttemptDto {
    pub id: String,
    pub module_id: String,
    pub lesson_id: Option<String>,
    pub kind: LearningAssessmentKind,
    pub correct: i64,
    pub total: i64,
    pub results: Vec<LearningQuestionResultDto>,
    pub submitted_at: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningProgramDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline_review: Option<super::outline_draft::LearningOutlineReviewDto>,
    pub summary: LearningProgramSummaryDto,
    pub prior_knowledge: String,
    pub minutes_per_session: i64,
    pub model_name: String,
    pub modules: Vec<LearningModuleDto>,
    pub sources: Vec<LearningSourceDto>,
    pub attempts: Vec<LearningAttemptDto>,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GenerateLearningProgramRequestDto {
    pub goal: String,
    pub prior_knowledge: String,
    pub minutes_per_session: i64,
    pub document_ids: Vec<String>,
    pub source_urls: Vec<String>,
    /// Omitted by older clients; their existing 2–6 module contract remains valid.
    #[serde(default)]
    pub course_depth: Option<LearningCourseDepth>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningCourseDepth {
    Focused,
    Course,
    DeepDive,
}

impl LearningCourseDepth {
    pub fn bounds(depth: Option<Self>) -> (usize, usize, usize, usize) {
        match depth {
            Some(Self::Focused) => (2, 3, 2, 3),
            Some(Self::Course) => (4, 6, 3, 5),
            Some(Self::DeepDive) => (6, 10, 4, 6),
            None => (2, 6, 2, 6),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AcceptLearningProgramRequestDto {
    pub program_id: String,
    pub expected_revision: i64,
    pub title: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PrepareLearningLessonRequestDto {
    pub program_id: String,
    pub lesson_id: String,
    pub expected_revision: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CompleteLearningLessonRequestDto {
    pub program_id: String,
    pub lesson_id: String,
    pub expected_revision: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SubmitLearningAttemptRequestDto {
    pub attempt_id: String,
    pub program_id: String,
    pub expected_revision: i64,
    pub module_id: String,
    pub lesson_id: Option<String>,
    pub kind: LearningAssessmentKind,
    pub answers: Vec<LearningAnswerDto>,
}

// Validated internal generation types. Never export answer keys before submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningAnswerKey {
    pub question_id: String,
    pub correct_index: usize,
    pub explanation: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedLearningLesson {
    pub verification: Option<super::content_verification::LessonVerificationReport>,
    pub blocks: Vec<LearningBlockDto>,
    pub questions: Vec<LearningQuestionDto>,
    pub keys: Vec<LearningAnswerKey>,
}

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
    pub scheduler_version: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningCanvasSnapshotDto {
    pub id: String,
    pub canvas_id: String,
    pub name: String,
    pub title: String,
    pub description: String,
    pub scene_json: serde_json::Value,
    pub element_count: usize,
    pub canvas_revision: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningCanvasDto {
    pub id: String,
    pub program_id: String,
    pub lesson_id: Option<String>,
    pub title: String,
    pub description: String,
    pub scene_json: serde_json::Value,
    pub element_count: usize,
    pub revision: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub snapshots: Vec<LearningCanvasSnapshotDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningCanvasWorkspaceDto {
    pub program_id: String,
    pub canvases: Vec<LearningCanvasDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateLearningCanvasRequestDto {
    pub operation_id: String,
    pub canvas_id: String,
    pub program_id: String,
    pub lesson_id: Option<String>,
    pub title: String,
    pub description: String,
    pub scene_json: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveLearningCanvasRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub canvas_id: String,
    pub expected_revision: i64,
    pub title: String,
    pub description: String,
    pub scene_json: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateLearningCanvasSnapshotRequestDto {
    pub operation_id: String,
    pub snapshot_id: String,
    pub program_id: String,
    pub canvas_id: String,
    pub expected_revision: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RestoreLearningCanvasSnapshotRequestDto {
    pub operation_id: String,
    pub pre_restore_snapshot_id: String,
    pub program_id: String,
    pub canvas_id: String,
    pub snapshot_id: String,
    pub expected_revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningSourceKind {
    Web,
    Document,
    Pasted,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningSourcePolicy {
    Fixed,
    Manual,
    BeforeUse,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningSourceCheckStatus {
    Unchanged,
    UpdateAvailable,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningSourceVersionSummaryDto {
    pub id: String,
    pub version_number: i64,
    pub title: String,
    pub publisher: Option<String>,
    pub resolved_url: Option<String>,
    pub excerpt: String,
    pub content_sha256: String,
    pub word_count: usize,
    pub truncated: bool,
    pub extraction_version: String,
    pub acquired_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningSourceCheckDto {
    pub operation_id: String,
    pub status: LearningSourceCheckStatus,
    pub checked_at: i64,
    pub active_digest: Option<String>,
    pub pending_version_id: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningSourceLibraryItemDto {
    pub id: String,
    pub kind: LearningSourceKind,
    pub origin: String,
    pub requested_url: Option<String>,
    pub freshness_policy: LearningSourcePolicy,
    pub active_version_id: Option<String>,
    pub pending_version_id: Option<String>,
    pub revision: i64,
    pub deleted_at: Option<i64>,
    pub deletion_reason: Option<String>,
    pub active_version: Option<LearningSourceVersionSummaryDto>,
    pub pending_version: Option<LearningSourceVersionSummaryDto>,
    pub versions: Vec<LearningSourceVersionSummaryDto>,
    pub latest_check: Option<LearningSourceCheckDto>,
    pub checks: Vec<LearningSourceCheckDto>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningSourceWorkspaceDto {
    pub program_id: String,
    pub sources: Vec<LearningSourceLibraryItemDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningSourceUsageDto {
    pub lesson_id: String,
    pub lesson_title: String,
    pub reference_kind: String,
    pub reference_title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningSourceVersionDto {
    pub source_id: String,
    pub version: LearningSourceVersionSummaryDto,
    pub full_text: String,
    pub usage: Vec<LearningSourceUsageDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GetLearningSourceVersionRequestDto {
    pub program_id: String,
    pub source_id: String,
    pub version_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchLearningSourcesRequestDto {
    pub program_id: String,
    pub query: String,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningSourceSearchResultDto {
    pub source_id: String,
    pub version_id: String,
    pub title: String,
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AddLearningWebSourceRequestDto {
    pub operation_id: String,
    pub source_id: String,
    pub version_id: String,
    pub program_id: String,
    pub url: String,
    pub freshness_policy: LearningSourcePolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AddLearningDocumentSourceRequestDto {
    pub operation_id: String,
    pub source_id: String,
    pub version_id: String,
    pub program_id: String,
    pub document_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AddLearningTextSourceRequestDto {
    pub operation_id: String,
    pub source_id: String,
    pub version_id: String,
    pub program_id: String,
    pub title: String,
    pub publisher: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RefreshLearningSourceRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub source_id: String,
    pub expected_revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AdoptLearningSourceVersionRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub source_id: String,
    pub version_id: String,
    pub expected_revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateLearningSourcePolicyRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub source_id: String,
    pub freshness_policy: LearningSourcePolicy,
    pub expected_revision: i64,
}

// Grounded Practice Workbench. These DTOs intentionally preserve the exact
// lesson/source versions used by an attempt; results never resolve through the
// mutable active-source pointers after a session starts.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningPracticeMode {
    Explore,
    Practice,
    Demonstrate,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningPracticeSessionStatus {
    Active,
    Submitted,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningPracticeHintLevel {
    OrientingQuestion,
    ConceptOrSource,
    PartialStrategy,
    WorkedExplanation,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningTutorRequestKind {
    Hint,
    Question,
    Critique,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningPracticeEvidenceDimension {
    Recall,
    Explanation,
    Application,
    Transfer,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningPracticeGradeStatus {
    Provisional,
    Uncertain,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningPracticeAssistanceKind {
    SourceOpened,
    TutorResponse,
    Hint,
    SolutionRevealed,
    ModeChanged,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningPracticeProposalKind {
    Misconception,
    FollowUp,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningPracticeProposalStatus {
    Pending,
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningPracticeCriterionDto {
    pub id: String,
    pub dimension: LearningPracticeEvidenceDimension,
    pub title: String,
    pub description: String,
    pub max_points: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningPracticeCitationDto {
    pub source_id: String,
    pub version_id: String,
    pub quote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningPracticeArtifactDto {
    pub revision: i64,
    pub text: String,
    pub sha256: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningPracticeSessionSummaryDto {
    pub id: String,
    pub lesson_id: String,
    pub lesson_title: String,
    pub status: LearningPracticeSessionStatus,
    pub mode: LearningPracticeMode,
    pub revision: i64,
    pub artifact_revision: i64,
    pub source_version_ids: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub submitted_at: Option<i64>,
    pub grade_status: Option<LearningPracticeGradeStatus>,
    #[serde(default)]
    pub task_kind: LearningPracticeTaskKind,
    #[serde(default)]
    pub revises_session_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningPracticeTaskKind {
    #[default]
    Independent,
    Guided,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningPracticeWorkspaceDto {
    pub program_id: String,
    pub sessions: Vec<LearningPracticeSessionSummaryDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningPracticeAssistanceEventDto {
    pub id: String,
    pub kind: LearningPracticeAssistanceKind,
    pub mode: LearningPracticeMode,
    pub artifact_revision: i64,
    pub details: serde_json::Value,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningPracticeTutorTurnDto {
    pub id: String,
    pub prompt: String,
    pub response: String,
    pub request_kind: LearningTutorRequestKind,
    pub hint_level: Option<LearningPracticeHintLevel>,
    pub citations: Vec<LearningPracticeCitationDto>,
    pub proposal_ids: Vec<String>,
    pub model_name: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningPracticeProposalDto {
    pub id: String,
    pub kind: LearningPracticeProposalKind,
    pub text: String,
    pub evidence_quote: Option<String>,
    pub tutor_turn_id: Option<String>,
    pub status: LearningPracticeProposalStatus,
    pub created_at: i64,
    pub decided_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningPracticeCriterionResultDto {
    pub criterion_id: String,
    pub dimension: LearningPracticeEvidenceDimension,
    pub score: Option<i64>,
    pub max_points: i64,
    pub observation: String,
    pub evidence_quote: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningPracticeEvidenceEventDto {
    pub dimension: LearningPracticeEvidenceDimension,
    pub observed: bool,
    pub observation: String,
    pub evidence_quote: Option<String>,
    pub assistance_kinds: Vec<LearningPracticeAssistanceKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningPracticeResultDto {
    pub grade_status: LearningPracticeGradeStatus,
    pub artifact_revision: i64,
    pub artifact_text: String,
    pub rubric: Vec<LearningPracticeCriterionDto>,
    pub criteria: Vec<LearningPracticeCriterionResultDto>,
    pub evidence: Vec<LearningPracticeEvidenceEventDto>,
    pub mode_at_submission: LearningPracticeMode,
    pub assistance: Vec<LearningPracticeAssistanceEventDto>,
    pub grader_model: String,
    pub submitted_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningPracticeSessionDto {
    pub summary: LearningPracticeSessionSummaryDto,
    pub task_prompt: String,
    pub lesson_objective: String,
    pub rubric: Vec<LearningPracticeCriterionDto>,
    pub artifact: LearningPracticeArtifactDto,
    pub assistance: Vec<LearningPracticeAssistanceEventDto>,
    pub tutor_turns: Vec<LearningPracticeTutorTurnDto>,
    pub proposals: Vec<LearningPracticeProposalDto>,
    pub revealed_solution: Option<String>,
    pub revealed_solution_citations: Vec<LearningPracticeCitationDto>,
    pub result: Option<LearningPracticeResultDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StartLearningPracticeSessionRequestDto {
    pub operation_id: String,
    pub session_id: String,
    pub program_id: String,
    pub lesson_id: String,
    pub expected_program_revision: i64,
    pub mode: LearningPracticeMode,
    #[serde(default)]
    pub task_kind: LearningPracticeTaskKind,
    #[serde(default)]
    pub revises_session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveLearningPracticeArtifactRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub session_id: String,
    pub expected_revision: i64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChangeLearningPracticeModeRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub session_id: String,
    pub expected_revision: i64,
    pub mode: LearningPracticeMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OpenLearningPracticeSourceRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub session_id: String,
    pub expected_revision: i64,
    pub source_id: String,
    pub version_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RequestLearningTutorResponseRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub session_id: String,
    pub expected_revision: i64,
    pub request_kind: LearningTutorRequestKind,
    pub prompt: String,
    pub hint_level: Option<LearningPracticeHintLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RevealLearningPracticeSolutionRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub session_id: String,
    pub expected_revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SubmitLearningPracticeAttemptRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub session_id: String,
    pub expected_revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DecideLearningPracticeProposalRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub session_id: String,
    pub expected_revision: i64,
    pub proposal_id: String,
}

// Versioned, renderer-safe assessment and shared evidence contracts. Candidate
// answer keys only appear on create requests as opaque JSON and are never part
// of a workspace or pre-submission form response.
pub use super::assessment_engine::{
    LearningAssessmentPurpose, LearningFeedbackTiming, LearningItemFormat, LearningRubricCriterion,
};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningBlueprintStatus {
    Draft,
    Accepted,
    Retired,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningAssessmentFormStatus {
    Active,
    Submitted,
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningAssessmentGradeStatus {
    Deterministic,
    Provisional,
    Uncertain,
    NeedsReview,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningEvidenceSourceKind {
    Practice,
    Assessment,
    Recall,
    Practical,
    Simulation,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningEvidenceDimension {
    Recall,
    Explanation,
    Application,
    Transfer,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningEvidenceResult {
    Observed,
    NotObserved,
    Uncertain,
    NotAssessed,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningFollowUpReasonCode {
    MissedOutcome,
    AssistedSuccess,
    LowTransfer,
    StaleEvidence,
    UncertainGrade,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningFollowUpActionKind {
    Lesson,
    Practice,
    Assessment,
    Recall,
    Practical,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningFollowUpStatus {
    Pending,
    Accepted,
    Dismissed,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningOutcomeDefinitionDto {
    pub id: String,
    pub module_id: Option<String>,
    pub lesson_id: Option<String>,
    pub title: String,
    pub description: String,
    pub ordinal: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningAssessmentBlueprintDto {
    pub id: String,
    pub revision: i64,
    pub predecessor_revision: Option<i64>,
    pub purpose: LearningAssessmentPurpose,
    pub title: String,
    pub instructions: String,
    pub expected_minutes: u32,
    pub allowed_aids: Vec<String>,
    pub passing_score: f64,
    pub feedback_timing: LearningFeedbackTiming,
    pub rubric: Vec<LearningRubricCriterion>,
    pub source_version_ids: Vec<String>,
    pub requirements: Vec<super::assessment_engine::LearningBlueprintRequirement>,
    pub status: LearningBlueprintStatus,
    pub change_reason: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningAssessmentCandidateWriteDto {
    pub id: String,
    pub outcome_ids: Vec<String>,
    pub format: LearningItemFormat,
    pub difficulty: u8,
    pub prompt: String,
    pub options: Vec<String>,
    pub artifact_kind: Option<String>,
    pub rubric: Vec<LearningRubricCriterion>,
    pub source_version_ids: Vec<String>,
    pub answer_explanation: String,
    /// Internal tagged key; this type is not Specta-exported or accepted over IPC.
    pub answer_key: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateLearningAssessmentBlueprintRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub blueprint_id: String,
    pub revision: i64,
    pub predecessor_revision: Option<i64>,
    pub purpose: LearningAssessmentPurpose,
    pub title: String,
    pub instructions: String,
    pub expected_minutes: u32,
    pub allowed_aids: Vec<String>,
    pub passing_score: f64,
    pub feedback_timing: LearningFeedbackTiming,
    pub rubric: Vec<LearningRubricCriterion>,
    pub source_version_ids: Vec<String>,
    pub requirements: Vec<super::assessment_engine::LearningBlueprintRequirement>,
    pub change_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningAssessmentFormItemDto {
    pub id: String,
    pub outcome_ids: Vec<String>,
    pub format: LearningItemFormat,
    pub difficulty: u8,
    pub prompt: String,
    pub options: Vec<String>,
    pub artifact_kind: Option<String>,
    pub rubric: Vec<LearningRubricCriterion>,
    pub points: f64,
    pub source_version_ids: Vec<String>,
    pub previously_exposed: bool,
    pub selected_index: Option<usize>,
    pub text_response: Option<String>,
    pub ordered_values: Vec<String>,
    pub artifact_json: Option<serde_json::Value>,
    pub response_revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningAssessmentItemResultDto {
    pub item_id: String,
    pub outcome_ids: Vec<String>,
    pub score: f64,
    pub correct: Option<bool>,
    pub grade_status: LearningAssessmentGradeStatus,
    pub feedback: String,
    pub criterion_results: Vec<LearningAssessmentCriterionResultDto>,
    pub artifact_quotes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningAssessmentCriterionResultDto {
    pub criterion_id: String,
    pub score: Option<u32>,
    pub max_points: u32,
    pub observation: String,
    pub artifact_quote: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningAssessmentSubmissionDto {
    pub score: f64,
    pub passed: bool,
    pub grade_status: LearningAssessmentGradeStatus,
    pub grader_model: Option<String>,
    pub grader_disagreement: Vec<String>,
    pub feedback: String,
    pub item_results: Vec<LearningAssessmentItemResultDto>,
    pub submitted_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningAssessmentFormDto {
    pub id: String,
    pub program_id: String,
    pub blueprint_id: String,
    pub blueprint_revision: i64,
    pub retake_of_form_id: Option<String>,
    pub status: LearningAssessmentFormStatus,
    pub revision: i64,
    pub title: String,
    pub instructions: String,
    pub expected_minutes: u32,
    pub allowed_aids: Vec<String>,
    pub purpose: LearningAssessmentPurpose,
    pub passing_score: f64,
    pub feedback_timing: LearningFeedbackTiming,
    pub rubric: Vec<LearningRubricCriterion>,
    pub source_version_ids: Vec<String>,
    pub model_name: Option<String>,
    pub items: Vec<LearningAssessmentFormItemDto>,
    pub created_at: i64,
    pub updated_at: i64,
    pub submitted_at: Option<i64>,
    pub submission: Option<LearningAssessmentSubmissionDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningAssessmentFormSummaryDto {
    pub id: String,
    pub blueprint_id: String,
    pub blueprint_revision: i64,
    pub title: String,
    pub purpose: LearningAssessmentPurpose,
    pub status: LearningAssessmentFormStatus,
    pub revision: i64,
    pub retake_of_form_id: Option<String>,
    pub score: Option<f64>,
    pub grade_status: Option<LearningAssessmentGradeStatus>,
    pub created_at: i64,
    pub submitted_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningEvidenceEventDto {
    pub id: String,
    pub outcome_id: Option<String>,
    pub source_kind: LearningEvidenceSourceKind,
    pub source_id: String,
    pub dimension: LearningEvidenceDimension,
    pub result: LearningEvidenceResult,
    pub score: Option<f64>,
    pub observation: String,
    pub evidence_quote: Option<String>,
    pub assistance: serde_json::Value,
    pub observed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningFollowUpRecommendationDto {
    pub id: String,
    pub outcome_id: Option<String>,
    pub reason_code: LearningFollowUpReasonCode,
    pub explanation: String,
    pub action_kind: LearningFollowUpActionKind,
    pub action_ref: Option<String>,
    pub status: LearningFollowUpStatus,
    pub evidence_event_ids: Vec<String>,
    pub created_at: i64,
    pub decided_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningAssessmentWorkspaceDto {
    pub program_id: String,
    pub outcomes: Vec<LearningOutcomeDefinitionDto>,
    pub blueprints: Vec<LearningAssessmentBlueprintDto>,
    pub forms: Vec<LearningAssessmentFormSummaryDto>,
    pub evidence: Vec<LearningEvidenceEventDto>,
    pub follow_ups: Vec<LearningFollowUpRecommendationDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StartLearningAssessmentFormRequestDto {
    pub operation_id: String,
    pub form_id: String,
    pub program_id: String,
    pub blueprint_id: String,
    pub blueprint_revision: i64,
    pub retake_of_form_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveLearningAssessmentResponseRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub form_id: String,
    pub expected_revision: i64,
    pub response: super::assessment_engine::LearningAssessmentResponse,
    pub assistance: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MutateLearningAssessmentFormRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub form_id: String,
    pub expected_revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DecideLearningFollowUpRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub follow_up_id: String,
}
