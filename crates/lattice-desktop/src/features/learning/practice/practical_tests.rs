use crate::features::learning::{
    assessment_engine::LearningRubricCriterion,
    dto::{AcceptLearningProgramRequestDto, LearningPracticeMode},
    lab_runtime::{LearningContainerEngine, LearningLabLimits},
    practical_dto::*,
    practical_generation::{GeneratedPracticalActivity, GeneratedPracticalFile, PracticalFileRole},
    practical_repository::{LearningPracticalRepository, SimulationTurnWrite},
    repository::LearningRepository,
};

#[test]
fn workspace_cleanup_requires_a_matching_invocation_marker() -> Result<()> {
    let run_id = id();
    let token = id();
    let root = crate::features::learning::practical_runs::run_workspace(&run_id);
    let marker = crate::features::learning::practical_runs::workspace_owner_marker(&run_id);
    std::fs::create_dir_all(&root)?;
    let sentinel = root.join("user-data.txt");
    std::fs::write(&sentinel, "preserve me")?;

    // This is the path taken when a container runner finds a pre-existing
    // root and returns a security error before it can materialize anything.
    crate::features::learning::practical_runs::cleanup_invocation_workspace(&run_id, false, false);
    assert_eq!(std::fs::read_to_string(&sentinel)?, "preserve me");

    // A caller-chosen run UUID cannot cause deletion of an existing folder.
    crate::features::learning::practical_runs::remove_owned_workspace(&run_id, &token);
    assert_eq!(std::fs::read_to_string(&sentinel)?, "preserve me");

    std::fs::write(&marker, id())?;
    crate::features::learning::practical_runs::remove_owned_workspace(&run_id, &token);
    assert_eq!(std::fs::read_to_string(&sentinel)?, "preserve me");

    std::fs::write(&marker, &token)?;
    crate::features::learning::practical_runs::remove_owned_workspace(&run_id, &token);
    assert!(!root.exists());
    assert!(!marker.exists());
    Ok(())
}
use lattice_core::shared::error::{AppError, Result};
use sqlx::{Row, SqlitePool};

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

use crate::features::learning::persistence::db;

async fn fixture_on_pool(pool: SqlitePool) -> Result<(SqlitePool, String, String, String)> {
    let program = crate::features::learning::tests::fixture();
    let program_id = program.summary.id.clone();
    let module_id = program.modules[0].id.clone();
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
    let outcome_id = id();
    sqlx::query("INSERT INTO learning_outcome_definitions(id,program_id,module_id,lesson_id,title,description,ordinal,created_at) VALUES(?,?,?,NULL,?,?,0,?)")
        .bind(&outcome_id)
        .bind(&program_id)
        .bind(&module_id)
        .bind("Apply the central idea")
        .bind("Use the module idea in a practical setting.")
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&pool)
        .await
        .map_err(db)?;
    Ok((pool, program_id, lesson_id, outcome_id))
}

async fn fixture() -> Result<(SqlitePool, String, String, String)> {
    fixture_on_pool(crate::features::learning::tests::pool().await?).await
}

fn generated() -> GeneratedPracticalActivity {
    GeneratedPracticalActivity {
        title: "Investigate the system".into(),
        brief: "Create an artifact, explain the tradeoffs, and verify the result.".into(),
        source_version_ids: vec![],
        rubric: vec![
            LearningRubricCriterion {
                id: id(),
                title: "Reasoning".into(),
                description: "Explains the relevant tradeoffs.".into(),
                max_points: 2,
            },
            LearningRubricCriterion {
                id: id(),
                title: "Verification".into(),
                description: "Checks the result against the stated constraints.".into(),
                max_points: 2,
            },
        ],
        files: vec![
            GeneratedPracticalFile {
                path: "work/answer.txt".into(),
                role: PracticalFileRole::Starter,
                content: "Write your answer here.\n".into(),
                editable: true,
            },
            GeneratedPracticalFile {
                path: "checks/evaluate.txt".into(),
                role: PracticalFileRole::Check,
                content: "private evaluator fixture\n".into(),
                editable: false,
            },
        ],
    }
}

