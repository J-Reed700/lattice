//! Immutable curriculum revision and durable generation-job rules.
//!
//! Persistence is added by the program-editing slice; this module owns the
//! deterministic state transition rules so a renderer or model cannot rewrite
//! completed work, started assessments, or an accepted plan in place.

use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

mod jobs;
mod revision;

pub use jobs::{
    advance_generation_job, cancel_generation_job, fail_generation_job, finish_generation_job,
    recover_interrupted_jobs, start_generation_job, validate_generation_job,
};
pub use revision::apply_curriculum_revision;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningCurriculumLessonState {
    Outline,
    Ready,
    Completed,
    Skipped,
    Replaced,
    Challenged,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningCurriculumLesson {
    pub id: String,
    pub title: String,
    pub objective: String,
    pub estimated_minutes: u32,
    pub state: LearningCurriculumLessonState,
    pub assessment_started: bool,
    pub replacement_lesson_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningCurriculumModule {
    pub id: String,
    pub title: String,
    pub purpose: String,
    pub prerequisite_module_ids: Vec<String>,
    pub outcome_ids: Vec<String>,
    pub lessons: Vec<LearningCurriculumLesson>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningCurriculumRevision {
    pub id: String,
    pub program_id: String,
    pub revision_number: u32,
    pub parent_revision_id: Option<String>,
    pub reason: String,
    pub modules: Vec<LearningCurriculumModule>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum LearningCurriculumOperation {
    AddLesson {
        module_id: String,
        after_lesson_id: Option<String>,
        lesson: LearningCurriculumLesson,
    },
    EditLesson {
        lesson_id: String,
        title: String,
        objective: String,
        estimated_minutes: u32,
    },
    MoveLesson {
        lesson_id: String,
        target_module_id: String,
        after_lesson_id: Option<String>,
    },
    SkipLesson {
        lesson_id: String,
    },
    ReplaceLesson {
        lesson_id: String,
        replacement: LearningCurriculumLesson,
    },
    ChallengePrerequisite {
        lesson_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningCurriculumChangeKind {
    Added,
    Edited,
    Moved,
    Skipped,
    Replaced,
    Challenged,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningCurriculumChange {
    pub kind: LearningCurriculumChangeKind,
    pub lesson_id: String,
    pub from_module_id: Option<String>,
    pub to_module_id: Option<String>,
    /// Immutable before/after values make previews auditable and useful to
    /// clients without asking them to reconstruct state from prose.
    pub before: Option<LearningCurriculumLesson>,
    pub after: Option<LearningCurriculumLesson>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningCurriculumRevisionRequest {
    pub revision_id: String,
    pub expected_revision_number: u32,
    pub reason: String,
    pub created_at: i64,
    pub operations: Vec<LearningCurriculumOperation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningCurriculumRevisionResult {
    pub revision: LearningCurriculumRevision,
    pub changes: Vec<LearningCurriculumChange>,
    pub required_lesson_count_before: usize,
    pub required_lesson_count_after: usize,
    pub resume_lesson_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningGenerationJobKind {
    ProgramOutline,
    LessonPreparation,
    AssessmentVariant,
    AdaptiveFollowUp,
    PracticalActivity,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningGenerationJobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningGenerationPhase {
    #[default]
    References,
    Writing,
    Review,
    Inventory,
    Coverage,
    Examples,
    Evidence,
    Research,
    Repair,
    Publishing,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningGenerationStep {
    pub phase: LearningGenerationPhase,
    pub started_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningGenerationActivity {
    pub phase: LearningGenerationPhase,
    pub phase_started_at: i64,
    pub last_activity_at: i64,
    pub last_checkpoint_at: Option<i64>,
    pub lesson_title: Option<String>,
    pub model_name: Option<String>,
    pub model_running: bool,
    pub response_characters: u32,
    pub model_attempt: u32,
    pub verification_pass: u32,
    /// Claims of this pass with a finished first check. Each claim counts once.
    pub checks_completed: u32,
    pub checks_total: u32,
    pub checks_reused: u32,
    /// Checked claims of this pass that are not supported at present. A claim
    /// a deeper evidence page supports leaves this count.
    pub checks_unresolved: u32,
    /// Checked claims of this pass the checker supported but whose sources
    /// the source check found too weak for the claim: research looks for
    /// stronger sources for them. Not counted in `checks_unresolved`.
    pub checks_sent_back: u32,
    /// Set while this pass checks its claims again against references
    /// research has just saved: how many it saved.
    pub new_references: Option<u32>,
    /// None while looking up saved checks; otherwise the number requiring the
    /// model in this pass, after every reusable comparison has been restored.
    pub model_checks_total: Option<u32>,
    /// Set while claims this pass left unsupported are checked against further
    /// pages of their evidence.
    pub deeper_evidence: Option<LearningDeeperEvidence>,
    /// Where the lesson's claims stand after the latest pass; none before
    /// the first pass observed them.
    pub claims: Option<LearningClaimTally>,
    pub recent_steps: Vec<LearningGenerationStep>,
}

/// The lesson's claims by who acts next and what is done to them (§6.3):
/// "Pass 3 · 4 sent back for stronger sources · 2 being repaired · 5
/// waiting for you · 108 verified".
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningClaimTally {
    /// The machine's, not checked yet.
    pub unchecked: u32,
    /// The machine's: no passage supports them or one contradicts them, and
    /// research looks for sources before they are checked again.
    pub searching: u32,
    /// The machine's: the checker supported them, but the source check found
    /// their sources too weak, and research looks for stronger ones before
    /// they are checked again.
    pub sent_back: u32,
    /// The machine's: research is done, and their text is rewritten next or
    /// a rewrite awaits its check.
    pub repairing: u32,
    /// Waiting for the user.
    pub waiting: u32,
    /// Verified against a source.
    pub verified: u32,
    /// Closed by the user or removed: confirmed, general knowledge, removed.
    pub resolved_by_you: u32,
}

/// One round of checking unsupported claims against their next evidence page.
#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningDeeperEvidence {
    /// The evidence page of this round. Page 1 is each claim's first check.
    pub page: u32,
    /// Claims checked against this page.
    pub claims: u32,
    /// Of those claims, the ones whose check against this page has finished.
    pub checked: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningGenerationJob {
    pub id: String,
    pub program_id: String,
    pub operation_id: String,
    pub kind: LearningGenerationJobKind,
    pub payload_sha256: String,
    pub base_revision_number: u32,
    pub status: LearningGenerationJobStatus,
    pub progress_completed: u32,
    pub progress_total: u32,
    pub progress_message: String,
    pub activity: Option<LearningGenerationActivity>,
    pub result_id: Option<String>,
    pub error: Option<String>,
    pub retry_of_job_id: Option<String>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
}

fn id(value: &str, label: &str) -> Result<()> {
    uuid::Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| AppError::InvalidInput(format!("Invalid {label} ID")))
}

fn text(value: &str, label: &str, max: usize) -> Result<()> {
    let len = value.trim().chars().count();
    if len == 0 || len > max {
        return Err(AppError::InvalidInput(format!(
            "{label} must contain 1–{max} characters."
        )));
    }
    Ok(())
}

fn validate_lesson(lesson: &LearningCurriculumLesson) -> Result<()> {
    id(&lesson.id, "lesson")?;
    text(&lesson.title, "Lesson title", 160)?;
    text(&lesson.objective, "Lesson objective", 1_200)?;
    if !(1..=600).contains(&lesson.estimated_minutes) {
        return Err(AppError::InvalidInput(
            "Lesson time must be between 1 and 600 minutes.".into(),
        ));
    }
    if lesson.state == LearningCurriculumLessonState::Replaced
        && lesson.replacement_lesson_id.is_none()
    {
        return Err(AppError::InvalidInput(
            "A replaced lesson must identify its replacement.".into(),
        ));
    }
    Ok(())
}

pub fn validate_curriculum(revision: &LearningCurriculumRevision) -> Result<()> {
    id(&revision.id, "curriculum revision")?;
    id(&revision.program_id, "program")?;
    if let Some(parent) = revision.parent_revision_id.as_deref() {
        id(parent, "parent curriculum revision")?;
    }
    text(&revision.reason, "Curriculum revision reason", 2_000)?;
    if revision.modules.is_empty() || revision.modules.len() > 24 {
        return Err(AppError::InvalidInput(
            "A curriculum must contain 1–24 modules.".into(),
        ));
    }
    let mut ids = HashSet::new();
    let mut module_ids = HashSet::new();
    for module in &revision.modules {
        id(&module.id, "module")?;
        text(&module.title, "Module title", 160)?;
        text(&module.purpose, "Module purpose", 2_000)?;
        if module.lessons.is_empty() || module.lessons.len() > 64 {
            return Err(AppError::InvalidInput(
                "A module must contain 1–64 lessons.".into(),
            ));
        }
        if !ids.insert(module.id.as_str()) || !module_ids.insert(module.id.as_str()) {
            return Err(AppError::InvalidInput(
                "Curriculum identifiers must be unique.".into(),
            ));
        }
        for outcome in &module.outcome_ids {
            id(outcome, "outcome")?;
            if !ids.insert(outcome.as_str()) {
                return Err(AppError::InvalidInput(
                    "Curriculum identifiers must be unique.".into(),
                ));
            }
        }
        for lesson in &module.lessons {
            validate_lesson(lesson)?;
            if !ids.insert(lesson.id.as_str()) {
                return Err(AppError::InvalidInput(
                    "Curriculum identifiers must be unique.".into(),
                ));
            }
        }
    }
    for module in &revision.modules {
        if module.prerequisite_module_ids.iter().any(|prerequisite| {
            prerequisite == &module.id || !module_ids.contains(prerequisite.as_str())
        }) {
            return Err(AppError::InvalidInput(
                "Module prerequisites must reference another module in this revision.".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
