//! Tests for plan revisions, diagnostics and lesson jobs.
use super::preparation::lesson_checkpoint_key;
use super::*;
use crate::{
    curriculum::{
        LearningCurriculumLesson, LearningCurriculumLessonState, LearningCurriculumOperation,
        LearningGenerationJobKind, LearningGenerationJobStatus,
    },
    dto::{AcceptLearningProgramRequestDto, LearningProgramDto},
    repository::LearningRepository,
    tests::{fixture, pool},
};
use lattice_core::shared::runtime::jobs::RecoveryPolicy;

async fn active_program(pool: &SqlitePool) -> Result<LearningProgramDto> {
    let repo = LearningRepository::new(pool.clone());
    let program = fixture();
    repo.create(&program).await?;
    repo.accept(&AcceptLearningProgramRequestDto {
        program_id: program.summary.id.clone(),
        expected_revision: 0,
        title: program.summary.title.clone(),
    })
    .await?;
    repo.get(&program.summary.id).await
}

fn op_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn jobs(pool: &SqlitePool) -> JobStore {
    JobStore::new(pool.clone())
}

fn new_lesson() -> LearningCurriculumLesson {
    LearningCurriculumLesson {
        id: uuid::Uuid::new_v4().to_string(),
        title: "A new neutral lesson".into(),
        objective: "Explain one relevant idea".into(),
        estimated_minutes: 20,
        state: LearningCurriculumLessonState::Outline,
        assessment_started: false,
        replacement_lesson_id: None,
    }
}

#[tokio::test]
async fn real_migration_seeds_uuid_outcomes_and_preview_is_replayable_and_cas_guarded() -> Result<()>
{
    let pool = pool().await?;
    let program = active_program(&pool).await?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    let plan = repo.plan(&program.summary.id).await?;
    let accepted = plan.accepted_revision.expect("seed snapshot");
    let outcome_id = accepted.modules[0].outcome_ids[0].clone();
    assert!(uuid::Uuid::parse_str(&outcome_id).is_ok());
    let exists: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM learning_outcome_definitions WHERE id=? AND program_id=?",
    )
    .bind(&outcome_id)
    .bind(&program.summary.id)
    .fetch_one(&pool)
    .await
    .map_err(db)?;
    assert_eq!(exists, 1);

    let added = new_lesson();
    let request = PreviewLearningCurriculumRevisionRequestDto {
        operation_id: op_id(),
        program_id: program.summary.id.clone(),
        expected_revision: program.summary.revision,
        reason: "Adjust upcoming practice".into(),
        operations: vec![LearningCurriculumOperation::AddLesson {
            module_id: accepted.modules[0].id.clone(),
            after_lesson_id: None,
            lesson: added.clone(),
        }],
    };
    let preview = repo.preview(&request).await?;
    let draft = preview.draft_revision.expect("draft snapshot");
    assert_eq!(
        preview.required_lesson_count_after,
        preview.required_lesson_count_before + 1
    );
    assert!(draft.modules[0]
        .lessons
        .iter()
        .any(|lesson| lesson.id == added.id));
    let replay = repo.preview(&request).await?;
    assert_eq!(replay.draft_revision.unwrap().id, draft.id);

    let mut changed_payload = request.clone();
    changed_payload.reason.push_str(" changed");
    assert!(repo.preview(&changed_payload).await.is_err());
    let mut stale = request;
    stale.operation_id = op_id();
    stale.expected_revision -= 1;
    crate::tests::assert_conflict(repo.preview(&stale).await);

    let accept = LearningCurriculumRevisionActionRequestDto {
        operation_id: op_id(),
        program_id: program.summary.id.clone(),
        revision_id: draft.id.clone(),
        expected_revision: program.summary.revision,
    };
    let accepted_plan = repo.accept_revision(&accept).await?;
    assert_eq!(accepted_plan.accepted_revision.unwrap().id, draft.id);
    crate::tests::assert_conflict(
        repo.accept_revision(&LearningCurriculumRevisionActionRequestDto {
            operation_id: op_id(),
            ..accept.clone()
        })
        .await,
    );
    let revision_after_accept: i64 =
        sqlx::query_scalar("SELECT revision FROM learning_programs WHERE id=?")
            .bind(&program.summary.id)
            .fetch_one(&pool)
            .await
            .map_err(db)?;
    assert_eq!(revision_after_accept, program.summary.revision + 1);
    assert_eq!(
        repo.accept_revision(&accept)
            .await?
            .accepted_revision
            .unwrap()
            .id,
        draft.id
    );
    let revision_after_replay: i64 =
        sqlx::query_scalar("SELECT revision FROM learning_programs WHERE id=?")
            .bind(&program.summary.id)
            .fetch_one(&pool)
            .await
            .map_err(db)?;
    assert_eq!(revision_after_replay, revision_after_accept);
    let stored_snapshot: String =
        sqlx::query_scalar("SELECT snapshot_json FROM learning_curriculum_revisions WHERE id=?")
            .bind(&draft.id)
            .fetch_one(&pool)
            .await
            .map_err(db)?;
    assert_eq!(
        serde_json::from_str::<LearningCurriculumRevision>(&stored_snapshot)
            .map_err(|e| AppError::Serialization(e.to_string()))?,
        draft
    );
    let accepted_at: Option<i64> =
        sqlx::query_scalar("SELECT accepted_at FROM learning_curriculum_revisions WHERE id=?")
            .bind(&draft.id)
            .fetch_one(&pool)
            .await
            .map_err(db)?;
    assert!(
        accepted_at.is_some(),
        "acceptance time survives revision history changes"
    );
    Ok(())
}

