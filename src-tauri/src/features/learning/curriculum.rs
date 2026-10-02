//! Immutable curriculum revision and durable generation-job rules.
//!
//! Persistence is added by the program-editing slice; this module owns the
//! deterministic state transition rules so a renderer or model cannot rewrite
//! completed work, started assessments, or an accepted plan in place.

use crate::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

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

fn immutable(lesson: &LearningCurriculumLesson) -> bool {
    lesson.state == LearningCurriculumLessonState::Completed || lesson.assessment_started
}

fn required_count(modules: &[LearningCurriculumModule]) -> usize {
    modules
        .iter()
        .flat_map(|module| &module.lessons)
        .filter(|lesson| {
            !matches!(
                lesson.state,
                LearningCurriculumLessonState::Skipped
                    | LearningCurriculumLessonState::Replaced
                    | LearningCurriculumLessonState::Challenged
            )
        })
        .count()
}

fn resume_lesson(modules: &[LearningCurriculumModule], previous: Option<&str>) -> Option<String> {
    let eligible = |lesson: &&LearningCurriculumLesson| {
        matches!(
            lesson.state,
            LearningCurriculumLessonState::Outline | LearningCurriculumLessonState::Ready
        )
    };
    let lessons = modules
        .iter()
        .flat_map(|module| &module.lessons)
        .collect::<Vec<_>>();
    previous
        .and_then(|id| {
            lessons
                .iter()
                .copied()
                .find(|lesson| lesson.id == id && eligible(lesson))
        })
        .or_else(|| lessons.iter().copied().find(eligible))
        .map(|lesson| lesson.id.clone())
}

fn lesson_location(
    modules: &[LearningCurriculumModule],
    lesson_id: &str,
) -> Option<(usize, usize)> {
    modules
        .iter()
        .enumerate()
        .find_map(|(module_index, module)| {
            module
                .lessons
                .iter()
                .position(|lesson| lesson.id == lesson_id)
                .map(|lesson_index| (module_index, lesson_index))
        })
}

fn lesson_at(
    modules: &[LearningCurriculumModule],
    (module_index, lesson_index): (usize, usize),
) -> Result<&LearningCurriculumLesson> {
    modules
        .get(module_index)
        .and_then(|module| module.lessons.get(lesson_index))
        .ok_or_else(|| AppError::InvalidState("Curriculum lesson location became invalid.".into()))
}

fn lesson_at_mut(
    modules: &mut [LearningCurriculumModule],
    (module_index, lesson_index): (usize, usize),
) -> Result<&mut LearningCurriculumLesson> {
    modules
        .get_mut(module_index)
        .and_then(|module| module.lessons.get_mut(lesson_index))
        .ok_or_else(|| AppError::InvalidState("Curriculum lesson location became invalid.".into()))
}

fn module_id_at(modules: &[LearningCurriculumModule], module_index: usize) -> Result<String> {
    modules
        .get(module_index)
        .map(|module| module.id.clone())
        .ok_or_else(|| AppError::InvalidState("Curriculum module location became invalid.".into()))
}

fn insertion_index(
    lessons: &[LearningCurriculumLesson],
    after_lesson_id: Option<&str>,
) -> Result<usize> {
    match after_lesson_id {
        Some(id) => lessons
            .iter()
            .position(|lesson| lesson.id == id)
            .map(|index| index + 1)
            .ok_or_else(|| {
                AppError::InvalidInput("The requested insertion anchor was not found.".into())
            }),
        None => Ok(0),
    }
}

