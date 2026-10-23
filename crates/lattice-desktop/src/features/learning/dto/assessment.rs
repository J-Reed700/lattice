//! Assessment blueprints, forms, submissions and shared evidence contracts.
use super::*;

// Renderer-safe quiz and evidence contracts. Answer keys never cross IPC: a
// form carries a key only as `key_reveal`, after submission (or per answered
// item in practice mode).

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
    /// A written answer (or short-answer near-miss) waiting for the grading
    /// job. Never a fail: no verdict is given while any item is pending.
    Pending,
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
    /// The passage the question was written from, for the source line.
    pub source_quote: String,
    pub previously_exposed: bool,
    /// Saved choice indices in shown order (single choice, true/false,
    /// multi-select).
    pub selected_indices: Vec<usize>,
    /// Saved answers per cloze blank.
    pub blank_responses: Vec<String>,
    /// Saved right-hand index per left-hand matching entry.
    pub matches: Vec<Option<usize>>,
    /// Matching only: the entries to match, and what they match to (which may
    /// hold one extra distractor). Empty for every other format.
    pub left_items: Vec<String>,
    pub right_items: Vec<String>,
    /// Cloze only: how many `{{n}}` blanks the prompt holds.
    pub blank_count: u32,
    pub text_response: Option<String>,
    pub ordered_values: Vec<String>,
    pub artifact_json: Option<serde_json::Value>,
    pub response_revision: i64,
    /// The key and why, only once the form is submitted (or, in practice
    /// mode, once this item is answered). Never present before.
    pub key_reveal: Option<LearningKeyRevealDto>,
    /// Short answers only: whether a number is expected and in which unit.
    /// Never the answer itself.
    pub answer_hint: Option<LearningAnswerHintDto>,
}

/// What a short-answer input may say before submission.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningAnswerHintDto {
    pub numeric: bool,
    /// The key's unit, e.g. "mM", when the answer is a number with a unit.
    pub unit_hint: Option<String>,
}