#[tokio::test]
async fn diagnostic_lifecycle_is_idempotent_complete_and_does_not_claim_mastery() -> Result<()> {
    let pool = pool().await?;
    let program = active_program(&pool).await?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    let start = StartLearningDiagnosticRequestDto {
        operation_id: op_id(),
        program_id: program.summary.id.clone(),
        expected_revision: program.summary.revision,
    };
    let active = repo.start_diagnostic(&start).await?;
    assert_eq!(active.status, LearningDiagnosticStatus::Active);
    assert!(active
        .prompts
        .iter()
        .all(|prompt| prompt.prompt.starts_with("Self-inventory:")));
    for prompt in &active.prompts {
        let durable: i64 = sqlx::query_scalar("SELECT count(*) FROM learning_outcome_definitions WHERE program_id=? AND id=? AND title=?")
            .bind(&program.summary.id).bind(&prompt.outcome_id).bind(&prompt.outcome_title)
            .fetch_one(&pool).await.map_err(db)?;
        assert_eq!(
            durable, 1,
            "diagnostic outcome ID and title must come from saved definitions"
        );
    }
    assert_eq!(repo.start_diagnostic(&start).await?.id, active.id);
    let mut changed_start = start.clone();
    changed_start.expected_revision += 1;
    assert!(repo.start_diagnostic(&changed_start).await.is_err());

    let mut incomplete = SubmitLearningDiagnosticRequestDto {
        operation_id: op_id(),
        program_id: program.summary.id.clone(),
        diagnostic_id: active.id.clone(),
        expected_revision: program.summary.revision,
        responses: vec![],
        save_only: false,
        expected_diagnostic_revision: None,
    };
    assert!(repo.submit_diagnostic(&incomplete).await.is_err());
    incomplete.responses = active
        .prompts
        .iter()
        .map(|prompt| LearningDiagnosticResponseDto {
            prompt_id: prompt.id.clone(),
            response: "I have some familiarity and a question to explore.".into(),
        })
        .collect();
    let submitted = repo.submit_diagnostic(&incomplete).await?;
    assert_eq!(submitted.status, LearningDiagnosticStatus::Submitted);
    assert_eq!(submitted.source_coverage_gaps.len(), active.prompts.len());
    assert!(submitted
        .interpretation
        .contains("do not establish mastery"));
    assert_eq!(repo.submit_diagnostic(&incomplete).await?.id, active.id);
    let mut changed_answer = incomplete.clone();
    changed_answer.responses[0].response.push_str(" changed");
    assert!(repo.submit_diagnostic(&changed_answer).await.is_err());

    let skipped = repo
        .skip_diagnostic(&SkipLearningDiagnosticRequestDto {
            operation_id: op_id(),
            program_id: program.summary.id.clone(),
            expected_revision: program.summary.revision,
        })
        .await?;
    assert_eq!(skipped.status, LearningDiagnosticStatus::Skipped);
    Ok(())
}

#[tokio::test]
async fn a_course_that_changed_since_it_was_loaded_is_reported_as_a_conflict() -> Result<()> {
    let pool = pool().await?;
    let program = active_program(&pool).await?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    let stale = program.summary.revision + 1;
    crate::tests::assert_conflict(
        repo.discard_revision(&DiscardLearningCurriculumRevisionRequestDto {
            operation_id: op_id(),
            program_id: program.summary.id.clone(),
            revision_id: op_id(),
            expected_revision: stale,
        })
        .await,
    );
    let target = program.modules[1].lessons[1].id.clone();
    crate::tests::assert_conflict(
        repo.start_job(&StartLearningGenerationJobRequestDto {
            operation_id: op_id(),
            program_id: program.summary.id.clone(),
            expected_revision: stale,
            kind: LearningGenerationJobKind::LessonPreparation,
            request_json: serde_json::json!({"lessonIds":[target]}).to_string(),
            progress_total: 1,
        })
        .await,
    );
    crate::tests::assert_conflict(
        repo.cancel_job(&LearningGenerationJobActionRequestDto {
            operation_id: op_id(),
            program_id: program.summary.id.clone(),
            job_id: op_id(),
            expected_revision: stale,
        })
        .await,
    );
    Ok(())
}

#[tokio::test]
async fn a_starting_point_that_changed_since_it_was_loaded_is_reported_as_a_conflict() -> Result<()>
{
    let pool = pool().await?;
    let program = active_program(&pool).await?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    let stale = program.summary.revision + 1;
    crate::tests::assert_conflict(
        repo.start_diagnostic(&StartLearningDiagnosticRequestDto {
            operation_id: op_id(),
            program_id: program.summary.id.clone(),
            expected_revision: stale,
        })
        .await,
    );
    crate::tests::assert_conflict(
        repo.skip_diagnostic(&SkipLearningDiagnosticRequestDto {
            operation_id: op_id(),
            program_id: program.summary.id.clone(),
            expected_revision: stale,
        })
        .await,
    );
    let active = repo
        .start_diagnostic(&StartLearningDiagnosticRequestDto {
            operation_id: op_id(),
            program_id: program.summary.id.clone(),
            expected_revision: program.summary.revision,
        })
        .await?;
    let submit = |expected_revision: i64, expected_diagnostic_revision: Option<i64>| {
        SubmitLearningDiagnosticRequestDto {
            operation_id: op_id(),
            program_id: program.summary.id.clone(),
            diagnostic_id: active.id.clone(),
            expected_revision,
            responses: vec![],
            save_only: true,
            expected_diagnostic_revision,
        }
    };
    crate::tests::assert_conflict(repo.submit_diagnostic(&submit(stale, None)).await);
    crate::tests::assert_conflict(
        repo.submit_diagnostic(&submit(program.summary.revision, Some(99)))
            .await,
    );
    Ok(())
}