#[tokio::test]
async fn builtin_run_executes_real_code_preserves_attempts_and_protects_evaluators() -> Result<()> {
    use crate::features::learning::embedded_runtime::LearningBuiltinRuntime;
    use crate::features::learning::lab_runtime::LearningLabFile;
    let (pool, program_id, lesson_id, _) = fixture().await?;
    let repository = LearningPracticalRepository::new(pool.clone());
    let runs = crate::features::learning::practical_runs::PracticalRunRegistry::new();
    let background = lattice_core::shared::runtime::background::BackgroundTasks::default();
    let runtimes = crate::features::learning::embedded_runtime::BuiltinRuntimes::for_tests();
    let request = GenerateLearningPracticalActivityRequestDto {
        operation_id: id(),
        activity_id: id(),
        program_id: program_id.clone(),
        lesson_id,
        expected_program_revision: LearningRepository::new(pool.clone())
            .get(&program_id)
            .await?
            .summary
            .revision,
        kind: LearningPracticalActivityKind::CodeLab,
        learner_brief: "Implement addition.".into(),
        practice_mode: LearningPracticeMode::Practice,
        allowed_aids: vec![],
        runtime_profile_id: None,
        builtin_runtime: Some(LearningBuiltinRuntime::Javascript),
        interview_target: None,
    };
    let mut activity = generated();
    activity.files = vec![
        GeneratedPracticalFile {path:"solution.mjs".into(),role:PracticalFileRole::Starter,content:"export const add=(a,b)=>0;".into(),editable:true},
        GeneratedPracticalFile {path:"checks.mjs".into(),role:PracticalFileRole::Check,content:"import {add} from './solution.mjs'; if(add(2,3)!==5) throw new Error('PRIVATE_ASSERTION_EXPECTED_5'); console.log('All checks passed');".into(),editable:false},
    ];
    let workspace = repository
        .save_generated_activity(&request, &activity, "fixture-model", "builtin-create")
        .await?
        .with_capabilities(runtimes.capabilities(), Vec::new());
    let saved = workspace
        .activities
        .first()
        .ok_or_else(|| AppError::NotFound("builtin activity".into()))?;
    assert_eq!(saved.runtime_kind, LearningPracticalRuntimeKind::Builtin);
    assert!(saved.runtime_available);
    assert!(saved.runtime_engine.is_none());
    assert!(saved.runtime_profile_id.is_none());
    let public = serde_json::to_string(&workspace)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    assert!(!public.contains("PRIVATE_ASSERTION_EXPECTED_5"));
    let mut run = StartLearningPracticalRunRequestDto {
        operation_id: id(),
        run_id: id(),
        program_id: program_id.clone(),
        activity_id: request.activity_id.clone(),
        expected_activity_revision: 0,
        practice_session_id: None,
        learner_files: vec![LearningLabFile {
            path: "solution.mjs".into(),
            content: "export const add=(a,b)=>a-b;".into(),
        }],
    };
    let unrelated_workspace = crate::features::learning::practical_runs::run_workspace(&run.run_id);
    std::fs::create_dir_all(&unrelated_workspace)?;
    let unrelated_file = unrelated_workspace.join("user-data.txt");
    std::fs::write(&unrelated_file, "must not be removed by a builtin run")?;
    assert_eq!(
        crate::features::learning::practical_runs::start_run(
            &repository,
            &runs,
            &background,
            &runtimes,
            &run,
            "builtin-wrong"
        )
        .await?
        .status,
        LearningPracticalRunStatus::Failed
    );
    assert_eq!(
        std::fs::read_to_string(&unrelated_file)?,
        "must not be removed by a builtin run"
    );
    std::fs::remove_dir_all(&unrelated_workspace)?;
    run.operation_id = id();
    run.run_id = id();
    run.learner_files[0].content = "export const add=(a,b)=>a+b;".into();
    let passed = crate::features::learning::practical_runs::start_run(
        &repository,
        &runs,
        &background,
        &runtimes,
        &run,
        "builtin-right",
    )
    .await?;
    assert_eq!(passed.status, LearningPracticalRunStatus::Passed);
    assert_eq!(
        passed.builtin_runtime,
        Some(LearningBuiltinRuntime::Javascript)
    );
    assert_eq!(
        crate::features::learning::practical_runs::start_run(
            &repository,
            &runs,
            &background,
            &runtimes,
            &run,
            "builtin-right"
        )
        .await?
        .id,
        passed.id
    );
    let reopened = LearningPracticalRepository::new(pool.clone())
        .workspace(&program_id)
        .await?
        .with_capabilities(runtimes.capabilities(), Vec::new());
    assert_eq!(reopened.runs.len(), 2);
    run.operation_id = id();
    run.run_id = id();
    run.learner_files.push(LearningLabFile {
        path: "checks.mjs".into(),
        content: "console.log('fake pass')".into(),
    });
    assert!(crate::features::learning::practical_runs::start_run(
        &repository,
        &runs,
        &background,
        &runtimes,
        &run,
        "replace-checks"
    )
    .await
    .is_err());
    let mut conflicting = request.clone();
    conflicting.operation_id = id();
    conflicting.activity_id = id();
    conflicting.runtime_profile_id = Some(id());
    assert!(repository
        .save_generated_activity(&conflicting, &activity, "fixture", "two-runtimes")
        .await
        .is_err());
    // Upgrading the app must not silently reinterpret an old exercise under
    // a different embedded interpreter. Saved attempts remain readable.
    sqlx::query("UPDATE learning_practical_activities SET builtin_runtime_version='previous-runtime-v1' WHERE id=?")
        .bind(&request.activity_id).execute(&pool).await.map_err(db)?;
    let older = repository
        .workspace(&program_id)
        .await?
        .with_capabilities(runtimes.capabilities(), Vec::new());
    assert!(!older.activities[0].runtime_available);
    assert_eq!(older.runs.len(), 2);
    run.operation_id = id();
    run.run_id = id();
    run.learner_files.pop();
    assert_eq!(
        crate::features::learning::practical_runs::start_run(
            &repository,
            &runs,
            &background,
            &runtimes,
            &run,
            "old-runtime"
        )
        .await?
        .status,
        LearningPracticalRunStatus::RuntimeUnavailable
    );
    assert_eq!(
        repository.run(&passed.id).await?.status,
        LearningPracticalRunStatus::Passed
    );
    Ok(())
}

#[tokio::test]
async fn runtime_setup_validates_before_download_and_replays_committed_profile() -> Result<()> {
    let (pool, program_id, _, _) = fixture().await?;
    let repository = LearningPracticalRepository::new(pool);
    let request = PrepareLearningRuntimePresetRequestDto {
        operation_id: id(),
        program_id: program_id.clone(),
        profile_id: id(),
        preset: crate::features::learning::runtime_catalog::LearningRuntimePresetId::Csharp,
        engine: LearningContainerEngine::Docker,
    };
    assert!(repository
        .preflight_runtime_setup(&request, "setup")
        .await?
        .is_none());
    repository
        .save_runtime_profile(
            &SaveLearningRuntimeProfileRequestDto {
                operation_id: request.operation_id.clone(),
                program_id,
                profile_id: request.profile_id.clone(),
                expected_revision: None,
                name: "C#".into(),
                engine: request.engine,
                image_id: format!("sha256:{}", "a".repeat(64)),
                command: vec!["dotnet".into(), "--version".into()],
                limits: LearningLabLimits::default(),
            },
            "setup",
        )
        .await?;
    assert!(repository
        .preflight_runtime_setup(&request, "setup")
        .await?
        .is_some());
    assert!(repository
        .preflight_runtime_setup(&request, "different")
        .await
        .is_err());
    let mut invalid = request;
    invalid.operation_id = id();
    invalid.program_id = id();
    assert!(repository
        .preflight_runtime_setup(&invalid, "missing-program")
        .await
        .is_err());
    Ok(())
}

