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
        activity: None,
        result_id: None,
        error: None,
        error_code: None,
        ended_by_update: false,
        lesson_ids: Vec::new(),
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

#[test]
fn a_curriculum_is_checked_for_blank_text_never_for_how_long_it_is() {
    let mut value = revision();
    value.modules[0].title = "Foundations of a durable mental model ".repeat(6);
    value.modules[0].purpose = "Build a durable mental model of the subject. ".repeat(50);
    value.modules[0].lessons[2].title = "Explain and apply the invariant ".repeat(8);
    value.modules[0].lessons[2].objective =
        "Explain and apply the invariant to a new case. ".repeat(30);
    let checked = validate_curriculum(&value);
    assert!(checked.is_ok(), "{checked:?}");
}

#[test]
fn a_lesson_is_edited_at_any_length() -> Result<()> {
    let base = revision();
    let title = "Explain and apply the invariant "
        .repeat(8)
        .trim()
        .to_owned();
    let objective = "Explain and apply the invariant to a new case. "
        .repeat(30)
        .trim()
        .to_owned();
    let request = LearningCurriculumRevisionRequest {
        revision_id: id(20),
        expected_revision_number: 3,
        reason: "Say more about the last lesson".into(),
        created_at: 20,
        operations: vec![LearningCurriculumOperation::EditLesson {
            lesson_id: id(12),
            title: title.clone(),
            objective: objective.clone(),
            estimated_minutes: 25,
        }],
    };
    let result = apply_curriculum_revision(&base, Some(&id(11)), request)?;
    assert_eq!(result.revision.modules[0].lessons[2].title, title);
    assert_eq!(result.revision.modules[0].lessons[2].objective, objective);
    Ok(())
}

#[test]
fn a_failure_is_recorded_whatever_its_message_says() -> Result<()> {
    let mut value = job(LearningGenerationJobStatus::Pending);
    start_generation_job(&mut value, 20)?;
    let message = "The model's answer could not be read: ".repeat(80);
    fail_generation_job(&mut value, message.clone(), 30)?;
    assert_eq!(value.status, LearningGenerationJobStatus::Failed);
    assert_eq!(value.error.as_deref(), Some(message.as_str()));
    Ok(())
}

#[test]
fn a_job_carries_its_error_code_as_error_code_and_reads_null_without_one() {
    use lattice_core::shared::ipc::ErrorCode;
    let mut value = job(LearningGenerationJobStatus::Pending);
    value.error = Some("Connection interrupted".into());
    value.error_code = Some(ErrorCode::NetworkError);
    let wire = serde_json::to_value(&value).unwrap();
    assert_eq!(wire["errorCode"], "NETWORK_ERROR");

    let mut stored = wire;
    stored.as_object_mut().unwrap().remove("errorCode");
    let read: LearningGenerationJob = serde_json::from_value(stored).unwrap();
    assert_eq!(read.error_code, None);
}