#[tokio::test]
async fn completed_and_assessed_lessons_are_protected_by_current_outcome_joins() -> Result<()> {
    let pool = pool().await?;
    let program = active_program(&pool).await?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    let accepted = repo
        .plan(&program.summary.id)
        .await?
        .accepted_revision
        .expect("accepted snapshot");
    let completed_lesson = accepted.modules[1].lessons[0].id.clone();
    sqlx::query("UPDATE learning_lessons SET completed=1 WHERE id=?")
        .bind(&completed_lesson)
        .execute(&pool)
        .await
        .map_err(db)?;

    let module = &accepted.modules[0];
    let assessed_lesson = module.lessons[0].id.clone();
    let outcome_id = module.outcome_ids[0].clone();
    let blueprint_id = uuid::Uuid::new_v4().to_string();
    let candidate_id = uuid::Uuid::new_v4().to_string();
    let form_id = uuid::Uuid::new_v4().to_string();
    let stamp = now();
    sqlx::query("INSERT INTO learning_assessment_blueprints(id,program_id,revision,predecessor_revision,purpose,title,instructions,expected_minutes,allowed_aids_json,passing_score,rubric_json,feedback_timing,source_version_ids_json,status,change_reason,created_at) VALUES(?,?,1,NULL,'practice','A neutral assessment','Respond to the prompts.',20,'[]',0.6,'[]','immediate','[]','accepted','Initial',?)")
        .bind(&blueprint_id).bind(&program.summary.id).bind(stamp).execute(&pool).await.map_err(db)?;
    sqlx::query("INSERT INTO learning_assessment_candidates(id,program_id,outcome_id,format,difficulty,prompt,options_json,artifact_kind,rubric_json,source_version_ids_json,authored_by,author_model,content_sha256,created_at) VALUES(?,?,?,'single_choice',1,'A neutral prompt','[\"A\",\"B\"]',NULL,'[]','[]','person',NULL,?,?)")
        .bind(&candidate_id).bind(&program.summary.id).bind(&outcome_id).bind(uuid::Uuid::new_v4().to_string()).bind(stamp).execute(&pool).await.map_err(db)?;
    sqlx::query(
        "INSERT INTO learning_assessment_candidate_outcomes(candidate_id,outcome_id) VALUES(?,?)",
    )
    .bind(&candidate_id)
    .bind(&outcome_id)
    .execute(&pool)
    .await
    .map_err(db)?;
    sqlx::query("INSERT INTO learning_assessment_forms(id,program_id,blueprint_id,blueprint_revision,retake_of_form_id,status,revision,title,instructions,expected_minutes,allowed_aids_json,purpose,passing_score,rubric_json,feedback_timing,source_version_ids_json,model_name,created_at,updated_at,submitted_at) VALUES(?,?,?,1,NULL,'active',0,'A neutral form','Respond to the prompts.',20,'[]','practice',0.6,'[]','immediate','[]',NULL,?,?,NULL)")
        .bind(&form_id).bind(&program.summary.id).bind(&blueprint_id).bind(stamp).bind(stamp).execute(&pool).await.map_err(db)?;
    sqlx::query("INSERT INTO learning_assessment_form_items(form_id,ordinal,candidate_id,outcome_id,format,difficulty,prompt,options_json,artifact_kind,rubric_json,points,previously_exposed) VALUES(?,0,?,?,'single_choice',1,'A neutral prompt','[\"A\",\"B\"]',NULL,'[]',1,0)")
        .bind(&form_id).bind(&candidate_id).bind(&outcome_id).execute(&pool).await.map_err(db)?;
    sqlx::query("INSERT INTO learning_assessment_form_item_outcomes(form_id,item_ordinal,outcome_id) VALUES(?,0,?)")
        .bind(&form_id).bind(&outcome_id).execute(&pool).await.map_err(db)?;

    let rejected_completed = repo
        .preview(&PreviewLearningCurriculumRevisionRequestDto {
            operation_id: op_id(),
            program_id: program.summary.id.clone(),
            expected_revision: program.summary.revision,
            reason: "Try changing completed work".into(),
            operations: vec![LearningCurriculumOperation::SkipLesson {
                lesson_id: completed_lesson,
            }],
        })
        .await;
    assert!(rejected_completed.is_err());
    let rejected_assessed = repo
        .preview(&PreviewLearningCurriculumRevisionRequestDto {
            operation_id: op_id(),
            program_id: program.summary.id.clone(),
            expected_revision: program.summary.revision,
            reason: "Try changing assessed module work".into(),
            operations: vec![LearningCurriculumOperation::EditLesson {
                lesson_id: assessed_lesson,
                title: "Changed after assessment start".into(),
                objective: "A changed objective".into(),
                estimated_minutes: 25,
            }],
        })
        .await;
    assert!(rejected_assessed.is_err());
    Ok(())
}

#[tokio::test]
async fn selected_lesson_jobs_allow_placement_and_retry_failures_without_duplicate_workers(
) -> Result<()> {
    let pool = pool().await?;
    let program = active_program(&pool).await?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    let target = program.modules[1].lessons[1].id.clone();
    let request = StartLearningGenerationJobRequestDto {
        operation_id: op_id(),
        program_id: program.summary.id.clone(),
        expected_revision: program.summary.revision,
        kind: LearningGenerationJobKind::LessonPreparation,
        request_json: serde_json::json!({"lessonIds":[target]}).to_string(),
        progress_total: 1,
    };
    let first = repo.start_lesson_job(&request).await?;
    assert_eq!(repo.start_lesson_job(&request).await?.id, first.id);
    assert!(jobs(&pool).claim(&first.id).await?.is_some());
    jobs(&pool)
        .fail(&first.id, "generation_failed", "Synthetic review failure")
        .await?;
    let (left, right) = tokio::join!(
        repo.start_lesson_job(&request),
        repo.start_lesson_job(&request)
    );
    let retry = left?;
    assert_eq!(retry.id, right?.id);
    assert_ne!(retry.id, first.id);
    assert_eq!(retry.retry_of_job_id.as_deref(), Some(first.id.as_str()));
    assert!(jobs(&pool).claim(&retry.id).await?.is_some());
    assert!(jobs(&pool).claim(&retry.id).await?.is_none());
    jobs(&pool)
        .fail(&retry.id, "generation_failed", "A second synthetic failure")
        .await?;
    let next = repo.start_lesson_job(&request).await?;
    assert_eq!(next.retry_of_job_id.as_deref(), Some(retry.id.as_str()));
    assert_eq!(repo.start_lesson_job(&request).await?.id, next.id);
    let mut invalid = request.clone();
    invalid.operation_id = op_id();
    invalid.request_json = serde_json::json!({"lessonIds":[op_id()]}).to_string();
    assert!(repo.start_lesson_job(&invalid).await.is_err());
    let mut batch = request;
    batch.operation_id = op_id();
    batch.progress_total = 2;
    batch.request_json=serde_json::json!({"lessonIds":[program.modules[1].lessons[1].id,program.modules[0].lessons[0].id]}).to_string();
    assert!(repo.start_job(&batch).await.is_err());
    Ok(())
}

