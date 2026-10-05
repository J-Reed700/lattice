use crate::features::learning::{
    assessment_engine as engine,
    assessment_repository::LearningAssessmentRepository,
    dto::*,
    portability_dto::DeleteLearningSourceRequestDto,
    repository::LearningRepository,
    source_library::{CapturedLearningSource, LearningSourceLibraryRepository},
};
use crate::shared::error::{AppError, Result};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    SqlitePool,
};
use std::str::FromStr;

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn db(e: sqlx::Error) -> AppError {
    AppError::Database(e.to_string())
}
async fn migrated(path: Option<&std::path::Path>) -> Result<SqlitePool> {
    let pool = if let Some(path) = path {
        let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))
            .map_err(db)?
            .create_if_missing(true)
            .foreign_keys(true);
        SqlitePoolOptions::new()
            .max_connections(2)
            .connect_with(options)
            .await
            .map_err(db)?
    } else {
        crate::features::learning::tests::pool().await?
    };
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(pool)
}
async fn fixture(pool: &SqlitePool) -> Result<(LearningProgramDto, String, String)> {
    let program = crate::features::learning::tests::fixture();
    let lr = LearningRepository::new(pool.clone());
    lr.create(&program).await?;
    lr.accept(&AcceptLearningProgramRequestDto {
        program_id: program.summary.id.clone(),
        expected_revision: 0,
        title: program.summary.title.clone(),
    })
    .await?;
    let source = LearningSourceLibraryRepository::new(pool.clone());
    let source_id = id();
    let version_id = id();
    source.add(&id(),&source_id,&version_id,&program.summary.id,LearningSourceKind::Pasted,"add_text","pasted text",None,LearningSourcePolicy::Fixed,CapturedLearningSource{title:"Assessment source".into(),publisher:None,requested_url:None,resolved_url:None,text:"A system contains interacting parts. Each part contributes to a larger process. The relationships between the parts help explain how the system behaves.".into(),truncated:false,extraction_version:"test_v1".into()},"hash".into()).await?;
    let ws = LearningAssessmentRepository::new(pool.clone())
        .workspace(&program.summary.id)
        .await?;
    let outcome = ws
        .outcomes
        .first()
        .ok_or_else(|| AppError::InvalidState("outcome missing".into()))?
        .id
        .clone();
    Ok((lr.get(&program.summary.id).await?, outcome, version_id))
}

