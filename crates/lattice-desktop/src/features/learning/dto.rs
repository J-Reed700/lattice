//! Studio's subject-neutral IPC contract. Answer keys are excluded from lesson DTOs.

use serde::{Deserialize, Serialize};

mod assessment;
mod canvas;
mod memory;
mod practice;
mod programs;
mod sources;
mod tangents;
mod trust;

pub use super::assessment_engine::{
    LearningAssessmentPurpose, LearningFeedbackTiming, LearningItemFormat, LearningRubricCriterion,
};
pub use assessment::*;
pub use canvas::*;
pub use memory::*;
pub use practice::*;
pub use programs::*;
pub use sources::*;
pub use tangents::*;
pub use trust::*;

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
    /// One core idea's mental model, in plain language.
    Explanation,
    /// One concrete case of the idea, worked step by step.
    WorkedExample,
    /// A deceptively similar case beside the idea, and the one feature that
    /// changes the outcome (learning science lessons §3.2).
    Contrast,
    /// A fresh case the learner reasons through and commits an answer to
    /// before the reasoning below `CHECK_REVEAL` is shown (§3.3).
    Check,
    GuidedPractice,
    IndependentPractice,
    Recap,
    /// The learner explains the lesson from memory before the recap (§3.4).
    Reflection,
}
/// The role a lesson's own reviewed question played when the lesson was
/// written; it sets the question's difficulty in the quiz bank.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LessonQuestionKind {
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
/// One of the six questions the lesson's prose step writes and reviews; it
/// joins the lesson's quiz bank when the lesson is published.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LessonQuestion {
    pub id: String,
    pub kind: LessonQuestionKind,
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
pub struct LearningProgramDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline_review: Option<super::outline_draft::LearningOutlineReviewDto>,
    pub summary: LearningProgramSummaryDto,
    pub prior_knowledge: String,
    pub minutes_per_session: i64,
    pub model_name: String,
    pub modules: Vec<LearningModuleDto>,
    pub sources: Vec<LearningSourceDto>,
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

// Validated internal generation types. Never export answer keys before submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LessonAnswerKey {
    pub question_id: String,
    pub correct_index: usize,
    pub explanation: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedLearningLesson {
    pub verification: Option<super::content_verification::LessonVerificationReport>,
    pub blocks: Vec<LearningBlockDto>,
    pub questions: Vec<LessonQuestion>,
    pub keys: Vec<LessonAnswerKey>,
}