#[tokio::test]
async fn a_job_an_update_ended_says_so_and_another_does_not() -> Result<()> {
    let pool = pool().await?;
    let program = active_program(&pool).await?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    let start = |lesson: &str| StartLearningGenerationJobRequestDto {
        operation_id: op_id(),
        program_id: program.summary.id.clone(),
        expected_revision: program.summary.revision,
        kind: LearningGenerationJobKind::LessonPreparation,
        request_json: serde_json::json!({"lessonIds":[lesson]}).to_string(),
        progress_total: 1,
    };
    let ended = repo
        .start_lesson_job(&start(&program.modules[1].lessons[1].id))
        .await?;
    assert!(jobs(&pool).claim(&ended.id).await?.is_some());
    jobs(&pool)
        .fail(&ended.id, "generation_failed", "Synthetic failure")
        .await?;
    sqlx::query("UPDATE jobs SET status = 'cancelled', error_code = 'ended_by_update', error_message = NULL WHERE id = ?")
        .bind(&ended.id)
        .execute(&pool)
        .await
        .map_err(crate::persistence::db)?;
    let other = repo
        .start_lesson_job(&start(&program.modules[0].lessons[0].id))
        .await?;
    assert!(jobs(&pool).claim(&other.id).await?.is_some());
    jobs(&pool)
        .fail(&other.id, "generation_failed", "Another synthetic failure")
        .await?;

    let listed = repo.jobs(&program.summary.id).await?;
    let flag = |id: &str| {
        let job = listed.iter().find(|job| job.id == id).unwrap();
        serde_json::to_value(job).unwrap()["endedByUpdate"].clone()
    };
    assert_eq!(flag(&ended.id), serde_json::json!(true));
    assert_eq!(flag(&other.id), serde_json::json!(false));
    Ok(())
}

#[tokio::test]
async fn deleting_a_program_removes_its_jobs_and_their_saved_work() -> Result<()> {
    let pool = pool().await?;
    let program = active_program(&pool).await?;
    let other = active_program(&pool).await?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    let start = |program: &LearningProgramDto| StartLearningGenerationJobRequestDto {
        operation_id: op_id(),
        program_id: program.summary.id.clone(),
        expected_revision: program.summary.revision,
        kind: LearningGenerationJobKind::LessonPreparation,
        request_json: serde_json::json!({"lessonIds":[program.modules[0].lessons[0].id]})
            .to_string(),
        progress_total: 1,
    };
    let job = repo.start_job(&start(&program)).await?;
    let kept = repo.start_job(&start(&other)).await?;
    assert!(jobs(&pool).claim(&job.id).await?.is_some());
    let lesson = &program.modules[0].lessons[0].id;
    repo.save_lesson_checkpoint(&job.id, lesson, "step", serde_json::json!(1))
        .await?;
    jobs(&pool)
        .fail(&job.id, "generation_failed", "Fixture failure")
        .await?;
    let retry = repo
        .retry_job(&LearningGenerationJobActionRequestDto {
            operation_id: op_id(),
            program_id: program.summary.id.clone(),
            job_id: job.id.clone(),
            expected_revision: program.summary.revision,
        })
        .await?;

    LearningRepository::new(pool.clone())
        .delete(&program.summary.id)
        .await?;
    for id in [&job.id, &retry.id] {
        assert!(matches!(
            jobs(&pool).get(id).await,
            Err(AppError::NotFound(_))
        ));
        assert!(jobs(&pool)
            .checkpoint(id, &lesson_checkpoint_key(lesson, "step"))
            .await?
            .is_none());
    }
    assert!(repo.jobs(&program.summary.id).await?.is_empty());
    assert_eq!(
        repo.job(&kept.id).await?.status,
        LearningGenerationJobStatus::Pending
    );
    Ok(())
}