async fn activity(
    repository: &LearningPracticalRepository,
    programs: &LearningRepository,
    program_id: &str,
    lesson_id: &str,
) -> Result<(GenerateLearningPracticalActivityRequestDto, String)> {
    let activity_id = id();
    let request = GenerateLearningPracticalActivityRequestDto {
        operation_id: id(),
        activity_id: activity_id.clone(),
        program_id: program_id.into(),
        lesson_id: lesson_id.into(),
        expected_program_revision: programs.get(program_id).await?.summary.revision,
        kind: LearningPracticalActivityKind::Custom,
        learner_brief: "Practice applying the idea.".into(),
        practice_mode: LearningPracticeMode::Practice,
        allowed_aids: vec!["Frozen source notes".into()],
        runtime_profile_id: None,
        builtin_runtime: None,
        interview_target: None,
    };
    repository
        .save_generated_activity(&request, &generated(), "fixture-model", "activity-payload")
        .await?;
    Ok((request, activity_id))
}

#[tokio::test]
async fn practical_activity_replay_is_safe_and_hidden_checks_never_reach_the_renderer() -> Result<()>
{
    let (pool, program_id, lesson_id, _) = fixture().await?;
    let programs = LearningRepository::new(pool.clone());
    let repository = LearningPracticalRepository::new(pool.clone());
    let (request, activity_id) = activity(&repository, &programs, &program_id, &lesson_id).await?;
    let runtimes = crate::features::learning::embedded_runtime::BuiltinRuntimes::for_tests();

    let workspace = repository
        .workspace(&program_id)
        .await?
        .with_capabilities(runtimes.capabilities(), Vec::new());
    let saved = workspace
        .activities
        .iter()
        .find(|item| item.id == activity_id)
        .ok_or_else(|| AppError::NotFound("saved practical activity".into()))?;
    assert_eq!(saved.files.len(), 1);
    assert_eq!(saved.files[0].path, "work/answer.txt");
    let serialized =
        serde_json::to_string(saved).map_err(|error| AppError::Serialization(error.to_string()))?;
    assert!(!serialized.contains("private evaluator fixture"));
    let hidden: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM learning_practical_files WHERE activity_id=? AND role='check'",
    )
    .bind(&activity_id)
    .fetch_one(&pool)
    .await
    .map_err(db)?;
    assert_eq!(hidden, 1);

    let replay = repository
        .save_generated_activity(&request, &generated(), "fixture-model", "activity-payload")
        .await?
        .with_capabilities(runtimes.capabilities(), Vec::new());
    assert_eq!(replay.activities.len(), 1);
    assert!(repository
        .save_generated_activity(&request, &generated(), "fixture-model", "different-payload")
        .await
        .is_err());
    Ok(())
}

#[tokio::test]
async fn practical_draft_is_revision_scoped_idempotent_and_rejects_private_files() -> Result<()> {
    use crate::features::learning::lab_runtime::LearningLabFile;
    let (pool, program_id, lesson_id, _) = fixture().await?;
    let programs = LearningRepository::new(pool.clone());
    let repository = LearningPracticalRepository::new(pool.clone());
    let (_, activity_id) = activity(&repository, &programs, &program_id, &lesson_id).await?;
    let scope = GetLearningPracticalDraftRequestDto {
        program_id: program_id.clone(),
        activity_id: activity_id.clone(),
        activity_revision: 0,
    };
    let initial = repository.get_draft(&scope).await?;
    assert_eq!(initial.draft_revision, 0);
    assert_eq!(
        initial.files,
        vec![LearningLabFile {
            path: "work/answer.txt".into(),
            content: "Write your answer here.\n".into()
        }]
    );

    let request = SaveLearningPracticalDraftRequestDto {
        operation_id: id(),
        program_id: program_id.clone(),
        activity_id: activity_id.clone(),
        activity_revision: 0,
        expected_draft_revision: 0,
        files: vec![LearningLabFile {
            path: "work/answer.txt".into(),
            content: "Learner's durable answer".into(),
        }],
    };
    let saved = repository.save_draft(&request, "draft-payload-one").await?;
    assert_eq!(saved.draft_revision, 1);
    let replay = repository.save_draft(&request, "draft-payload-one").await?;
    assert_eq!(replay, saved);
    assert!(repository
        .save_draft(&request, "changed-payload")
        .await
        .is_err());
    let mut newer = request.clone();
    newer.operation_id = id();
    newer.expected_draft_revision = 1;
    newer.files[0].content = "newer version".into();
    let latest = repository.save_draft(&newer, "draft-payload-two").await?;
    assert_eq!(latest.draft_revision, 2);
    assert_eq!(
        repository.save_draft(&request, "draft-payload-one").await?,
        saved
    );
    assert_eq!(
        repository.get_draft(&scope).await?.files[0].content,
        "newer version"
    );

    let mut stale = request.clone();
    stale.operation_id = id();
    stale.expected_draft_revision = 0;
    assert!(repository
        .save_draft(&stale, "stale-operation")
        .await
        .is_err());

    let mut hidden = request.clone();
    hidden.operation_id = id();
    hidden.expected_draft_revision = 2;
    hidden.files.push(LearningLabFile {
        path: "checks/evaluate.txt".into(),
        content: "private evaluator fixture".into(),
    });
    assert!(repository.save_draft(&hidden, "hidden-file").await.is_err());
    let mut traversal = request.clone();
    traversal.operation_id = id();
    traversal.expected_draft_revision = 2;
    traversal.files[0].path = "../answer.txt".into();
    assert!(repository
        .save_draft(&traversal, "path-traversal")
        .await
        .is_err());

    let other_scope = GetLearningPracticalDraftRequestDto {
        program_id: id(),
        ..scope.clone()
    };
    assert!(repository.get_draft(&other_scope).await.is_err());
    sqlx::query("UPDATE learning_practical_activities SET revision=1 WHERE id=?")
        .bind(&activity_id)
        .execute(&pool)
        .await
        .map_err(db)?;
    assert!(repository.get_draft(&scope).await.is_err());
    let next_revision = repository
        .get_draft(&GetLearningPracticalDraftRequestDto {
            activity_revision: 1,
            ..scope
        })
        .await?;
    assert_eq!(next_revision.draft_revision, 0);
    assert_eq!(next_revision.files[0].content, "Write your answer here.\n");
    Ok(())
}

