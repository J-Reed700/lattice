//! Assessment blueprints, forms, submissions and shared evidence contracts.
use super::*;

// Versioned, renderer-safe assessment and shared evidence contracts. Candidate
// answer keys only appear on create requests as opaque JSON and are never part
// of a workspace or pre-submission form response.

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
    pub requirements:
        Vec<crate::features::learning::assessment_engine::LearningBlueprintRequirement>,
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
    pub requirements:
        Vec<crate::features::learning::assessment_engine::LearningBlueprintRequirement>,
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
    pub response: crate::features::learning::assessment_engine::LearningAssessmentResponse,
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