#[tokio::test]
async fn job_restart_recovery_retry_and_publication_are_durable_and_atomic() -> Result<()> {
    let pool = pool().await?;
    let program = active_program(&pool).await?;
    let curriculum = LearningCurriculumRepository::new(pool.clone());
    let stored = curriculum.plan(&program.summary.id).await?;
    let accepted = stored.accepted_revision.expect("accepted");
    let target = accepted.modules[0].lessons[0].id.clone();
    // Establish the conflicting persisted item before the job's content
    // baseline: another course's bank item already holds the ID the staged
    // lesson's question joins the bank under. This test exercises
    // publication rollback, not a stale job.
    let duplicate_question_id = uuid::Uuid::new_v4().to_string();
    let other = active_program(&pool).await?;
    let other_outcome = curriculum
        .plan(&other.summary.id)
        .await?
        .accepted_revision
        .expect("accepted")
        .modules[0]
        .outcome_ids[0]
        .clone();
    sqlx::query("INSERT INTO learning_assessment_candidates(id,program_id,outcome_id,format,difficulty,prompt,options_json,artifact_kind,rubric_json,source_version_ids_json,authored_by,author_model,content_sha256,created_at) VALUES(?,?,?,'single_choice',2,'Existing item','[\"One\",\"Two\"]',NULL,'[]','[]','person',NULL,?,?)")
        .bind(&duplicate_question_id).bind(&other.summary.id).bind(&other_outcome).bind(uuid::Uuid::new_v4().to_string()).bind(now()).execute(&pool).await.map_err(db)?;
    let request = StartLearningGenerationJobRequestDto {
        operation_id: op_id(),
        program_id: program.summary.id.clone(),
        expected_revision: program.summary.revision,
        kind: LearningGenerationJobKind::LessonPreparation,
        request_json: serde_json::json!({"lessonIds":[target]}).to_string(),
        progress_total: 1,
    };
    let job = curriculum.start_job(&request).await?;
    assert_eq!(curriculum.start_job(&request).await?.id, job.id);
    let mut mismatch = request.clone();
    mismatch.request_json = "{\"lessonIds\":[]}".into();
    assert!(curriculum.start_job(&mismatch).await.is_err());
    assert!(jobs(&pool).claim(&job.id).await?.is_some());
    let draft_body = "{\"blocks\":[{\"body\":\"Unpublished lesson draft\"}]}";
    crate::lesson_drafts::run(&curriculum, &job.id, &target, async {
        crate::lesson_drafts::authoring(crate::lesson_drafts::AuthoringContext {
            scope_hash: "same-lesson-request".into(),
            prompt: "Original authoring prompt and citation order".into(),
            sources: program.sources.clone(),
            reference_hashes: program
                .sources
                .iter()
                .map(|source| {
                    (
                        source.id.clone(),
                        crate::content_verification::digest(&source.excerpt),
                    )
                })
                .collect(),
        });
        assert!(crate::lesson_drafts::resume("matching-inputs".into())
            .await?
            .is_none());
        crate::lesson_drafts::save(draft_body).await?;
        crate::lesson_drafts::record_teaching(draft_body, "teaching-policy".into()).await?;
        crate::lesson_drafts::record_claim_inventory(draft_body, "audited-inventory".into())
            .await?;
        crate::lesson_drafts::record_pending_repair(draft_body, "failed-checks-to-repair".into())
            .await?;
        assert!(crate::lesson_drafts::teaching_completed(draft_body, "teaching-policy").await?);
        Ok(())
    })
    .await?;
    assert_eq!(
        curriculum
            .lesson_draft(&job.id, &target)
            .await?
            .unwrap()
            .candidate,
        draft_body
    );
    assert!(LearningRepository::new(pool.clone())
        .get(&program.summary.id)
        .await?
        .modules[0]
        .lessons[0]
        .blocks
        .is_empty());
    let activity = curriculum::LearningGenerationActivity {
        phase: curriculum::LearningGenerationPhase::Evidence,
        checks_completed: 7,
        checks_total: 10,
        checks_reused: 2,
        ..Default::default()
    };
    let snapshot = serde_json::to_value(&activity)?;
    jobs(&pool)
        .progress(&job.id, 1, "Prepared one lesson", Some(&snapshot))
        .await?;
    assert!(
        jobs(&pool)
            .progress(&job.id, 0, "An older heartbeat", Some(&snapshot))
            .await?
    );
    let after_delayed_heartbeat = curriculum.job(&job.id).await?;
    assert_eq!(after_delayed_heartbeat.progress_completed, 1);
    assert_eq!(
        after_delayed_heartbeat.progress_message,
        "Prepared one lesson"
    );
    assert!(jobs(&pool)
        .progress(&job.id, 0, "Backwards", None)
        .await
        .is_err());
    curriculum
        .save_lesson_checkpoint(
            &job.id,
            &target,
            "completed-query",
            serde_json::json!({"query":"public subject terms"}),
        )
        .await?;

    // Process death retains the same queued job and its unpublished draft.
    // Explicit retry after a failure copies these checkpoints to a child.
    jobs(&pool)
        .recover(LESSON_PREPARATION, RecoveryPolicy::Requeue)
        .await?;
    let interrupted = curriculum.job(&job.id).await?;
    assert_eq!(interrupted.status, LearningGenerationJobStatus::Pending);
    assert!(interrupted.finished_at.is_none());
    assert_eq!(interrupted.activity, Some(activity.clone()));
    assert!(jobs(&pool).claim(&job.id).await?.is_some());
    jobs(&pool)
        .fail(
            &job.id,
            "generation_failed",
            "Synthetic failure after restart",
        )
        .await?;
    let retry_request = LearningGenerationJobActionRequestDto {
        operation_id: op_id(),
        program_id: program.summary.id.clone(),
        job_id: job.id.clone(),
        expected_revision: program.summary.revision,
    };
    let retry = curriculum.retry_job(&retry_request).await?;
    assert_eq!(retry.activity, Some(activity));
    assert_eq!(
        curriculum
            .lesson_checkpoint(&retry.id, &target, "completed-query")
            .await?,
        Some(serde_json::json!({"query":"public subject terms"}))
    );
    assert_eq!(retry.retry_of_job_id.as_deref(), Some(job.id.as_str()));
    assert_eq!(curriculum.retry_job(&retry_request).await?.id, retry.id);
    let mut replay_mismatch = retry_request.clone();
    replay_mismatch.job_id = retry.id.clone();
    assert!(curriculum.retry_job(&replay_mismatch).await.is_err());

    assert!(jobs(&pool).claim(&retry.id).await?.is_some());
    let reopened = LearningCurriculumRepository::new(pool.clone());
    crate::lesson_drafts::run(&reopened, &retry.id, &target, async {
        let mut augmented = program.sources.clone();
        let mut additional = augmented[0].clone();
        additional.id = uuid::Uuid::new_v4().to_string();
        additional.excerpt = "Additional research never grants publication approval.".into();
        augmented.push(additional);
        let (input_hash, authoring) =
            crate::lesson_drafts::reusable_authoring("same-lesson-request", &augmented)
                .await?
                .expect("additional research retains the authoring context");
        assert_eq!(input_hash, "matching-inputs");
        assert_eq!(
            authoring.prompt,
            "Original authoring prompt and citation order"
        );
        assert_eq!(authoring.sources.len(), program.sources.len());
        assert_eq!(authoring.sources[0].id, program.sources[0].id);
        assert!(
            crate::lesson_drafts::reusable_authoring("different-lesson-request", &augmented)
                .await?
                .is_none()
        );
        augmented[0]
            .excerpt
            .push_str(" Changed original reference.");
        assert!(
            crate::lesson_drafts::reusable_authoring("same-lesson-request", &augmented)
                .await?
                .is_none()
        );
        assert!(
            crate::lesson_drafts::reusable_authoring("same-lesson-request", &[])
                .await?
                .is_none()
        );
        crate::lesson_drafts::authoring(authoring);
        assert_eq!(
            crate::lesson_drafts::resume("matching-inputs".into())
                .await?
                .as_deref(),
            Some(draft_body)
        );
        let formatted =
            serde_json::to_string_pretty(&serde_json::from_str::<serde_json::Value>(draft_body)?)?;
        assert_eq!(
            crate::lesson_drafts::pending_repair(&formatted)
                .await?
                .as_deref(),
            Some("failed-checks-to-repair")
        );
        assert_eq!(
            crate::lesson_drafts::claim_inventory(&formatted)
                .await?
                .as_deref(),
            Some("audited-inventory")
        );
        assert!(crate::lesson_drafts::claim_inventory("changed draft")
            .await?
            .is_none());
        assert!(crate::lesson_drafts::teaching_completed(draft_body, "teaching-policy").await?);
        assert!(!crate::lesson_drafts::teaching_completed(draft_body, "new-policy").await?);
        assert!(
            !crate::lesson_drafts::teaching_completed("changed draft", "teaching-policy").await?
        );
        assert!(
            crate::lesson_drafts::resume("changed-source-or-prompt".into())
                .await?
                .is_none()
        );
        assert!(!crate::lesson_drafts::teaching_completed(draft_body, "teaching-policy").await?);
        crate::lesson_drafts::resume("matching-inputs".into()).await?;
        crate::lesson_drafts::save("changed draft").await?;
        crate::lesson_drafts::save(draft_body).await?;
        assert!(crate::lesson_drafts::claim_inventory(draft_body)
            .await?
            .is_none());
        assert!(crate::lesson_drafts::pending_repair(draft_body)
            .await?
            .is_none());
        assert!(!crate::lesson_drafts::teaching_completed(draft_body, "teaching-policy").await?);
        Ok(())
    })
    .await?;
    assert!(reopened
        .save_lesson_draft(
            &job.id,
            &target,
            crate::lesson_drafts::Draft {
                input_hash: "matching-inputs".into(),
                candidate: "should not overwrite an interrupted job".into(),
                teaching_receipt: None,
                authoring: None,
                claim_inventory: None,
                pending_repair: None,
                teaching_passes: None,
            }
        )
        .await
        .is_err());
    let before_revision: i64 =
        sqlx::query_scalar("SELECT revision FROM learning_programs WHERE id=?")
            .bind(&program.summary.id)
            .fetch_one(&pool)
            .await
            .map_err(db)?;
    let mut broken = crate::dto::PreparedLearningLesson {
        verification: None,
        blocks: vec![crate::dto::LearningBlockDto {
            rubric: vec![],
            kind: crate::dto::LearningBlockKind::Explanation,
            title: "A staged explanation".into(),
            body: "This should roll back with the transaction.".into(),
            source_ids: vec![],
        }],
        questions: vec![crate::dto::LessonQuestion {
            id: duplicate_question_id.clone(),
            kind: crate::dto::LessonQuestionKind::Practice,
            prompt: "A conflicting item".into(),
            options: vec!["One".into(), "Two".into()],
            source_ids: vec![],
        }],
        // Keyed, so the question joins the bank and meets the conflict.
        keys: vec![crate::dto::LessonAnswerKey {
            question_id: duplicate_question_id,
            correct_index: 0,
            explanation: "One is the answer.".into(),
            rationales: vec![],
        }],
    };
    broken.verification = Some(crate::content_verification::tests::attest(&target, &broken));
    assert!(curriculum
        .publish_prepared_lessons(&retry.id, before_revision, &[(target.clone(), broken)])
        .await
        .is_err());
    let preparation: String =
        sqlx::query_scalar("SELECT preparation FROM learning_lessons WHERE id=?")
            .bind(&target)
            .fetch_one(&pool)
            .await
            .map_err(db)?;
    assert_eq!(preparation, "outline");
    let block_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM learning_blocks WHERE lesson_id=?")
            .bind(&target)
            .fetch_one(&pool)
            .await
            .map_err(db)?;
    assert_eq!(
        block_count, 0,
        "the staged block must roll back with the duplicate question"
    );
    let after_failure: i64 =
        sqlx::query_scalar("SELECT revision FROM learning_programs WHERE id=?")
            .bind(&program.summary.id)
            .fetch_one(&pool)
            .await
            .map_err(db)?;
    assert_eq!(after_failure, before_revision);

    let mut good = crate::dto::PreparedLearningLesson {
        verification: None,
        blocks: vec![crate::dto::LearningBlockDto {
            kind: crate::dto::LearningBlockKind::Explanation,
            title: "Verified teaching fixture".into(),
            body: "A persisted explanation.".into(),
            source_ids: vec![],
            rubric: vec![],
        }],
        questions: vec![],
        keys: vec![],
    };
    good.verification = Some(crate::content_verification::tests::attest(&target, &good));
    curriculum
        .save_lesson_checkpoint(
            &retry.id,
            &target,
            "prepared-v1",
            serde_json::to_value(&good)?,
        )
        .await?;
    // Interrupt after all expensive work, immediately before publication.
    jobs(&pool).suspend(&retry.id).await?;
    assert!(curriculum
        .save_lesson_checkpoint(&retry.id, &target, "late-write", serde_json::json!(true))
        .await
        .is_err());
    jobs(&pool)
        .recover(LESSON_PREPARATION, RecoveryPolicy::Requeue)
        .await?;
    assert!(jobs(&pool).claim(&retry.id).await?.is_some());
    let good = serde_json::from_value(
        curriculum
            .lesson_checkpoint(&retry.id, &target, "prepared-v1")
            .await?
            .unwrap(),
    )?;
    curriculum
        .publish_prepared_lessons(&retry.id, before_revision, &[(target.clone(), good)])
        .await?;
    let completed = curriculum.job(&retry.id).await?;
    assert_eq!(completed.status, LearningGenerationJobStatus::Completed);
    assert!(completed.finished_at.is_some());
    jobs(&pool)
        .recover(LESSON_PREPARATION, RecoveryPolicy::Requeue)
        .await?;
    assert!(
        jobs(&pool).claim(&retry.id).await?.is_none(),
        "Redelivering a completed job cannot publish twice"
    );
    let prepared: String =
        sqlx::query_scalar("SELECT preparation FROM learning_lessons WHERE id=?")
            .bind(&target)
            .fetch_one(&pool)
            .await
            .map_err(db)?;
    assert_eq!(prepared, "ready");
    let published_plan = curriculum.plan(&program.summary.id).await?;
    let published_snapshot = published_plan
        .accepted_revision
        .expect("published snapshot");
    assert_eq!(
        published_snapshot.revision_number,
        accepted.revision_number + 1
    );
    assert_eq!(
        published_snapshot.parent_revision_id.as_deref(),
        Some(accepted.id.as_str())
    );
    assert_eq!(
        published_snapshot.modules[0].lessons[0].state,
        LearningCurriculumLessonState::Ready
    );
    Ok(())
}

