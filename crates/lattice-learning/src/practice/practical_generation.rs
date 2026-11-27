//! Strict, source-scoped generation for practical activities.
//!
//! Public activity material and hidden executable checks are generated in
//! independent calls. Interviews additionally keep private, reviewed answer
//! guides; these never enter executable checks or public activity material.

use crate::lessons::structured_call::Asking;
use crate::{
    assessment_engine::LearningRubricCriterion,
    dto::{LearningLessonDto, LearningSourceVersionDto},
    practical_dto::{
        LearningInterviewPlanDto, LearningInterviewTargetDto, LearningPracticalActivityKind,
        LearningSimulationSessionDto, LearningSimulationSpeaker,
    },
};
#[cfg(test)]
use lattice_core::application::services::model_contract::ask::Answered;
use lattice_core::application::services::model_contract::ask::{ask, Refusal, Settled};
use lattice_core::application::services::model_contract::packed::Packed;
use lattice_core::application::services::model_contract::shown::{Internal, Scope, Told};
use lattice_core::shared::constants::MAX_CHAT_MESSAGE_CHARS;
use lattice_core::{
    application::ports::LLMPort,
    shared::error::{AppError, Result},
};
use serde::{Deserialize, Serialize};

#[path = "practical_generation/activity.rs"]
mod activity;
#[path = "practical_generation/excerpts.rs"]
mod excerpts;
#[path = "interview_feedback.rs"]
mod interview_feedback;
#[path = "interview_generation.rs"]
mod interview_generation;
#[path = "practical_evaluator.rs"]
mod practical_evaluator;
#[path = "practical_generation/simulation.rs"]
mod simulation;
#[path = "practical_generation/simulation_turn.rs"]
mod simulation_turn;
#[cfg(test)]
#[path = "practical_generation_tests.rs"]
mod tests;

use super::interview_answers::InterviewAnswerGuides;
use super::simulation_policy::{
    InterviewQuestionsPolicy, ScenarioPolicy, SimulationCue, SimulationEvent, SimulationState,
    TurnPolicy,
};
pub use activity::generate_activity_with_plan;
use activity::validate_files;
#[cfg(test)]
use activity::{ActivityInput, PracticalActivity, PracticalDraft};
pub(super) use excerpts::{
    by_source, ranked_excerpts, sentence_snapshots, with_snapshots, Excerpt,
};
pub use interview_generation::generate_interview_plan;
pub(super) use interview_generation::public_interview_activity;
pub(crate) use interview_generation::validate_interviewer_reply;
#[cfg(test)]
use simulation::Turn;
use simulation::{citations_of, Said, SimulationTurn, TurnInput};
pub use simulation_turn::generate_simulation_turn;

pub const INTERVIEW_PLAN_FILE_PATH: &str = "interview/plan.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PracticalFileRole {
    Starter,
    Check,
    Reference,
}

#[derive(Debug, Clone)]
pub struct GeneratedPracticalFile {
    pub path: String,
    pub role: PracticalFileRole,
    pub content: String,
    pub editable: bool,
}

#[derive(Debug, Clone)]
pub struct GeneratedPracticalActivity {
    pub title: String,
    pub brief: String,
    pub source_version_ids: Vec<String>,
    pub rubric: Vec<LearningRubricCriterion>,
    pub files: Vec<GeneratedPracticalFile>,
}

#[derive(Debug, Clone)]
pub struct SimulationActivityContext {
    pub title: String,
    pub brief: String,
    pub rubric: Vec<LearningRubricCriterion>,
    pub interview_plan: Option<LearningInterviewPlanDto>,
    pub interview_answers: Option<InterviewAnswerGuides>,
}

#[derive(Debug, Clone)]
pub struct GeneratedSimulationTurn {
    pub content: String,
    pub citations: Vec<crate::dto::LearningPracticeCitationDto>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimulationJson {
    content: String,
    citations: Vec<crate::dto::LearningPracticeCitationDto>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ActivityJson {
    pub(super) title: String,
    pub(super) brief: String,
    pub(super) source_version_ids: Vec<String>,
    pub(super) rubric: Vec<CriterionJson>,
    pub(super) starter_files: Vec<FileJson>,
    pub(super) reference_files: Vec<FileJson>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CriterionJson {
    pub(super) title: String,
    pub(super) description: String,
    pub(super) max_points: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileJson {
    pub(super) path: String,
    pub(super) content: String,
}

pub fn invalid(message: impl Into<String>) -> AppError {
    AppError::InvalidInput(message.into())
}

pub(super) fn bounded(value: &str, label: &str, maximum: usize) -> Result<()> {
    let length = value.trim().chars().count();
    if length == 0 || length > maximum {
        return Err(invalid(format!(
            "{label} must contain 1–{maximum} characters."
        )));
    }
    Ok(())
}

pub(super) fn present(value: &str, label: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(invalid(format!("{label} must not be blank.")));
    }
    Ok(())
}

/// What a learner may write to ask for an activity: the bound a chat message has.
pub fn validate_request(learner_brief: &str) -> Result<()> {
    bounded(learner_brief, "Practical request", MAX_CHAT_MESSAGE_CHARS)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PracticalRuntimeContext {
    pub name: String,
    pub command: Vec<String>,
    pub contract: String,
    pub learner_contract: String,
    pub evaluator_paths: Vec<String>,
}

impl PracticalRuntimeContext {
    pub(super) fn learner_environment(&self) -> serde_json::Value {
        serde_json::json!({
            "name":self.name,"contract":self.learner_contract,"reservedPaths":self.evaluator_paths,
            "customCommand": if self.evaluator_paths.is_empty() { Some(&self.command) } else { None },
        })
    }
}

pub async fn generate_activity(
    llm: &dyn LLMPort,
    kind: LearningPracticalActivityKind,
    learner_brief: &str,
    lesson: &LearningLessonDto,
    sources: &[LearningSourceVersionDto],
    runtime: Option<&PracticalRuntimeContext>,
    interview_target: Option<&LearningInterviewTargetDto>,
    researched_source_version_ids: &[String],
) -> Result<GeneratedPracticalActivity> {
    generate_activity_with_plan(
        llm,
        kind,
        learner_brief,
        lesson,
        sources,
        runtime,
        interview_target,
        researched_source_version_ids,
        None,
    )
    .await
}

/// A refusal telling the model what an app error said.
pub(super) fn refusal(error: AppError) -> Refusal {
    Refusal(match error {
        AppError::InvalidInput(message) => message,
        other => other.to_string(),
    })
}