pub fn apply_curriculum_revision(
    base: &LearningCurriculumRevision,
    current_lesson_id: Option<&str>,
    request: LearningCurriculumRevisionRequest,
) -> Result<LearningCurriculumRevisionResult> {
    validate_curriculum(base)?;
    id(&request.revision_id, "curriculum revision")?;
    if request.expected_revision_number != base.revision_number {
        return Err(AppError::InvalidState(
            "The accepted curriculum changed. Reload before applying this revision.".into(),
        ));
    }
    if request.revision_id == base.id {
        return Err(AppError::InvalidInput(
            "A curriculum revision needs a new identifier.".into(),
        ));
    }
    text(&request.reason, "Curriculum revision reason", 2_000)?;
    if request.operations.is_empty() || request.operations.len() > 64 {
        return Err(AppError::InvalidInput(
            "A curriculum revision must contain 1–64 changes.".into(),
        ));
    }
    let before = required_count(&base.modules);
    let mut modules = base.modules.clone();
    let mut changes = Vec::with_capacity(request.operations.len());
    for operation in request.operations {
        match operation {
            LearningCurriculumOperation::AddLesson {
                module_id,
                after_lesson_id,
                lesson,
            } => {
                validate_lesson(&lesson)?;
                if lesson.state != LearningCurriculumLessonState::Outline
                    || lesson.assessment_started
                    || lesson.replacement_lesson_id.is_some()
                    || lesson_location(&modules, &lesson.id).is_some()
                {
                    return Err(AppError::InvalidInput(
                        "A newly added lesson must be a new untouched outline.".into(),
                    ));
                }
                let module = modules
                    .iter_mut()
                    .find(|module| module.id == module_id)
                    .ok_or_else(|| AppError::NotFound("Target module was not found.".into()))?;
                let index = insertion_index(&module.lessons, after_lesson_id.as_deref())?;
                let lesson_id = lesson.id.clone();
                let title = lesson.title.clone();
                let added = lesson.clone();
                module.lessons.insert(index, lesson);
                changes.push(LearningCurriculumChange {
                    kind: LearningCurriculumChangeKind::Added,
                    lesson_id,
                    from_module_id: None,
                    to_module_id: Some(module_id),
                    before: None,
                    after: Some(added),
                    description: format!("Added lesson “{title}”."),
                });
            }
            LearningCurriculumOperation::EditLesson {
                lesson_id,
                title,
                objective,
                estimated_minutes,
            } => {
                text(&title, "Lesson title", 160)?;
                text(&objective, "Lesson objective", 1_200)?;
                if !(1..=600).contains(&estimated_minutes) {
                    return Err(AppError::InvalidInput(
                        "Lesson time must be between 1 and 600 minutes.".into(),
                    ));
                }
                let (module_index, lesson_index) = lesson_location(&modules, &lesson_id)
                    .ok_or_else(|| AppError::NotFound("Lesson was not found.".into()))?;
                let module_id = module_id_at(&modules, module_index)?;
                let lesson = lesson_at_mut(&mut modules, (module_index, lesson_index))?;
                if immutable(lesson) {
                    return Err(AppError::InvalidState(
                        "Completed lessons and lessons with a started assessment cannot change."
                            .into(),
                    ));
                }
                let before = lesson.clone();
                lesson.title = title;
                lesson.objective = objective;
                lesson.estimated_minutes = estimated_minutes;
                let after = lesson.clone();
                changes.push(LearningCurriculumChange {
                    kind: LearningCurriculumChangeKind::Edited,
                    lesson_id,
                    from_module_id: Some(module_id.clone()),
                    to_module_id: Some(module_id),
                    before: Some(before),
                    after: Some(after),
                    description: "Updated the lesson title, objective, or session estimate.".into(),
                });
            }
            LearningCurriculumOperation::MoveLesson {
                lesson_id,
                target_module_id,
                after_lesson_id,
            } => {
                let (from_module, lesson_index) = lesson_location(&modules, &lesson_id)
                    .ok_or_else(|| AppError::NotFound("Lesson was not found.".into()))?;
                if immutable(lesson_at(&modules, (from_module, lesson_index))?) {
                    return Err(AppError::InvalidState(
                        "Completed lessons and lessons with a started assessment cannot move."
                            .into(),
                    ));
                }
                let from_id = module_id_at(&modules, from_module)?;
                let lesson = modules
                    .get_mut(from_module)
                    .and_then(|module| {
                        (lesson_index < module.lessons.len())
                            .then(|| module.lessons.remove(lesson_index))
                    })
                    .ok_or_else(|| {
                        AppError::InvalidState("Curriculum lesson location became invalid.".into())
                    })?;
                let before = lesson.clone();
                let target_index = modules
                    .iter()
                    .position(|module| module.id == target_module_id)
                    .ok_or_else(|| AppError::NotFound("Target module was not found.".into()))?;
                let target = modules.get_mut(target_index).ok_or_else(|| {
                    AppError::InvalidState("Curriculum module location became invalid.".into())
                })?;
                let index = insertion_index(&target.lessons, after_lesson_id.as_deref())?;
                target.lessons.insert(index, lesson);
                changes.push(LearningCurriculumChange {
                    kind: LearningCurriculumChangeKind::Moved,
                    lesson_id,
                    from_module_id: Some(from_id),
                    to_module_id: Some(target_module_id),
                    before: Some(before.clone()),
                    after: Some(before),
                    description: "Moved the lesson without changing its historical identity."
                        .into(),
                });
            }
            LearningCurriculumOperation::SkipLesson { lesson_id } => {
                let (module_index, lesson_index) = lesson_location(&modules, &lesson_id)
                    .ok_or_else(|| AppError::NotFound("Lesson was not found.".into()))?;
                let module_id = module_id_at(&modules, module_index)?;
                let lesson = lesson_at_mut(&mut modules, (module_index, lesson_index))?;
                if immutable(lesson) {
                    return Err(AppError::InvalidState(
                        "Completed lessons and lessons with a started assessment cannot be skipped."
                            .into(),
                    ));
                }
                let before = lesson.clone();
                lesson.state = LearningCurriculumLessonState::Skipped;
                let after = lesson.clone();
                changes.push(LearningCurriculumChange {
                    kind: LearningCurriculumChangeKind::Skipped,
                    lesson_id,
                    from_module_id: Some(module_id),
                    to_module_id: None,
                    before: Some(before),
                    after: Some(after),
                    description: "Skipped this lesson in the new accepted plan.".into(),
                });
            }
            LearningCurriculumOperation::ReplaceLesson {
                lesson_id,
                replacement,
            } => {
                validate_lesson(&replacement)?;
                if replacement.state != LearningCurriculumLessonState::Outline
                    || replacement.assessment_started
                    || replacement.replacement_lesson_id.is_some()
                    || lesson_location(&modules, &replacement.id).is_some()
                {
                    return Err(AppError::InvalidInput(
                        "A replacement must be a new untouched lesson outline.".into(),
                    ));
                }
                let (module_index, lesson_index) = lesson_location(&modules, &lesson_id)
                    .ok_or_else(|| AppError::NotFound("Lesson was not found.".into()))?;
                if immutable(lesson_at(&modules, (module_index, lesson_index))?) {
                    return Err(AppError::InvalidState(
                        "Completed lessons and lessons with a started assessment cannot be replaced."
                            .into(),
                    ));
                }
                let module_id = module_id_at(&modules, module_index)?;
                let before = lesson_at(&modules, (module_index, lesson_index))?.clone();
                let replacement_id = replacement.id.clone();
                let after = replacement.clone();
                let module = modules.get_mut(module_index).ok_or_else(|| {
                    AppError::InvalidState("Curriculum module location became invalid.".into())
                })?;
                let lesson = module.lessons.get_mut(lesson_index).ok_or_else(|| {
                    AppError::InvalidState("Curriculum lesson location became invalid.".into())
                })?;
                lesson.state = LearningCurriculumLessonState::Replaced;
                lesson.replacement_lesson_id = Some(replacement_id.clone());
                module.lessons.insert(lesson_index + 1, replacement);
                changes.push(LearningCurriculumChange {
                    kind: LearningCurriculumChangeKind::Replaced,
                    lesson_id,
                    from_module_id: Some(module_id.clone()),
                    to_module_id: Some(module_id),
                    before: Some(before),
                    after: Some(after),
                    description: format!("Replaced this lesson with {replacement_id}."),
                });
            }
            LearningCurriculumOperation::ChallengePrerequisite { lesson_id } => {
                let (module_index, lesson_index) = lesson_location(&modules, &lesson_id)
                    .ok_or_else(|| AppError::NotFound("Lesson was not found.".into()))?;
                let module_id = module_id_at(&modules, module_index)?;
                let lesson = lesson_at_mut(&mut modules, (module_index, lesson_index))?;
                if immutable(lesson) {
                    return Err(AppError::InvalidState(
                        "Completed lessons and lessons with a started assessment cannot be challenged."
                            .into(),
                    ));
                }
                let before = lesson.clone();
                lesson.state = LearningCurriculumLessonState::Challenged;
                let after = lesson.clone();
                changes.push(LearningCurriculumChange {
                    kind: LearningCurriculumChangeKind::Challenged,
                    lesson_id,
                    from_module_id: Some(module_id),
                    to_module_id: None,
                    before: Some(before),
                    after: Some(after),
                    description: "Recorded a prerequisite challenge in the new plan.".into(),
                });
            }
        }
    }
    // Protect order as well as fields. Moving an upcoming lesson around a
    // completed or already-assessed lesson must not silently displace that
    // protected lesson in the accepted sequence.
    for (base_module_index, base_module) in base.modules.iter().enumerate() {
        for (base_lesson_index, base_lesson) in base_module.lessons.iter().enumerate() {
            if !immutable(base_lesson) {
                continue;
            }
            let Some((module_index, lesson_index)) = lesson_location(&modules, &base_lesson.id)
            else {
                return Err(AppError::InvalidState(
                    "Completed lessons and lessons with a started assessment cannot be removed."
                        .into(),
                ));
            };
            if module_index != base_module_index || lesson_index != base_lesson_index {
                return Err(AppError::InvalidState(
                    "Upcoming changes cannot move or displace completed or assessed lessons."
                        .into(),
                ));
            }
        }
    }
    let revision = LearningCurriculumRevision {
        id: request.revision_id,
        program_id: base.program_id.clone(),
        revision_number: base.revision_number.saturating_add(1),
        parent_revision_id: Some(base.id.clone()),
        reason: request.reason.trim().into(),
        modules,
        created_at: request.created_at,
    };
    validate_curriculum(&revision)?;
    let after = required_count(&revision.modules);
    Ok(LearningCurriculumRevisionResult {
        resume_lesson_id: resume_lesson(&revision.modules, current_lesson_id),
        revision,
        changes,
        required_lesson_count_before: before,
        required_lesson_count_after: after,
    })
}