async fn shared_file_pool(directory: &std::path::Path) -> Result<SqlitePool> {
    let options = crate::test_support::file_options(directory.join("plan.sqlite"));
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
        .map_err(db)?;
    sqlx::migrate!("../lattice-desktop/migrations")
        .run(&pool)
        .await
        .unwrap();
    Ok(pool)
}

#[tokio::test]
async fn loading_a_seeded_plan_takes_no_write_lock() -> Result<()> {
    let directory = tempfile::tempdir().unwrap();
    let pool = shared_file_pool(directory.path()).await?;
    let program = active_program(&pool).await?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    let seeded = repo.plan(&program.summary.id).await?;
    let writer = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)?;
    let loaded = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        repo.plan(&program.summary.id),
    )
    .await
    .expect("Loading a seeded plan must not wait for the write lock")?;
    assert_eq!(
        loaded.accepted_revision.unwrap().id,
        seeded.accepted_revision.unwrap().id
    );
    writer.rollback().await.map_err(db)?;
    Ok(())
}

#[tokio::test]
async fn concurrent_plan_loads_of_a_legacy_program_seed_one_accepted_revision() -> Result<()> {
    let directory = tempfile::tempdir().unwrap();
    let pool = shared_file_pool(directory.path()).await?;
    let program = active_program(&pool).await?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    let accepted_before: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM learning_curriculum_revisions WHERE program_id=? AND status='accepted'",
    )
    .bind(&program.summary.id)
    .fetch_one(&pool)
    .await
    .map_err(db)?;
    assert_eq!(accepted_before, 0);
    let (left, right) = tokio::join!(
        repo.plan(&program.summary.id),
        repo.plan(&program.summary.id)
    );
    assert_eq!(
        left?.accepted_revision.unwrap().id,
        right?.accepted_revision.unwrap().id
    );
    let accepted_after: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM learning_curriculum_revisions WHERE program_id=? AND status='accepted'",
    )
    .bind(&program.summary.id)
    .fetch_one(&pool)
    .await
    .map_err(db)?;
    assert_eq!(accepted_after, 1);
    Ok(())
}