#[tokio::test]
async fn generated_keyed_assessment_form_is_persisted_and_renderer_safe() -> Result<()> {
    let pool = migrated(None).await?;
    let (program, outcome, source_id) = fixture(&pool).await?;
    let req = CreateLearningAssessmentBlueprintRequestDto {
        operation_id: id(),
        program_id: program.summary.id.clone(),
        blueprint_id: id(),
        revision: 1,
        predecessor_revision: None,
        purpose: LearningAssessmentPurpose::Checkpoint,
        title: "Systems check".into(),
        instructions: "Choose the supported statement.".into(),
        expected_minutes: 10,
        allowed_aids: vec![],
        passing_score: 0.7,
        feedback_timing: LearningFeedbackTiming::AfterSubmission,
        rubric: vec![LearningRubricCriterion {
            id: id(),
            title: "Evidence".into(),
            description: "Uses a source-backed statement".into(),
            max_points: 1,
        }],
        source_version_ids: vec![source_id.clone()],
        requirements: vec![
            engine::LearningBlueprintRequirement {
                outcome_id: outcome.clone(),
                format: LearningItemFormat::MultipleChoice,
                count: 1,
                difficulty_min: 1,
                difficulty_max: 3,
            },
            engine::LearningBlueprintRequirement {
                outcome_id: outcome.clone(),
                format: LearningItemFormat::Explanation,
                count: 1,
                difficulty_min: 1,
                difficulty_max: 3,
            },
        ],
        change_reason: "Initial version".into(),
    };
    let candidate = LearningAssessmentCandidateWriteDto {
        id: id(),
        outcome_ids: vec![outcome],
        format: LearningItemFormat::MultipleChoice,
        difficulty: 1,
        prompt: "Which statement describes a system?".into(),
        options: vec![
            "Parts interact within a larger process.".into(),
            "Parts never affect one another.".into(),
        ],
        artifact_kind: None,
        rubric: req.rubric.clone(),
        source_version_ids: vec![source_id.clone()],
        answer_explanation: "The source describes interacting parts as a system.".into(),
        answer_key: serde_json::to_value(engine::LearningAssessmentKey::Choice(0))
            .map_err(|e| AppError::Serialization(e.to_string()))?,
    };
    let choice_id = candidate.id.clone();
    let open_id = id();
    let open_candidate = LearningAssessmentCandidateWriteDto {
        id: open_id.clone(),
        outcome_ids: vec![req.requirements[0].outcome_id.clone()],
        format: LearningItemFormat::Explanation,
        difficulty: 1,
        prompt: "Explain how interacting parts contribute to the behavior of a system.".into(),
        options: vec![],
        artifact_kind: None,
        rubric: req.rubric.clone(),
        source_version_ids: req.source_version_ids.clone(),
        answer_explanation: "A rubric-scored explanation does not use a keyed rationale.".into(),
        answer_key: serde_json::to_value(engine::LearningAssessmentKey::Rubric)
            .map_err(|e| AppError::Serialization(e.to_string()))?,
    };
    let repo = LearningAssessmentRepository::new(pool.clone());
    repo.create_blueprint(
        &req,
        &[candidate.clone(), open_candidate.clone()],
        "trusted-model",
        "blueprint-hash",
    )
    .await?;
    let form = repo
        .start_form(
            &StartLearningAssessmentFormRequestDto {
                operation_id: id(),
                form_id: id(),
                program_id: program.summary.id.clone(),
                blueprint_id: req.blueprint_id.clone(),
                blueprint_revision: 1,
                retake_of_form_id: None,
            },
            "start-hash",
        )
        .await?;
    assert_eq!(form.items.len(), 2);
    let mut second_blueprint = req.clone();
    second_blueprint.operation_id = id();
    second_blueprint.blueprint_id = id();
    let mut second_choice = candidate.clone();
    second_choice.id = id();
    second_choice.prompt = "A prompt authored only for the second blueprint.".into();
    let mut second_open = open_candidate.clone();
    second_open.id = id();
    second_open.prompt = "An explanation prompt authored only for the second blueprint.".into();
    repo.create_blueprint(
        &second_blueprint,
        &[second_choice.clone(), second_open.clone()],
        "trusted-model",
        "second-blueprint-hash",
    )
    .await?;
    let second_form = repo
        .start_form(
            &StartLearningAssessmentFormRequestDto {
                operation_id: id(),
                form_id: id(),
                program_id: program.summary.id.clone(),
                blueprint_id: second_blueprint.blueprint_id,
                blueprint_revision: 1,
                retake_of_form_id: None,
            },
            "second-start-hash",
        )
        .await?;
    assert!(second_form
        .items
        .iter()
        .all(|item| { item.prompt == second_choice.prompt || item.prompt == second_open.prompt }));
    assert!(second_form
        .items
        .iter()
        .any(|item| item.id == second_choice.id));
    assert!(second_form
        .items
        .iter()
        .any(|item| item.id == second_open.id));
    let serialized =
        serde_json::to_string(&form).map_err(|e| AppError::Serialization(e.to_string()))?;
    assert!(!serialized.contains("answerKey"));
    let program_id = program.summary.id.clone();
    let form_id = form.id.clone();
    let saved = repo
        .save_response(
            &SaveLearningAssessmentResponseRequestDto {
                operation_id: id(),
                program_id: program_id.clone(),
                form_id: form_id.clone(),
                expected_revision: 0,
                response: engine::LearningAssessmentResponse {
                    item_id: choice_id.clone(),
                    selected_index: Some(0),
                    text: None,
                    ordered_values: vec![],
                    artifact_json: None,
                },
                assistance: vec![],
            },
            "choice-hash",
        )
        .await?;
    assert_eq!(saved.revision, 1);
    let saved = repo
        .save_response(
            &SaveLearningAssessmentResponseRequestDto {
                operation_id: id(),
                program_id: program_id.clone(),
                form_id: form_id.clone(),
                expected_revision: 1,
                response: engine::LearningAssessmentResponse {
                    item_id: open_id.clone(),
                    selected_index: None,
                    text: Some(
                        "A system has interacting parts that contribute to a larger process."
                            .into(),
                    ),
                    ordered_values: vec![],
                    artifact_json: None,
                },
                assistance: vec![],
            },
            "explanation-hash",
        )
        .await?;
    assert_eq!(saved.revision, 2);
    let mut open = std::collections::HashMap::new();
    open.insert(
        open_id,
        (
            LearningAssessmentGradeStatus::Uncertain,
            vec![LearningAssessmentCriterionResultDto {
                criterion_id: req.rubric[0].id.clone(),
                score: None,
                max_points: 1,
                observation: "The response could not be graded reliably.".into(),
                artifact_quote: Some("interacting parts that contribute".into()),
            }],
        ),
    );
    let submit_request = MutateLearningAssessmentFormRequestDto {
        operation_id: id(),
        program_id: program_id.clone(),
        form_id: form_id.clone(),
        expected_revision: 2,
    };
    let submit_hash = crate::features::learning::plugin::source_request_hash(&submit_request)?;
    let submitted = repo
        .submit(
            &submit_request,
            &submit_hash,
            open,
            Some("trusted-grader".into()),
        )
        .await?;
    assert_eq!(submitted.status, LearningAssessmentFormStatus::Submitted);
    let result = submitted
        .submission
        .ok_or_else(|| AppError::InvalidState("submission missing".into()))?;
    assert_eq!(result.item_results.len(), 2);
    assert_eq!(
        result.grade_status,
        LearningAssessmentGradeStatus::Uncertain
    );
    assert!(!result.passed);
    assert_eq!(
        result
            .item_results
            .iter()
            .find(|i| i.item_id == choice_id)
            .and_then(|i| i.correct),
        Some(true)
    );
    let replay = repo
        .preflight_submit(&submit_request, &submit_hash)
        .await?
        .ok_or_else(|| AppError::InvalidState("submit replay was not found".into()))?;
    assert_eq!(replay.status, LearningAssessmentFormStatus::Submitted);
    let changed = MutateLearningAssessmentFormRequestDto {
        expected_revision: 3,
        ..submit_request
    };
    let changed_hash = crate::features::learning::plugin::source_request_hash(&changed)?;
    assert!(repo
        .preflight_submit(&changed, &changed_hash)
        .await
        .is_err());
    let unknown_score: Option<f64> = sqlx::query_scalar(
        "SELECT score FROM learning_evidence_events WHERE program_id=? AND source_id=? AND result='uncertain' LIMIT 1",
    )
    .bind(&program_id)
    .bind(&form_id)
    .fetch_optional(&pool)
    .await
    .map_err(db)?
    .flatten();
    assert_eq!(unknown_score, None);
    let logical_source_id: String = sqlx::query_scalar(
        "SELECT source_id FROM learning_source_versions WHERE program_id=? AND id=?",
    )
    .bind(&program_id)
    .bind(&source_id)
    .fetch_one(&pool)
    .await
    .map_err(db)?;
    LearningSourceLibraryRepository::new(pool.clone())
        .delete_source(&DeleteLearningSourceRequestDto {
            operation_id: id(),
            program_id: program_id.clone(),
            source_id: logical_source_id.clone(),
            expected_revision: 0,
            reason: "Removed after this attempt was submitted".into(),
        })
        .await?;
    let lesson_id = program
        .modules
        .first()
        .and_then(|module| module.lessons.first())
        .map(|lesson| lesson.id.clone())
        .ok_or_else(|| AppError::InvalidState("Fixture has no lesson".into()))?;
    sqlx::query(
        "UPDATE learning_lessons SET title='A later curriculum title' WHERE program_id=? AND id=?",
    )
    .bind(&program_id)
    .bind(lesson_id)
    .execute(&pool)
    .await
    .map_err(db)?;
    let historical = repo.get_form(&form_id).await?;
    assert_eq!(historical.status, LearningAssessmentFormStatus::Submitted);
    assert_eq!(
        historical
            .submission
            .as_ref()
            .map(|value| value.item_results.len()),
        Some(2)
    );
    assert!(historical.source_version_ids.contains(&source_id));
    assert!(LearningSourceLibraryRepository::new(pool.clone())
        .get_version(&GetLearningSourceVersionRequestDto {
            program_id: program_id.clone(),
            source_id: logical_source_id,
            version_id: source_id,
        })
        .await
        .is_ok());
    Ok(())
}