pub fn validate_generation_job(job: &LearningGenerationJob) -> Result<()> {
    id(&job.id, "generation job")?;
    id(&job.program_id, "program")?;
    id(&job.operation_id, "generation operation")?;
    if job.payload_sha256.len() != 64
        || !job
            .payload_sha256
            .bytes()
            .all(|value| value.is_ascii_hexdigit())
    {
        return Err(AppError::InvalidInput(
            "Generation jobs require a SHA-256 payload digest.".into(),
        ));
    }
    if job.progress_total == 0 || job.progress_completed > job.progress_total {
        return Err(AppError::InvalidInput(
            "Generation-job progress is invalid.".into(),
        ));
    }
    let finished = matches!(
        job.status,
        LearningGenerationJobStatus::Completed
            | LearningGenerationJobStatus::Failed
            | LearningGenerationJobStatus::Cancelled
            | LearningGenerationJobStatus::Interrupted
    );
    if finished != job.finished_at.is_some() {
        return Err(AppError::InvalidInput(
            "Generation-job completion time does not match its status.".into(),
        ));
    }
    if (job.status == LearningGenerationJobStatus::Completed) != job.result_id.is_some() {
        return Err(AppError::InvalidInput(
            "Only completed generation jobs may publish a result.".into(),
        ));
    }
    if job.status == LearningGenerationJobStatus::Completed
        && job.progress_completed != job.progress_total
    {
        return Err(AppError::InvalidInput(
            "Completed generation jobs must report complete progress.".into(),
        ));
    }
    let requires_error = matches!(
        job.status,
        LearningGenerationJobStatus::Failed | LearningGenerationJobStatus::Interrupted
    );
    if requires_error != job.error.is_some() {
        return Err(AppError::InvalidInput(
            "Only failed or interrupted generation jobs may preserve an error.".into(),
        ));
    }
    if let Some(error) = &job.error {
        text(error, "Generation error", 2_000)?;
    }
    Ok(())
}