fn module_request(
    program: &LearningProgramDto,
    module: usize,
) -> StartLearningModuleJobsRequestDto {
    StartLearningModuleJobsRequestDto {
        operation_id: stable_job_operation_id(
            &program.summary.id,
            &format!("module:{}", program.modules[module].id),
            program.summary.revision,
        ),
        program_id: program.summary.id.clone(),
        module_id: program.modules[module].id.clone(),
        expected_revision: program.summary.revision,
    }
}

async fn lesson_of(pool: &SqlitePool, job: &LearningGenerationJob) -> Result<String> {
    sqlx::query_scalar("SELECT json_extract(requested_json,'$.lessonIds[0]') FROM jobs WHERE id=?")
        .bind(&job.id)
        .fetch_one(pool)
        .await
        .map_err(db)
}

async fn lessons_of(pool: &SqlitePool, jobs: &[LearningGenerationJob]) -> Result<Vec<String>> {
    let mut lessons = Vec::new();
    for job in jobs {
        lessons.push(lesson_of(pool, job).await?);
    }
    Ok(lessons)
}

fn lesson_ids(program: &LearningProgramDto, module: usize) -> Vec<String> {
    program.modules[module]
        .lessons
        .iter()
        .map(|l| l.id.clone())
        .collect()
}

async fn make_ready(pool: &SqlitePool, lesson: &str) -> Result<()> {
    sqlx::query("UPDATE learning_lessons SET preparation='ready' WHERE id=?")
        .bind(lesson)
        .execute(pool)
        .await
        .map_err(db)?;
    Ok(())
}

#[tokio::test]
async fn a_module_queues_a_job_for_each_unprepared_lesson_in_course_order() -> Result<()> {
    let pool = pool().await?;
    let program = active_program(&pool).await?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    let queued = repo.start_module_jobs(&module_request(&program, 1)).await?;
    assert_eq!(lessons_of(&pool, &queued).await?, lesson_ids(&program, 1));
    for job in &queued {
        assert_eq!(job.status, LearningGenerationJobStatus::Pending);
        assert_eq!(job.progress_total, 1);
    }
    let listed = repo.jobs(&program.summary.id).await?;
    assert_eq!(
        listed.iter().map(|j| j.id.clone()).collect::<Vec<_>>(),
        queued.iter().map(|j| j.id.clone()).collect::<Vec<_>>()
    );
    Ok(())
}

#[tokio::test]
async fn a_module_skips_a_lesson_that_is_ready() -> Result<()> {
    let pool = pool().await?;
    let program = active_program(&pool).await?;
    let lessons = lesson_ids(&program, 0);
    make_ready(&pool, &lessons[0]).await?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    let queued = repo.start_module_jobs(&module_request(&program, 0)).await?;
    assert_eq!(lessons_of(&pool, &queued).await?, vec![lessons[1].clone()]);
    Ok(())
}

#[tokio::test]
async fn a_module_returns_the_job_of_a_lesson_already_queued() -> Result<()> {
    let pool = pool().await?;
    let program = active_program(&pool).await?;
    let lessons = lesson_ids(&program, 0);
    let repo = LearningCurriculumRepository::new(pool.clone());
    let existing = repo
        .start_job(&StartLearningGenerationJobRequestDto {
            operation_id: op_id(),
            program_id: program.summary.id.clone(),
            expected_revision: program.summary.revision,
            kind: LearningGenerationJobKind::LessonPreparation,
            request_json: serde_json::json!({"lessonIds":[lessons[1]]}).to_string(),
            progress_total: 1,
        })
        .await?;
    let queued = repo.start_module_jobs(&module_request(&program, 0)).await?;
    assert_eq!(queued.len(), 2);
    assert_eq!(queued[1].id, existing.id);
    assert_ne!(queued[0].id, existing.id);
    assert_eq!(repo.jobs(&program.summary.id).await?.len(), 2);
    Ok(())
}