#[tokio::test]
async fn interrupted_assessments_cannot_be_submitted_and_retakes_mark_exposure() -> Result<()> {
    let pool = migrated(None).await?;
    let (program, outcome, source_version_id) = fixture(&pool).await?;
    let rubric = vec![LearningRubricCriterion {
        id: id(),
        title: "Reasoning".into(),
        description: "States the supported relationship".into(),
        max_points: 1,
    }];
    let candidate = LearningAssessmentCandidateWriteDto {
        id: id(),
        outcome_ids: vec![outcome.clone()],
        format: LearningItemFormat::MultipleChoice,
        difficulty: 1,
        prompt: "Which statement is supported?".into(),
        options: vec!["Parts interact.".into(), "Parts are unrelated.".into()],
        artifact_kind: None,
        rubric: rubric.clone(),
        source_version_ids: vec![source_version_id.clone()],
        answer_explanation: "The passage describes interacting parts.".into(),
        answer_key: serde_json::to_value(engine::LearningAssessmentKey::Choice(0))
            .map_err(|error| AppError::Serialization(error.to_string()))?,
    };
    let candidate_id = candidate.id.clone();
    let blueprint = CreateLearningAssessmentBlueprintRequestDto {
        operation_id: id(),
        program_id: program.summary.id.clone(),
        blueprint_id: id(),
        revision: 1,
        predecessor_revision: None,
        purpose: LearningAssessmentPurpose::Checkpoint,
        title: "A focused check".into(),
        instructions: "Answer from the frozen source.".into(),
        expected_minutes: 5,
        allowed_aids: vec![],
        passing_score: 0.7,
        feedback_timing: LearningFeedbackTiming::AfterSubmission,
        rubric,
        source_version_ids: vec![source_version_id],
        requirements: vec![engine::LearningBlueprintRequirement {
            outcome_id: outcome,
            format: LearningItemFormat::MultipleChoice,
            count: 1,
            difficulty_min: 1,
            difficulty_max: 2,
        }],
        change_reason: "First assessment".into(),
    };
    let repo = LearningAssessmentRepository::new(pool.clone());
    repo.create_blueprint(&blueprint, &[candidate], "fixture-author", "blueprint")
        .await?;
    let original = repo
        .start_form(
            &StartLearningAssessmentFormRequestDto {
                operation_id: id(),
                form_id: id(),
                program_id: program.summary.id.clone(),
                blueprint_id: blueprint.blueprint_id.clone(),
                blueprint_revision: 1,
                retake_of_form_id: None,
            },
            "original-form",
        )
        .await?;
    assert!(original.items.iter().any(|item| item.id == candidate_id));
    let interrupted = repo
        .interrupt(
            &MutateLearningAssessmentFormRequestDto {
                operation_id: id(),
                program_id: program.summary.id.clone(),
                form_id: original.id.clone(),
                expected_revision: 0,
            },
            "interrupt",
        )
        .await?;
    assert_eq!(
        interrupted.status,
        LearningAssessmentFormStatus::Interrupted
    );
    let submit = MutateLearningAssessmentFormRequestDto {
        operation_id: id(),
        program_id: program.summary.id.clone(),
        form_id: original.id.clone(),
        expected_revision: interrupted.revision,
    };
    assert!(repo
        .submit(
            &submit,
            "interrupted-submit",
            std::collections::HashMap::new(),
            None
        )
        .await
        .is_err());
    assert!(repo.get_form(&original.id).await?.submission.is_none());

    let retake = repo
        .start_form(
            &StartLearningAssessmentFormRequestDto {
                operation_id: id(),
                form_id: id(),
                program_id: program.summary.id.clone(),
                blueprint_id: blueprint.blueprint_id,
                blueprint_revision: 1,
                retake_of_form_id: Some(original.id),
            },
            "retake-form",
        )
        .await?;
    assert!(retake
        .items
        .iter()
        .any(|item| item.id == candidate_id && item.previously_exposed));
    Ok(())
}

#[tokio::test]
async fn assessment_outcomes_are_seeded_once_and_program_scoped() -> Result<()> {
    let pool = migrated(None).await?;
    let (program, _, _) = fixture(&pool).await?;
    let repo = LearningAssessmentRepository::new(pool.clone());
    let first = repo.workspace(&program.summary.id).await?;
    let second = repo.workspace(&program.summary.id).await?;
    assert_eq!(first.outcomes.len(), second.outcomes.len());
    assert!(!first.outcomes.is_empty());
    let other = crate::features::learning::tests::fixture();
    LearningRepository::new(pool.clone()).create(&other).await?;
    let other_ws = repo.workspace(&other.summary.id).await?;
    assert_ne!(
        first.outcomes.first().map(|o| &o.id),
        other_ws.outcomes.first().map(|o| &o.id)
    );
    Ok(())
}