#[tokio::test]
async fn practical_draft_survives_closing_and_reopening_file_backed_database() -> Result<()> {
    use crate::features::learning::lab_runtime::LearningLabFile;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

    let directory = tempfile::tempdir().map_err(|error| AppError::Database(error.to_string()))?;
    let database_path = directory.path().join("learning-studio.sqlite");
    let options = || {
        SqliteConnectOptions::from_str(&format!("sqlite:{}", database_path.display()))
            .map(|options| options.create_if_missing(true).foreign_keys(true))
            .map_err(|error| AppError::Database(error.to_string()))
    };
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options()?)
        .await
        .map_err(db)?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|error| AppError::Database(error.to_string()))?;
    let (pool, program_id, lesson_id, _) = fixture_on_pool(pool).await?;
    let repository = LearningPracticalRepository::new(pool.clone());
    let programs = LearningRepository::new(pool.clone());
    let (_, activity_id) = activity(&repository, &programs, &program_id, &lesson_id).await?;
    let request = SaveLearningPracticalDraftRequestDto {
        operation_id: id(),
        program_id: program_id.clone(),
        activity_id: activity_id.clone(),
        activity_revision: 0,
        expected_draft_revision: 0,
        files: vec![LearningLabFile {
            path: "work/answer.txt".into(),
            content: "restored after application restart".into(),
        }],
    };
    let saved = repository.save_draft(&request, "restart-payload").await?;
    pool.close().await;

    let reopened = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options()?)
        .await
        .map_err(db)?;
    let restored = LearningPracticalRepository::new(reopened.clone())
        .get_draft(&GetLearningPracticalDraftRequestDto {
            program_id,
            activity_id,
            activity_revision: 0,
        })
        .await?;
    assert_eq!(restored.draft_revision, saved.draft_revision);
    assert_eq!(restored.files, saved.files);
    reopened.close().await;
    Ok(())
}

#[tokio::test]
async fn simulation_uses_cas_replays_mutations_and_records_only_uncertain_evidence() -> Result<()> {
    let (pool, program_id, lesson_id, outcome_id) = fixture().await?;
    let programs = LearningRepository::new(pool.clone());
    let repository = LearningPracticalRepository::new(pool.clone());
    let (_, activity_id) = activity(&repository, &programs, &program_id, &lesson_id).await?;
    let session_id = id();
    let start = StartLearningSimulationRequestDto {
        operation_id: id(),
        session_id: session_id.clone(),
        program_id: program_id.clone(),
        activity_id,
        expected_activity_revision: 0,
        practice_session_id: None,
        learner_role: "Engineer".into(),
        counterpart_role: "Reviewer".into(),
    };
    let created = repository.start_simulation(&start, "start-payload").await?;
    assert_eq!(created.status, LearningSimulationStatus::Active);
    assert_eq!(
        repository
            .start_simulation(&start, "start-payload")
            .await?
            .id,
        session_id
    );

    let turn = SendLearningSimulationTurnRequestDto {
        operation_id: id(),
        program_id: program_id.clone(),
        session_id: session_id.clone(),
        expected_revision: 0,
        content: "I would isolate the variable and record the constraint first.".into(),
    };
    let reply = SimulationTurnWrite {
        content: "Which observation would distinguish the two explanations?".into(),
        citations: vec![],
        model_name: "fixture-model".into(),
    };
    let advanced = repository
        .append_simulation_turn(&turn, &reply, "turn-payload")
        .await?;
    assert_eq!(advanced.revision, 1);
    assert_eq!(advanced.turns.len(), 2);
    assert_eq!(
        repository
            .append_simulation_turn(&turn, &reply, "turn-payload")
            .await?
            .turns
            .len(),
        2
    );
    let stale = SendLearningSimulationTurnRequestDto {
        operation_id: id(),
        ..turn.clone()
    };
    assert!(repository
        .append_simulation_turn(&stale, &reply, "stale-payload")
        .await
        .is_err());

    let submitted = repository
        .finish_simulation(
            &FinishLearningSimulationRequestDto {
                operation_id: id(),
                program_id: program_id.clone(),
                session_id: session_id.clone(),
                expected_revision: 1,
            },
            &SimulationTurnWrite {
                content: "The response identifies a useful next observation; human review remains required."
                    .into(),
                citations: vec![],
                model_name: "fixture-model".into(),
            },
            "finish-payload",
        )
        .await?;
    assert_eq!(submitted.status, LearningSimulationStatus::Submitted);
    assert_eq!(submitted.turns.len(), 3);
    let evidence = sqlx::query("SELECT outcome_id,result,score FROM learning_evidence_events WHERE program_id=? AND source_kind='simulation'")
        .bind(&program_id)
        .fetch_one(&pool)
        .await
        .map_err(db)?;
    assert_eq!(evidence.get::<String, _>("outcome_id"), outcome_id);
    assert_eq!(evidence.get::<String, _>("result"), "uncertain");
    assert_eq!(evidence.get::<Option<f64>, _>("score"), None);
    Ok(())
}

