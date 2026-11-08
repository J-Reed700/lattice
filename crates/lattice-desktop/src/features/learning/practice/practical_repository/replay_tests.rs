//! Operation replays and the generation context read back for simulations.
use super::*;
use crate::features::learning::{
    dto::AcceptLearningProgramRequestDto,
    practical_generation::{GeneratedPracticalActivity, GeneratedPracticalFile, PracticalFileRole},
    repository::LearningRepository,
    tests,
};

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

async fn program(pool: &SqlitePool) -> Result<(String, String)> {
    let program = tests::fixture();
    let program_id = program.summary.id.clone();
    let lesson_id = program.modules[0].lessons[0].id.clone();
    let programs = LearningRepository::new(pool.clone());
    programs.create(&program).await?;
    programs
        .accept(&AcceptLearningProgramRequestDto {
            program_id: program_id.clone(),
            expected_revision: 0,
            title: program.summary.title,
        })
        .await?;
    Ok((program_id, lesson_id))
}

fn generated() -> GeneratedPracticalActivity {
    GeneratedPracticalActivity {
        title: "Plan a field survey of a tidal pool".into(),
        brief: "Choose sampling points, justify them and list what you would record.".into(),
        source_version_ids: vec![],
        rubric: vec![LearningRubricCriterion {
            id: id(),
            title: "Sampling".into(),
            description: "Justifies where and when to sample.".into(),
            max_points: 3,
        }],
        files: vec![GeneratedPracticalFile {
            path: "work/plan.txt".into(),
            role: PracticalFileRole::Starter,
            content: "Survey plan:\n".into(),
            editable: true,
        }],
    }
}

fn request(
    program_id: &str,
    lesson_id: &str,
    revision: i64,
) -> GenerateLearningPracticalActivityRequestDto {
    GenerateLearningPracticalActivityRequestDto {
        operation_id: id(),
        activity_id: id(),
        program_id: program_id.into(),
        lesson_id: lesson_id.into(),
        expected_program_revision: revision,
        kind: LearningPracticalActivityKind::Custom,
        learner_brief: "Practice planning observations.".into(),
        practice_mode: LearningPracticeMode::Practice,
        allowed_aids: vec![],
        runtime_profile_id: None,
        builtin_runtime: None,
        interview_target: None,
    }
}

#[tokio::test]
async fn activity_and_simulation_operations_replay_only_the_identical_request() -> Result<()> {
    let pool = tests::pool().await?;
    let (program_id, lesson_id) = program(&pool).await?;
    let revision = LearningRepository::new(pool.clone())
        .get(&program_id)
        .await?
        .summary
        .revision;
    let repository = LearningPracticalRepository::new(pool.clone());
    let generate = request(&program_id, &lesson_id, revision);

    assert!(repository
        .replay_activity_operation(
            &program_id,
            &generate.activity_id,
            &generate.operation_id,
            "generate"
        )
        .await?
        .is_none());
    repository
        .save_generated_activity(&generate, &generated(), "fixture-model", "generate")
        .await?;
    let replayed = repository
        .replay_activity_operation(
            &program_id,
            &generate.activity_id,
            &generate.operation_id,
            "generate",
        )
        .await?
        .expect("recorded generate operation");
    assert_eq!(replayed.workspace.program_id, program_id);
    assert!(replayed
        .workspace
        .activities
        .iter()
        .any(|a| a.id == generate.activity_id));
    let conflict = repository
        .replay_activity_operation(
            &program_id,
            &generate.activity_id,
            &generate.operation_id,
            "other",
        )
        .await;
    assert!(matches!(conflict, Err(AppError::InvalidInput(_))));
    let other_subject = repository
        .replay_activity_operation(&program_id, &id(), &generate.operation_id, "generate")
        .await;
    assert!(matches!(other_subject, Err(AppError::InvalidInput(_))));

    let session_id = id();
    let start = StartLearningSimulationRequestDto {
        operation_id: id(),
        session_id: session_id.clone(),
        program_id: program_id.clone(),
        activity_id: generate.activity_id.clone(),
        expected_activity_revision: 0,
        practice_session_id: None,
        learner_role: "Marine biologist".into(),
        counterpart_role: "Park ranger".into(),
    };
    assert!(repository
        .replay_simulation_operation(
            &program_id,
            &session_id,
            "start",
            &start.operation_id,
            "start"
        )
        .await?
        .is_none());
    repository.start_simulation(&start, "start").await?;
    let session = repository
        .replay_simulation_operation(
            &program_id,
            &session_id,
            "start",
            &start.operation_id,
            "start",
        )
        .await?
        .expect("recorded start operation");
    assert_eq!(session.id, session_id);
    assert_eq!(session.status, LearningSimulationStatus::Active);
    let wrong_kind = repository
        .replay_simulation_operation(
            &program_id,
            &session_id,
            "turn",
            &start.operation_id,
            "start",
        )
        .await;
    assert!(matches!(wrong_kind, Err(AppError::InvalidInput(_))));
    Ok(())
}

#[tokio::test]
async fn simulation_context_reads_the_saved_activity_and_rejects_unknown_ones() -> Result<()> {
    let pool = tests::pool().await?;
    let (program_id, lesson_id) = program(&pool).await?;
    let revision = LearningRepository::new(pool.clone())
        .get(&program_id)
        .await?
        .summary
        .revision;
    let repository = LearningPracticalRepository::new(pool.clone());
    let generate = request(&program_id, &lesson_id, revision);
    let activity = generated();
    repository
        .save_generated_activity(&generate, &activity, "fixture-model", "generate")
        .await?;

    let (context, source_versions) = repository
        .simulation_generation_context(&program_id, &generate.activity_id)
        .await?;
    assert_eq!(context.title, activity.title);
    assert_eq!(context.brief, activity.brief);
    assert_eq!(context.rubric.len(), 1);
    assert_eq!(context.rubric[0].title, "Sampling");
    assert!(source_versions.is_empty());

    let unknown = repository
        .simulation_generation_context(&program_id, &id())
        .await;
    assert!(matches!(unknown, Err(AppError::NotFound(_))));
    let (other_program, _) = program(&pool).await?;
    let foreign = repository
        .simulation_generation_context(&other_program, &generate.activity_id)
        .await;
    assert!(matches!(foreign, Err(AppError::NotFound(_))));
    let malformed = repository
        .simulation_generation_context("not-a-uuid", &generate.activity_id)
        .await;
    assert!(matches!(malformed, Err(AppError::InvalidInput(_))));
    let malformed = repository
        .simulation_generation_context(&program_id, "not-a-uuid")
        .await;
    assert!(matches!(malformed, Err(AppError::InvalidInput(_))));
    Ok(())
}