pub fn start_generation_job(job: &mut LearningGenerationJob, now: i64) -> Result<()> {
    if !matches!(
        job.status,
        LearningGenerationJobStatus::Pending | LearningGenerationJobStatus::Interrupted
    ) {
        return Err(AppError::InvalidState(
            "Only pending or interrupted generation jobs can start.".into(),
        ));
    }
    job.status = LearningGenerationJobStatus::Running;
    job.started_at = Some(now);
    job.finished_at = None;
    job.error = None;
    job.result_id = None;
    validate_generation_job(job)
}

pub fn advance_generation_job(job: &mut LearningGenerationJob, completed: u32) -> Result<()> {
    if job.status != LearningGenerationJobStatus::Running || completed < job.progress_completed {
        return Err(AppError::InvalidState(
            "Only a running job can advance, and progress cannot move backward.".into(),
        ));
    }
    job.progress_completed = completed.min(job.progress_total);
    validate_generation_job(job)
}

pub fn finish_generation_job(
    job: &mut LearningGenerationJob,
    result_id: String,
    now: i64,
) -> Result<()> {
    if job.status != LearningGenerationJobStatus::Running {
        return Err(AppError::InvalidState(
            "Only a running generation job can publish a result.".into(),
        ));
    }
    id(&result_id, "generation result")?;
    job.status = LearningGenerationJobStatus::Completed;
    job.progress_completed = job.progress_total;
    job.result_id = Some(result_id);
    job.finished_at = Some(now);
    job.error = None;
    validate_generation_job(job)
}