/// One part of a keyed answer: a cloze blank, a multi-select option, a
/// matching pair or an ordering position, by its shown index.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningPartResult {
    pub index: usize,
    pub correct: bool,
    /// Why the part was wrong; empty when it was right.
    pub detail: String,
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
    /// Partial credit: points this item earned out of its points.
    pub points_earned: f64,
    pub points_possible: f64,
    /// Why a keyed item lost points, in words ("2 of 3 right, 1 wrong pick").
    pub detail: String,
    /// The learner overrode an upheld regrade; counted as full credit.
    pub learner_override: bool,
    /// The model grader's checked reasons (section 4.3a). None for keyed items.
    pub rationale: Option<LearningGradeRationaleDto>,
    /// Per-part verdicts for cloze, multi-select, matching and ordering.
    pub part_results: Vec<LearningPartResult>,
    pub contest: Option<LearningGradeContestDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningAssessmentSubmissionDto {
    pub score: f64,
    /// None while any item is pending: no verdict without every grade.
    pub passed: Option<bool>,
    pub grade_status: LearningAssessmentGradeStatus,
    pub grader_model: Option<String>,
    pub grader_disagreement: Vec<String>,
    pub feedback: String,
    pub item_results: Vec<LearningAssessmentItemResultDto>,
    pub submitted_at: i64,
    /// The score before any learner override; None when nothing is overridden.
    pub score_without_overrides: Option<f64>,
    pub pending_items: i64,
    /// The `quiz_grading` job grading the pending items, while one exists.
    pub grading_job_id: Option<String>,
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
    pub purpose: LearningAssessmentPurpose,
    pub passing_score: f64,
    pub feedback_timing: LearningFeedbackTiming,
    pub source_version_ids: Vec<String>,
    pub model_name: Option<String>,
    pub items: Vec<LearningAssessmentFormItemDto>,
    pub created_at: i64,
    pub updated_at: i64,
    pub submitted_at: Option<i64>,
    pub submission: Option<LearningAssessmentSubmissionDto>,
    /// The quiz this form was taken for; None only for unscoped legacy forms.
    pub scope: Option<LearningQuizScopeDto>,
    pub mode: LearningQuizMode,
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

// Quizzes and tests (docs/design/2026-10-23-quizzes-and-tests.md, section 6.3).

/// Which quiz: a lesson quiz (purpose `checkpoint`), a module test
/// (`module_test`) or the course exam (`cumulative`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LearningQuizScopeKind {
    Lesson,
    Module,
    Course,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct LearningQuizScopeDto {
    pub kind: LearningQuizScopeKind,
    /// The lesson or module id; None for the course exam.
    pub id: Option<String>,
}

/// Graded counts toward the verdict; practice is ungraded with feedback per item.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningQuizMode {
    Graded,
    Practice,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningQuizBankStatus {
    NotPrepared,
    Preparing,
    Ready,
    Failed,
}

/// From the best attempt. Pending while any item of it awaits grading.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningQuizVerdict {
    NotTaken,
    Pending,
    Passed,
    NotPassed,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningQuizAttemptSummaryDto {
    pub form_id: String,
    pub score: f64,
    pub passed: Option<bool>,
    pub grade_status: LearningAssessmentGradeStatus,
    pub submitted_at: i64,
    pub retake_of_form_id: Option<String>,
    pub overridden_items: i64,
    pub pending_items: i64,
    /// 1 for the first graded attempt at this quiz, then 2, 3…
    pub attempt_number: i64,
}

/// Which of the scope's lessons the quiz covers now (module test, course exam).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningQuizCoverageDto {
    pub covered_lessons: i64,
    pub total_lessons: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningQuizStatusDto {
    pub scope: LearningQuizScopeDto,
    pub title: String,
    pub bank_status: LearningQuizBankStatus,
    /// The bank's state in words; empty when nothing needs saying.
    pub bank_message: String,
    pub bank_size: i64,
    /// Items the learner has not seen yet.
    pub fresh_items: i64,
    pub form_size: i64,
    pub expected_minutes: i64,
    pub passing_score: f64,
    pub coverage: Option<LearningQuizCoverageDto>,
    /// The open or paused form, when there is one, with when it was started
    /// and last saved, and whether it is paused.
    pub active_form_id: Option<String>,
    pub active_form_started_at: Option<i64>,
    pub active_form_updated_at: Option<i64>,
    pub active_form_paused: bool,
    pub verdict: LearningQuizVerdict,
    pub best: Option<LearningQuizAttemptSummaryDto>,
    pub latest: Option<LearningQuizAttemptSummaryDto>,
    pub attempts: i64,
    pub preparing_job_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningLessonQuizDto {
    pub lesson_id: String,
    pub quiz: LearningQuizStatusDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningModuleQuizzesDto {
    pub module_id: String,
    pub test: LearningQuizStatusDto,
    pub lessons: Vec<LearningLessonQuizDto>,
}

/// Per course; pass marks are fractions of the points (0.7 = 70 %).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningQuizSettingsDto {
    pub quiz_pass: f64,
    pub test_pass: f64,
    pub exam_pass: f64,
    pub written_answers: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningQuizOverviewDto {
    pub program_id: String,
    pub settings: LearningQuizSettingsDto,
    pub exam: LearningQuizStatusDto,
    pub modules: Vec<LearningModuleQuizzesDto>,
    /// Lesson quizzes only.
    pub quizzes_passed: i64,
    pub quizzes_total: i64,
    pub outcomes: Vec<LearningOutcomeDefinitionDto>,
    pub evidence: Vec<LearningEvidenceEventDto>,
    pub follow_ups: Vec<LearningFollowUpRecommendationDto>,
}

/// The key after submission, in the item's shown option order.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningKeyRevealDto {
    pub correct_indices: Vec<usize>,
    pub accepted_answer: Option<String>,
    pub ordered_values: Vec<String>,
    pub matches: Vec<usize>,
    pub blanks: Vec<String>,
    pub explanation: String,
    /// Per option, why it is wrong; empty strings for the key options.
    pub distractor_rationales: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningGradeContestStatus {
    Open,
    Revised,
    Upheld,
    Overridden,
    Failed,
}

/// "I think I was right": the first grade and the blind regrade, side by side.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningGradeContestDto {
    pub id: String,
    pub status: LearningGradeContestStatus,
    pub note: String,
    pub first_score: f64,
    pub regrade_score: Option<f64>,
    pub regrade_reason: Option<String>,
    pub first_rationale: LearningGradeRationaleDto,
    pub regrade_rationale: Option<LearningGradeRationaleDto>,
    /// The regrade's direct answer to the learner's note.
    pub response_to_note: Option<String>,
    pub created_at: i64,
    pub decided_at: Option<i64>,
}

/// Section 4.3a. Uncertain when a quote failed its check against the answer
/// or the source.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningCriterionVerdict {
    Met,
    PartlyMet,
    Missed,
    Uncertain,
}

/// One rubric criterion's checked reason.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningCriterionRationaleDto {
    pub criterion_id: String,
    pub title: String,
    pub verdict: LearningCriterionVerdict,
    /// None when the criterion could not be graded.
    pub points: Option<f64>,
    pub max_points: f64,
    /// Required whenever points are lost.
    pub what_was_missing: String,
    /// Verbatim spans of the learner's answer, each checked as a substring;
    /// empty when the problem is an omission.
    pub your_words: Vec<String>,
    /// What a full-credit answer says: names the missing points, never a
    /// model answer to copy.
    pub full_credit_includes: String,
    pub source_quote: Option<String>,
    pub source_version_id: Option<String>,
}

/// Why a model-graded answer earned what it did. A grade without a checkable
/// reason is not shown as a grade.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningGradeRationaleDto {
    pub summary: String,
    pub criteria: Vec<LearningCriterionRationaleDto>,
    pub grader_model: String,
    pub rubric_version: String,
    pub graded_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateLearningQuizSettingsRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub settings: LearningQuizSettingsDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PrepareLearningQuizBankRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub scope: LearningQuizScopeDto,
    /// Add questions to a ready bank ("Prepare more questions").
    pub top_up: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StartLearningQuizRequestDto {
    pub operation_id: String,
    pub form_id: String,
    pub program_id: String,
    pub scope: LearningQuizScopeDto,
    pub mode: LearningQuizMode,
    pub retake_of_form_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResumeLearningQuizGradingRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub form_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ContestLearningQuizGradeRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub form_id: String,
    pub item_id: String,
    /// The learner's argument; may be empty.
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OverrideLearningQuizGradeRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub form_id: String,
    pub item_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReportLearningQuizItemRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub form_id: String,
    pub item_id: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AddLearningQuizMissesToRecallRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub form_id: String,
    pub item_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningQuizMissesToRecallDto {
    pub cards_created: i64,
    pub deck_id: String,
}