#[tokio::test]
async fn runtime_profiles_use_cas_and_restart_recovery_marks_runs_interrupted() -> Result<()> {
    let (pool, program_id, lesson_id, _) = fixture().await?;
    let programs = LearningRepository::new(pool.clone());
    let repository = LearningPracticalRepository::new(pool.clone());
    let profile_id = id();
    let create = SaveLearningRuntimeProfileRequestDto {
        operation_id: id(),
        program_id: program_id.clone(),
        profile_id: profile_id.clone(),
        expected_revision: None,
        name: "Pinned local runtime".into(),
        engine: LearningContainerEngine::Docker,
        image_id: format!("sha256:{}", "a".repeat(64)),
        command: vec!["python".into(), "-I".into(), "checks/run.py".into()],
        limits: LearningLabLimits::default(),
    };
    repository
        .save_runtime_profile(&create, "profile-create")
        .await?;
    repository
        .save_runtime_profile(&create, "profile-create")
        .await?;
    let activity_id = id();
    let generated_request = GenerateLearningPracticalActivityRequestDto {
        operation_id: id(),
        activity_id: activity_id.clone(),
        program_id: program_id.clone(),
        lesson_id: lesson_id.clone(),
        expected_program_revision: programs.get(&program_id).await?.summary.revision,
        kind: LearningPracticalActivityKind::CodeLab,
        learner_brief: "Complete and verify the contained exercise.".into(),
        practice_mode: LearningPracticeMode::Demonstrate,
        allowed_aids: vec![],
        runtime_profile_id: Some(profile_id.clone()),
        builtin_runtime: None,
        interview_target: None,
    };
    repository
        .save_generated_activity(
            &generated_request,
            &generated(),
            "fixture-model",
            "contained-activity",
        )
        .await?;
    let update = SaveLearningRuntimeProfileRequestDto {
        operation_id: id(),
        expected_revision: Some(0),
        name: "Pinned local runtime v2".into(),
        engine: LearningContainerEngine::Podman,
        image_id: format!("sha256:{}", "b".repeat(64)),
        ..create.clone()
    };
    let snapshot = repository
        .save_runtime_profile(&update, "profile-update")
        .await?;
    let docker = crate::features::learning::lab_runtime::LearningLabRuntimeCapability {
        engine: LearningContainerEngine::Docker,
        available: true,
        version: None,
        reason: None,
    };
    let updated = snapshot
        .clone()
        .with_capabilities(Vec::new(), vec![docker.clone()]);
    assert_eq!(
        updated
            .runtime_profiles
            .iter()
            .find(|profile| profile.id == profile_id)
            .map(|profile| profile.revision),
        Some(1)
    );
    let frozen = updated
        .activities
        .iter()
        .find(|activity| activity.id == activity_id)
        .ok_or_else(|| AppError::NotFound("frozen practical activity".into()))?;
    assert_eq!(frozen.runtime_engine, Some(LearningContainerEngine::Docker));
    assert_eq!(
        frozen.runtime_image_id,
        Some(format!("sha256:{}", "a".repeat(64)))
    );
    assert_eq!(
        frozen.runtime_command,
        Some(vec!["python".into(), "-I".into(), "checks/run.py".into()])
    );
    // Availability follows the activity's frozen engine, even after its
    // profile is edited to select another engine.
    assert!(frozen.runtime_available);
    let unavailable = snapshot.clone().with_capabilities(
        Vec::new(),
        vec![
            crate::features::learning::lab_runtime::LearningLabRuntimeCapability {
                available: false,
                reason: Some("Docker is offline".into()),
                ..docker.clone()
            },
        ],
    );
    assert!(!unavailable.activities[0].runtime_available);
    assert_eq!(
        unavailable.activities[0]
            .runtime_unavailable_reason
            .as_deref(),
        Some("Docker is offline")
    );
    let mut disabled = snapshot;
    disabled
        .workspace
        .runtime_profiles
        .iter_mut()
        .for_each(|profile| profile.enabled = false);
    let disabled = disabled.with_capabilities(Vec::new(), vec![docker]);
    assert!(!disabled.activities[0].runtime_available);
    assert_eq!(
        disabled.activities[0].runtime_unavailable_reason.as_deref(),
        Some("The frozen runtime profile is disabled.")
    );
    let stale = SaveLearningRuntimeProfileRequestDto {
        operation_id: id(),
        ..update
    };
    assert!(repository
        .save_runtime_profile(&stale, "profile-stale")
        .await
        .is_err());

    let run_id = id();
    sqlx::query("INSERT INTO learning_practical_runs(id,program_id,activity_id,activity_revision,activity_snapshot_json,practice_session_id,operation_id,payload_hash,engine,image_id,status,learner_files_json,created_at) VALUES(?,?,?,?,?,NULL,?,?,NULL,NULL,'running','[]',?)")
        .bind(&run_id)
        .bind(&program_id)
        .bind(&activity_id)
        .bind(0_i64)
        .bind("{}")
        .bind(id())
        .bind("recovery-payload")
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&pool)
        .await
        .map_err(db)?;
    assert_eq!(
        crate::features::learning::practical_runs::recover_running_runs(&repository).await?,
        1
    );
    let recovered = repository.run(&run_id).await?;
    assert_eq!(recovered.status, LearningPracticalRunStatus::Interrupted);
    assert!(recovered.completed_at.is_some());
    assert_eq!(
        crate::features::learning::practical_runs::recover_running_runs(&repository).await?,
        0
    );
    Ok(())
}