pub fn fail_generation_job(job: &mut LearningGenerationJob, error: String, now: i64) -> Result<()> {
    if job.status != LearningGenerationJobStatus::Running {
        return Err(AppError::InvalidState(
            "Only a running generation job can fail.".into(),
        ));
    }
    text(&error, "Generation error", 2_000)?;
    job.status = LearningGenerationJobStatus::Failed;
    job.error = Some(error);
    job.result_id = None;
    job.finished_at = Some(now);
    validate_generation_job(job)
}

pub fn cancel_generation_job(job: &mut LearningGenerationJob, now: i64) -> Result<()> {
    if !matches!(
        job.status,
        LearningGenerationJobStatus::Pending
            | LearningGenerationJobStatus::Running
            | LearningGenerationJobStatus::Interrupted
    ) {
        return Err(AppError::InvalidState(
            "This generation job has already finished.".into(),
        ));
    }
    job.status = LearningGenerationJobStatus::Cancelled;
    job.result_id = None;
    job.error = None;
    job.finished_at = Some(now);
    validate_generation_job(job)
}

pub fn recover_interrupted_jobs(jobs: &mut [LearningGenerationJob]) {
    for job in jobs {
        if job.status == LearningGenerationJobStatus::Running {
            job.status = LearningGenerationJobStatus::Interrupted;
            job.result_id = None;
            job.error = Some("Generation stopped before publishing a result.".into());
            job.finished_at = Some(chrono::Utc::now().timestamp_millis());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(seed: u128) -> String {
        uuid::Uuid::from_u128(seed).to_string()
    }

    fn lesson(seed: u128, state: LearningCurriculumLessonState) -> LearningCurriculumLesson {
        LearningCurriculumLesson {
            id: id(seed),
            title: format!("Lesson {seed}"),
            objective: "Explain and apply the invariant.".into(),
            estimated_minutes: 25,
            state,
            assessment_started: false,
            replacement_lesson_id: None,
        }
    }

    fn revision() -> LearningCurriculumRevision {
        LearningCurriculumRevision {
            id: id(1),
            program_id: id(2),
            revision_number: 3,
            parent_revision_id: Some(id(3)),
            reason: "Accepted plan".into(),
            modules: vec![LearningCurriculumModule {
                id: id(4),
                title: "Foundations".into(),
                purpose: "Build a durable mental model.".into(),
                prerequisite_module_ids: vec![],
                outcome_ids: vec![id(5)],
                lessons: vec![
                    lesson(10, LearningCurriculumLessonState::Completed),
                    lesson(11, LearningCurriculumLessonState::Ready),
                    lesson(12, LearningCurriculumLessonState::Outline),
                ],
            }],
            created_at: 10,
        }
    }

    #[test]
    fn completed_and_started_work_cannot_change_under_history() {
        let base = revision();
        let request = LearningCurriculumRevisionRequest {
            revision_id: id(20),
            expected_revision_number: 3,
            reason: "Shorten the program".into(),
            created_at: 20,
            operations: vec![LearningCurriculumOperation::SkipLesson { lesson_id: id(10) }],
        };
        assert!(apply_curriculum_revision(&base, Some(&id(11)), request).is_err());
        assert_eq!(
            base.modules[0].lessons[0].state,
            LearningCurriculumLessonState::Completed
        );
    }

    #[test]
    fn accepted_revision_reports_denominator_change_and_preserves_cursor() -> Result<()> {
        let base = revision();
        let request = LearningCurriculumRevisionRequest {
            revision_id: id(20),
            expected_revision_number: 3,
            reason: "Replace the final outline with a transfer lesson.".into(),
            created_at: 20,
            operations: vec![LearningCurriculumOperation::ReplaceLesson {
                lesson_id: id(12),
                replacement: lesson(13, LearningCurriculumLessonState::Outline),
            }],
        };
        let result = apply_curriculum_revision(&base, Some(&id(11)), request)?;
        assert_eq!(result.revision.revision_number, 4);
        assert_eq!(result.resume_lesson_id, Some(id(11)));
        assert_eq!(result.required_lesson_count_before, 3);
        assert_eq!(result.required_lesson_count_after, 3);
        assert_eq!(
            result.changes[0].kind,
            LearningCurriculumChangeKind::Replaced
        );
        assert_eq!(
            base.modules[0].lessons.len(),
            3,
            "base revision is immutable"
        );
        Ok(())
    }

    fn job(status: LearningGenerationJobStatus) -> LearningGenerationJob {
        LearningGenerationJob {
            id: id(30),
            program_id: id(2),
            operation_id: id(32),
            kind: LearningGenerationJobKind::LessonPreparation,
            payload_sha256: "a".repeat(64),
            base_revision_number: 3,
            status,
            progress_completed: 0,
            progress_total: 4,
            progress_message: "Queued".into(),
            result_id: None,
            error: None,
            retry_of_job_id: None,
            created_at: 10,
            started_at: None,
            finished_at: None,
        }
    }

    #[test]
    fn generation_jobs_publish_only_after_atomic_completion() -> Result<()> {
        let mut value = job(LearningGenerationJobStatus::Pending);
        start_generation_job(&mut value, 20)?;
        advance_generation_job(&mut value, 2)?;
        assert!(value.result_id.is_none());
        finish_generation_job(&mut value, id(31), 30)?;
        assert_eq!(value.status, LearningGenerationJobStatus::Completed);
        assert_eq!(value.progress_completed, 4);
        assert!(cancel_generation_job(&mut value, 40).is_err());
        Ok(())
    }

    #[test]
    fn running_jobs_recover_as_interrupted_without_a_partial_result() {
        let mut jobs = [job(LearningGenerationJobStatus::Running)];
        jobs[0].result_id = Some(id(31));
        recover_interrupted_jobs(&mut jobs);
        assert_eq!(jobs[0].status, LearningGenerationJobStatus::Interrupted);
        assert!(jobs[0].result_id.is_none());
        assert!(jobs[0].finished_at.is_some());
    }
}
