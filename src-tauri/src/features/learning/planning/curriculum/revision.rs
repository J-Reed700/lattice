//! Applying a curriculum revision: each operation on a copy of the accepted
//! plan, with completed and assessed lessons kept in place.

use super::{
    id, text, validate_curriculum, validate_lesson, LearningCurriculumChange,
    LearningCurriculumChangeKind, LearningCurriculumLesson, LearningCurriculumLessonState,
    LearningCurriculumModule, LearningCurriculumOperation, LearningCurriculumRevision,
    LearningCurriculumRevisionRequest, LearningCurriculumRevisionResult,
};
use lattice_core::shared::error::{AppError, Result};

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