#[tokio::test]
async fn interview_activity_with_plan_roundtrips_and_hides_internal_plan_file() -> Result<()> {
    let (pool, program_id, lesson_id, _) = fixture().await?;
    let repository = LearningPracticalRepository::new(pool.clone());
    let programs = LearningRepository::new(pool.clone());
    let research_ids = crate::features::learning::interview_research::research_interview_target(
        &TestResearchLLM(serde_json::json!({"needs":[]})),
        &crate::features::web::mocks::MockWebService::new(),
        &crate::features::learning::source_library::LearningSourceLibraryRepository::new(
            pool.clone(),
        ),
        &program_id,
        &id(),
        "Streaming systems",
        "Prepare for Acme",
        &LearningInterviewTargetDto {
            company_context: Some(
                "Acme role focuses on streaming pipelines and backpressure.".into(),
            ),
            ..Default::default()
        },
    )
    .await?;

    let plan = LearningInterviewPlanDto {
        company_name: "Acme Corp".into(),
        company_url: Some("https://acme.example.com".into()),
        target_role: "Staff Backend Engineer".into(),
        round_kind: LearningInterviewRoundKind::TechnicalDeepDive,
        company_summary: "Acme Corp builds high-throughput streaming pipelines.".into(),
        learner_role: "Candidate (Staff Backend Engineer)".into(),
        counterpart_role: "Interviewer at Acme Corp".into(),
        researched_source_version_ids: research_ids.clone(),
        questions: vec![LearningInterviewQuestionDto {
            id: id(),
            round: LearningInterviewRoundKind::TechnicalDeepDive,
            question: "How do you handle backpressure in event streams?".into(),
            company_rationale: "Relevant to Acme streaming architecture.".into(),
            follow_up_probes: vec!["What buffering strategies do you apply?".into()],
            evaluation_signals: vec!["Understands queue limits and drop policies.".into()],
        }],
    };

    let plan_json =
        serde_json::to_string_pretty(&plan).map_err(|e| AppError::Serialization(e.to_string()))?;

    let activity_id = id();
    let request = GenerateLearningPracticalActivityRequestDto {
        operation_id: id(),
        activity_id: activity_id.clone(),
        program_id: program_id.clone(),
        lesson_id: lesson_id.clone(),
        expected_program_revision: programs.get(&program_id).await?.summary.revision,
        kind: LearningPracticalActivityKind::Interview,
        learner_brief: "Practice technical deep dive interview for Acme Corp.".into(),
        practice_mode: LearningPracticeMode::Practice,
        allowed_aids: vec![],
        runtime_profile_id: None,
        builtin_runtime: None,
        interview_target: Some(LearningInterviewTargetDto {
            company_name: Some("Acme Corp".into()),
            company_url: Some("https://acme.example.com".into()),
            target_role: Some("Staff Backend Engineer".into()),
            company_context: None,
            round_kind: Some(LearningInterviewRoundKind::TechnicalDeepDive),
        }),
    };

    let mut generated_act = generated();
    generated_act.source_version_ids = research_ids.clone();
    generated_act.files = vec![GeneratedPracticalFile {
        path: crate::features::learning::practice::practical_generation::INTERVIEW_PLAN_FILE_PATH
            .into(),
        role: PracticalFileRole::Reference,
        content: plan_json,
        editable: false,
    }];

    let workspace = repository
        .save_generated_activity(
            &request,
            &generated_act,
            "fixture-model",
            "interview-activity",
        )
        .await?;

    let saved = workspace
        .workspace
        .activities
        .iter()
        .find(|a| a.id == activity_id)
        .ok_or_else(|| AppError::NotFound("Interview activity not found in workspace".into()))?;

    // 1. Verify interview_plan round-tripped on LearningPracticalActivityDto
    assert_eq!(saved.interview_plan.as_ref(), Some(&plan));

    // 2. Verify interview/plan.json is hidden from public files
    assert!(!saved.files.iter().any(|f| f.path
        == crate::features::learning::practice::practical_generation::INTERVIEW_PLAN_FILE_PATH));

    // 3. Verify interview_plan is provided in simulation_generation_context
    let (sim_context, _sources) = repository
        .simulation_generation_context(&program_id, &activity_id)
        .await?;
    assert_eq!(sim_context.interview_plan.as_ref(), Some(&plan));

    let start = StartLearningSimulationRequestDto {
        operation_id: id(),
        session_id: id(),
        program_id: program_id.clone(),
        activity_id: activity_id.clone(),
        expected_activity_revision: 0,
        practice_session_id: None,
        learner_role: plan.learner_role.clone(),
        counterpart_role: plan.counterpart_role.clone(),
    };
    let session = repository
        .start_simulation(&start, "start-interview")
        .await?;
    assert_eq!(session.turns.len(), 1);
    assert_eq!(
        session.turns[0].speaker,
        LearningSimulationSpeaker::Counterpart
    );
    assert_eq!(session.turns[0].content, plan.questions[0].question);
    let replay = repository
        .start_simulation(&start, "start-interview")
        .await?;
    assert_eq!(replay.turns.len(), 1);
    assert_eq!(replay.turns[0].id, session.turns[0].id);
    // A later revision or retirement must not change a running interview.
    sqlx::query("UPDATE learning_practical_activities SET brief='Changed brief',revision=1,status='retired' WHERE id=?")
        .bind(&activity_id).execute(&pool).await.map_err(db)?;
    let (frozen, _) = repository
        .simulation_session_generation_context(&program_id, &session.id)
        .await?;
    assert_eq!(frozen.brief, generated_act.brief);
    assert_eq!(frozen.interview_plan.as_ref(), Some(&plan));

    use crate::features::learning::{pack_repository::LearningPackRepository, portability_dto::*};
    let directory = tempfile::tempdir()?;
    let packs = LearningPackRepository::new(
        pool.clone(),
        directory.path().join("packs"),
        directory.path().join("app"),
    );
    let exported = packs
        .export(&ExportLearningPackRequestDto {
            operation_id: id(),
            program_id: program_id.clone(),
            options: LearningPackExportOptionsDto {
                include_evidence: false,
                include_practical_artifacts: true,
                include_source_bodies: true,
                source_body_redistribution_confirmed: true,
            },
        })
        .await?;
    let preview_id = id();
    let preview = packs
        .preview(&PreviewLearningPackImportRequestDto {
            operation_id: id(),
            preview_id: preview_id.clone(),
            source_path: exported.exports[0].destination_path.clone(),
            conflict_policy: LearningPackConflictPolicy::CreateCopy,
        })
        .await?;
    assert!(preview.import_previews[0].can_apply);
    let imported = packs
        .apply(&ApplyLearningPackImportRequestDto {
            operation_id: id(),
            preview_id,
            expected_root_sha256: preview.import_previews[0].manifest.root_sha256.clone(),
        })
        .await?;
    let copied_program = &imported.imports[0].imported_program_id;
    let copied = repository.workspace(copied_program).await?;
    let copied_plan = copied.workspace.activities[0]
        .interview_plan
        .as_ref()
        .expect("Copied interview plan");
    assert_ne!(copied_plan.researched_source_version_ids, research_ids);
    assert_eq!(
        copied_plan.researched_source_version_ids,
        copied.workspace.activities[0].source_version_ids
    );
    let (copied_context, copied_sources) = repository
        .simulation_session_generation_context(copied_program, &copied.workspace.simulations[0].id)
        .await?;
    assert_eq!(copied_sources, copied_plan.researched_source_version_ids);
    assert_eq!(
        copied_context
            .interview_plan
            .as_ref()
            .expect("Frozen copied plan")
            .researched_source_version_ids,
        copied_sources
    );

    Ok(())
}

