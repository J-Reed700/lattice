use super::{
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
    let root = super::practical_repository::run_workspace(&run_id);
    let marker = super::practical_repository::workspace_owner_marker(&run_id);
    std::fs::create_dir_all(&root)?;
    let sentinel = root.join("user-data.txt");
    std::fs::write(&sentinel, "preserve me")?;

    // This is the path taken when a container runner finds a pre-existing
    // root and returns a security error before it can materialize anything.
    super::practical_repository::cleanup_invocation_workspace(&run_id, false, false);
    assert_eq!(std::fs::read_to_string(&sentinel)?, "preserve me");

    // A caller-chosen run UUID cannot cause deletion of an existing folder.
    super::practical_repository::remove_owned_workspace(&run_id, &token);
    assert_eq!(std::fs::read_to_string(&sentinel)?, "preserve me");

    std::fs::write(&marker, id())?;
    super::practical_repository::remove_owned_workspace(&run_id, &token);
    assert_eq!(std::fs::read_to_string(&sentinel)?, "preserve me");

    std::fs::write(&marker, &token)?;
    super::practical_repository::remove_owned_workspace(&run_id, &token);
    assert!(!root.exists());
    assert!(!marker.exists());
    Ok(())
}
use crate::shared::error::{AppError, Result};
use sqlx::{Row, SqlitePool};

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn db(error: sqlx::Error) -> AppError {
    AppError::Database(error.to_string())
}

