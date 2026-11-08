//! Grounded Practice Workbench sessions, tutoring, grading and proposals.
use super::*;

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