struct TestResearchLLM(serde_json::Value);

#[async_trait::async_trait]
impl lattice_core::application::ports::LLMPort for TestResearchLLM {
    async fn complete(
        &self,
        _request: &lattice_core::application::ports::llm_port::CompletionRequest,
    ) -> Result<lattice_core::application::ports::llm_port::CompletionResponse> {
        Ok(
            lattice_core::application::ports::llm_port::CompletionResponse::from_text(
                self.0.to_string(),
            ),
        )
    }
    fn model_name(&self) -> &str {
        "test-research-model"
    }
    fn max_context_tokens(&self) -> usize {
        131_072
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.split_whitespace().count()
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn interview_research_ingests_context_and_web_pages_idempotently() -> Result<()> {
    use crate::features::learning::practice::interview_research;
    use crate::features::learning::references::source_library::LearningSourceLibraryRepository;
    use crate::features::web::mocks::MockWebService;
    use lattice_core::application::contracts::web::{FetchUrlContentOutput, WebSearchResult};

    let (pool, program_id, _lesson_id, _) = fixture().await?;
    let library = LearningSourceLibraryRepository::new(pool.clone());
    let existing_count = library.workspace(&program_id).await?.sources.len();
    let llm = TestResearchLLM(
        serde_json::json!({"needs":[{"query":"Stripe distributed systems architecture","kind":"web"}]}),
    );

    let mock_web = MockWebService::new();
    mock_web.set_url_content(
        "https://stripe.com",
        FetchUrlContentOutput {
            url: "https://stripe.com".into(),
            title: Some("Stripe Payments Infrastructure".into()),
            content: "Stripe builds economic infrastructure for the internet with high availability and reliability.".into(),
            content_truncated: false,
            word_count: 15,
            fetch_time_ms: 0.0,
            content_type: None,
            from_cache: false,
            read_via: Default::default(),
        },
    );
    mock_web.set_url_content(
        "https://stripe.com/blog/distributed-systems",
        FetchUrlContentOutput {
            url: "https://stripe.com/blog/distributed-systems".into(),
            title: Some("Stripe Engineering Blog: Distributed Systems".into()),
            content:
                "Deep dive into idempotent request handling and transaction consensus at scale."
                    .into(),
            content_truncated: false,
            word_count: 14,
            fetch_time_ms: 0.0,
            content_type: None,
            from_cache: false,
            read_via: Default::default(),
        },
    );
    mock_web.set_search_results(vec![WebSearchResult {
        title: "Stripe Engineering Blog: Distributed Systems".into(),
        url: "https://stripe.com/blog/distributed-systems".into(),
        snippet: "Distributed systems deep dive".into(),
        published_date: None,
        source: None,
    }]);

    let target = LearningInterviewTargetDto {
        company_name: Some("Stripe".into()),
        company_url: Some("https://stripe.com".into()),
        target_role: Some("Staff Infrastructure Engineer".into()),
        company_context: Some(
            "We run global payment infrastructure requiring 99.999% uptime and zero duplicate transactions.".into(),
        ),
        round_kind: Some(LearningInterviewRoundKind::TechnicalDeepDive),
    };

    let op_id = id();
    let versions_first = interview_research::research_interview_target(
        &llm,
        &mock_web,
        &library,
        &program_id,
        &op_id,
        "Distributed Systems",
        "Design idempotent APIs",
        &target,
    )
    .await?;

    // Verify at least pasted context, seed URL, and research web pages were ingested
    assert_eq!(versions_first.len(), 3);

    // Verify sources in workspace
    let ws_after_first = library.workspace(&program_id).await?;
    assert_eq!(ws_after_first.sources.len(), existing_count + 3);

    // Previously captured pages survive a retry even if they disappear from search.
    mock_web.set_search_results(vec![]);

    // Call again with same op_id for idempotent retry
    let versions_second = interview_research::research_interview_target(
        &llm,
        &mock_web,
        &library,
        &program_id,
        &op_id,
        "Distributed Systems",
        "Design idempotent APIs",
        &target,
    )
    .await?;

    // Verify deterministic/idempotent version IDs returned on replay
    assert_eq!(versions_first, versions_second);

    // Verify no duplicate sources were added to library workspace
    let ws_after_second = library.workspace(&program_id).await?;
    assert_eq!(ws_after_first.sources.len(), ws_after_second.sources.len());

    mock_web.set_search_results(vec![WebSearchResult {
        title: "Stripe engineering".into(),
        url: "https://stripe.com/blog/distributed-systems".into(),
        snippet: String::new(),
        published_date: None,
        source: None,
    }]);
    let reused = interview_research::research_interview_target(
        &llm,
        &mock_web,
        &library,
        &program_id,
        &id(),
        "Distributed Systems",
        "Design idempotent APIs",
        &target,
    )
    .await?;
    assert_eq!(reused, versions_first);
    assert_eq!(
        library.workspace(&program_id).await?.sources.len(),
        existing_count + 3
    );
    let provenance = library.added_by_version(&program_id).await?;
    assert_eq!(
        versions_first
            .iter()
            .filter(|version| provenance.get(*version)
                == Some(&lattice_core::domain::learning::claim_review::SourceAddedBy::User))
            .count(),
        2
    );
    let mut changed = target;
    changed.company_context = Some("Changed request under the same operation ID".into());
    assert!(interview_research::research_interview_target(
        &llm,
        &mock_web,
        &library,
        &program_id,
        &op_id,
        "Distributed Systems",
        "Design idempotent APIs",
        &changed,
    )
    .await
    .is_err());

    Ok(())
}

#[tokio::test]
async fn interview_research_rejects_redirects_outside_structured_site_scope() -> Result<()> {
    use crate::features::learning::practice::interview_research;
    use crate::features::learning::source_library::LearningSourceLibraryRepository;
    use crate::features::web::mocks::MockWebService;
    use lattice_core::application::contracts::web::{FetchUrlContentOutput, WebSearchResult};
    let (pool, program_id, _, _) = fixture().await?;
    let library = LearningSourceLibraryRepository::new(pool);
    let existing_count = library.workspace(&program_id).await?.sources.len();
    let web = MockWebService::new();
    let address = "https://acme.example/engineering";
    web.set_search_results(vec![WebSearchResult {
        title: "Acme engineering".into(),
        url: address.into(),
        snippet: String::new(),
        published_date: None,
        source: None,
    }]);
    let llm = TestResearchLLM(serde_json::json!({"needs":[{
        "query":"site:acme.example Acme engineering", "kind":"web", "site":"acme.example/engineering"
    }]}));
    // Both the broad query scope and the narrower structured scope must hold.
    for resolved in [
        "https://unrelated.example/redirect",
        "https://acme.example/other",
    ] {
        web.set_url_content(
            address,
            FetchUrlContentOutput {
                url: resolved.into(),
                title: Some("Out-of-scope content".into()),
                content: "This page has left the requested engineering documentation scope.".into(),
                content_truncated: false,
                word_count: 10,
                fetch_time_ms: 0.0,
                content_type: None,
                from_cache: false,
                read_via: Default::default(),
            },
        );
        let versions = interview_research::research_interview_target(
            &llm,
            &web,
            &library,
            &program_id,
            &id(),
            "System design",
            "Interview at Acme",
            &LearningInterviewTargetDto {
                company_name: Some("Acme".into()),
                ..Default::default()
            },
        )
        .await?;
        assert!(versions.is_empty());
        assert_eq!(
            library.workspace(&program_id).await?.sources.len(),
            existing_count
        );
    }
    // A user may already have saved a URL that redirected elsewhere. Reusing
    // its active version must enforce the same scope as a fresh capture.
    let saved = interview_research::research_interview_target(
        &TestResearchLLM(serde_json::json!({"needs":[]})),
        &web,
        &library,
        &program_id,
        &id(),
        "System design",
        "Interview at Acme",
        &LearningInterviewTargetDto {
            company_name: Some("Acme".into()),
            company_url: Some(address.into()),
            ..Default::default()
        },
    )
    .await?;
    assert_eq!(saved.len(), 1);
    let researched = interview_research::research_interview_target(
        &llm,
        &web,
        &library,
        &program_id,
        &id(),
        "System design",
        "Interview at Acme",
        &LearningInterviewTargetDto {
            company_name: Some("Acme".into()),
            ..Default::default()
        },
    )
    .await?;
    assert!(researched.is_empty());
    assert_eq!(
        library.workspace(&program_id).await?.sources.len(),
        existing_count + 1
    );
    Ok(())
}