#[tokio::test]
async fn repeating_a_module_request_returns_the_same_jobs() -> Result<()> {
    let pool = pool().await?;
    let program = active_program(&pool).await?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    let request = module_request(&program, 0);
    let first = repo.start_module_jobs(&request).await?;
    let again = repo.start_module_jobs(&request).await?;
    assert_eq!(first.len(), 2);
    assert_eq!(
        first.iter().map(|j| j.id.clone()).collect::<Vec<_>>(),
        again.iter().map(|j| j.id.clone()).collect::<Vec<_>>()
    );
    assert_eq!(repo.jobs(&program.summary.id).await?.len(), 2);
    Ok(())
}

#[tokio::test]
async fn a_module_with_nothing_to_prepare_queues_nothing() -> Result<()> {
    let pool = pool().await?;
    let program = active_program(&pool).await?;
    for lesson in lesson_ids(&program, 0) {
        make_ready(&pool, &lesson).await?;
    }
    let repo = LearningCurriculumRepository::new(pool.clone());
    let queued = repo.start_module_jobs(&module_request(&program, 0)).await?;
    assert!(queued.is_empty());
    assert!(repo.jobs(&program.summary.id).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_module_request_against_a_stale_revision_queues_nothing() -> Result<()> {
    let pool = pool().await?;
    let program = active_program(&pool).await?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    let mut stale = module_request(&program, 0);
    stale.expected_revision += 1;
    let error = repo.start_module_jobs(&stale).await.unwrap_err();
    assert!(error.to_string().contains("Program changed"), "{error}");
    assert!(repo.jobs(&program.summary.id).await?.is_empty());
    let queued = repo.start_module_jobs(&module_request(&program, 0)).await?;
    assert_eq!(queued.len(), 2);
    Ok(())
}

#[tokio::test]
async fn a_module_request_against_an_inactive_program_is_refused() -> Result<()> {
    let pool = pool().await?;
    let program = fixture();
    LearningRepository::new(pool.clone())
        .create(&program)
        .await?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    let error = repo
        .start_module_jobs(&module_request(&program, 0))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("not active"), "{error}");
    assert!(repo.jobs(&program.summary.id).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_module_skips_a_lesson_that_waits_for_the_users_review() -> Result<()> {
    let pool = pool().await?;
    let program = active_program(&pool).await?;
    let lessons = lesson_ids(&program, 0);
    let repo = LearningCurriculumRepository::new(pool.clone());
    let held = repo
        .start_job(&StartLearningGenerationJobRequestDto {
            operation_id: op_id(),
            program_id: program.summary.id.clone(),
            expected_revision: program.summary.revision,
            kind: LearningGenerationJobKind::LessonPreparation,
            request_json: serde_json::json!({"lessonIds":[lessons[0]]}).to_string(),
            progress_total: 1,
        })
        .await?;
    assert!(jobs(&pool).claim(&held.id).await?.is_some());
    jobs(&pool)
        .fail(&held.id, "generation_failed", "Held for review")
        .await?;
    sqlx::query("INSERT INTO learning_lesson_reviews(lesson_id,program_id,job_id,state,revision,draft_sha256,checker_model,opened_at,updated_at) VALUES(?,?,?,'needs_review',0,'sha','fixture',1,1)")
        .bind(&lessons[0])
        .bind(&program.summary.id)
        .bind(&held.id)
        .execute(&pool)
        .await
        .map_err(db)?;
    let queued = repo.start_module_jobs(&module_request(&program, 0)).await?;
    assert_eq!(lessons_of(&pool, &queued).await?, vec![lessons[1].clone()]);
    Ok(())
}

#[tokio::test]
async fn a_job_names_the_lessons_it_prepares() -> Result<()> {
    let pool = pool().await?;
    let program = active_program(&pool).await?;
    let lessons = lesson_ids(&program, 0);
    let repo = LearningCurriculumRepository::new(pool.clone());
    let job = repo
        .start_job(&StartLearningGenerationJobRequestDto {
            operation_id: op_id(),
            program_id: program.summary.id.clone(),
            expected_revision: program.summary.revision,
            kind: LearningGenerationJobKind::LessonPreparation,
            request_json: serde_json::json!({"lessonIds":lessons}).to_string(),
            progress_total: 2,
        })
        .await?;
    assert_eq!(job.lesson_ids, lessons);
    assert_eq!(repo.job(&job.id).await?.lesson_ids, lessons);
    assert_eq!(repo.jobs(&program.summary.id).await?[0].lesson_ids, lessons);
    Ok(())
}

#[tokio::test]
async fn cancelling_a_queued_lesson_and_preparing_the_module_again_requeues_it() -> Result<()> {
    let pool = pool().await?;
    let program = active_program(&pool).await?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    let first_jobs = repo
        .start_module_jobs(&module_request(&program, 0))
        .await
        .unwrap();
    repo.cancel_job(&LearningGenerationJobActionRequestDto {
        operation_id: op_id(),
        program_id: program.summary.id.clone(),
        job_id: first_jobs[1].id.clone(),
        expected_revision: program.summary.revision,
    })
    .await
    .unwrap();
    let second_jobs = repo
        .start_module_jobs(&module_request(&program, 0))
        .await
        .unwrap();
    assert_eq!(second_jobs.len(), 2);
    assert_eq!(second_jobs[0].id, first_jobs[0].id);
    assert_eq!(second_jobs[0].status, LearningGenerationJobStatus::Pending);
    assert_ne!(second_jobs[1].id, first_jobs[1].id);
    assert_eq!(second_jobs[1].status, LearningGenerationJobStatus::Pending);
    assert_eq!(
        second_jobs[1].retry_of_job_id.as_deref(),
        Some(first_jobs[1].id.as_str())
    );
    Ok(())
}