async fn fixture() -> Result<(SqlitePool, String, String, String)> {
    let pool = super::tests::pool().await?;
    let program = super::tests::fixture();
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
    use super::embedded_runtime::LearningBuiltinRuntime;
    use super::lab_runtime::LearningLabFile;
    let (pool, program_id, lesson_id, _) = fixture().await?;
    let repository = LearningPracticalRepository::new(pool.clone());
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
    };
    let mut activity = generated();
    activity.files = vec![
        GeneratedPracticalFile {path:"solution.mjs".into(),role:PracticalFileRole::Starter,content:"export const add=(a,b)=>0;".into(),editable:true},
        GeneratedPracticalFile {path:"checks.mjs".into(),role:PracticalFileRole::Check,content:"import {add} from './solution.mjs'; if(add(2,3)!==5) throw new Error('PRIVATE_ASSERTION_EXPECTED_5'); console.log('All checks passed');".into(),editable:false},
    ];
    let workspace = repository
        .save_generated_activity(&request, &activity, "fixture-model", "builtin-create")
        .await?;
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
    let unrelated_workspace = super::practical_repository::run_workspace(&run.run_id);
    std::fs::create_dir_all(&unrelated_workspace)?;
    let unrelated_file = unrelated_workspace.join("user-data.txt");
    std::fs::write(&unrelated_file, "must not be removed by a builtin run")?;
    assert_eq!(
        repository.start_run(&run, "builtin-wrong").await?.status,
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
    let passed = repository.start_run(&run, "builtin-right").await?;
    assert_eq!(passed.status, LearningPracticalRunStatus::Passed);
    assert_eq!(
        passed.builtin_runtime,
        Some(LearningBuiltinRuntime::Javascript)
    );
    assert_eq!(
        repository.start_run(&run, "builtin-right").await?.id,
        passed.id
    );
    let reopened = LearningPracticalRepository::new(pool.clone())
        .workspace(&program_id)
        .await?;
    assert_eq!(reopened.runs.len(), 2);
    run.operation_id = id();
    run.run_id = id();
    run.learner_files.push(LearningLabFile {
        path: "checks.mjs".into(),
        content: "console.log('fake pass')".into(),
    });
    assert!(repository.start_run(&run, "replace-checks").await.is_err());
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
    let older = repository.workspace(&program_id).await?;
    assert!(!older.activities[0].runtime_available);
    assert_eq!(older.runs.len(), 2);
    run.operation_id = id();
    run.run_id = id();
    run.learner_files.pop();
    assert_eq!(
        repository.start_run(&run, "old-runtime").await?.status,
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
        preset: super::runtime_catalog::LearningRuntimePresetId::Csharp,
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

    let workspace = repository.workspace(&program_id).await?;
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
        .await?;
    assert_eq!(replay.activities.len(), 1);
    assert!(repository
        .save_generated_activity(&request, &generated(), "fixture-model", "different-payload")
        .await
        .is_err());
    Ok(())
}

#[tokio::test]
async fn practical_draft_is_revision_scoped_idempotent_and_rejects_private_files() -> Result<()> {
    use super::lab_runtime::LearningLabFile;
    let (pool, program_id, lesson_id, _) = fixture().await?;
    let programs = LearningRepository::new(pool.clone());
    let repository = LearningPracticalRepository::new(pool.clone());
    let (_, activity_id) = activity(&repository, &programs, &program_id, &lesson_id).await?;
    let scope = GetLearningPracticalDraftRequestDto {
        program_id: program_id.clone(), activity_id: activity_id.clone(), activity_revision: 0,
    };
    let initial = repository.get_draft(&scope).await?;
    assert_eq!(initial.draft_revision, 0);
    assert_eq!(initial.files, vec![LearningLabFile { path: "work/answer.txt".into(), content: "Write your answer here.\n".into() }]);

    let request = SaveLearningPracticalDraftRequestDto {
        operation_id: id(), program_id: program_id.clone(), activity_id: activity_id.clone(), activity_revision: 0,
        expected_draft_revision: 0, files: vec![LearningLabFile { path: "work/answer.txt".into(), content: "Learner's durable answer".into() }],
    };
    let saved = repository.save_draft(&request, "draft-payload-one").await?;
    assert_eq!(saved.draft_revision, 1);
    let replay = repository.save_draft(&request, "draft-payload-one").await?;
    assert_eq!(replay, saved);
    assert!(repository.save_draft(&request, "changed-payload").await.is_err());
    assert_eq!(repository.get_draft(&scope).await?.files[0].content, "Learner's durable answer");

    let mut stale = request.clone();
    stale.operation_id = id();
    stale.expected_draft_revision = 0;
    assert!(repository.save_draft(&stale, "stale-operation").await.is_err());

    let mut hidden = request.clone();
    hidden.operation_id = id();
    hidden.expected_draft_revision = 1;
    hidden.files.push(LearningLabFile { path: "checks/evaluate.txt".into(), content: "private evaluator fixture".into() });
    assert!(repository.save_draft(&hidden, "hidden-file").await.is_err());
    let mut traversal = request.clone();
    traversal.operation_id = id();
    traversal.expected_draft_revision = 1;
    traversal.files[0].path = "../answer.txt".into();
    assert!(repository.save_draft(&traversal, "path-traversal").await.is_err());

    let other_scope = GetLearningPracticalDraftRequestDto { program_id: id(), ..scope.clone() };
    assert!(repository.get_draft(&other_scope).await.is_err());
    sqlx::query("UPDATE learning_practical_activities SET revision=1 WHERE id=?")
        .bind(&activity_id).execute(&pool).await.map_err(db)?;
    assert!(repository.get_draft(&scope).await.is_err());
    let next_revision = repository.get_draft(&GetLearningPracticalDraftRequestDto { activity_revision: 1, ..scope }).await?;
    assert_eq!(next_revision.draft_revision, 0);
    assert_eq!(next_revision.files[0].content, "Write your answer here.\n");
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
    let updated = repository
        .save_runtime_profile(&update, "profile-update")
        .await?;
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
    assert_eq!(repository.recover_running_runs().await?, 1);
    let recovered = repository.run(&run_id).await?;
    assert_eq!(recovered.status, LearningPracticalRunStatus::Interrupted);
    assert!(recovered.completed_at.is_some());
    assert_eq!(repository.recover_running_runs().await?, 0);
    Ok(())
}
