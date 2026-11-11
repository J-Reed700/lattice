#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::practical_dto::LearningPracticalPublicFileRole;
use crate::practice::coding::problem::{CodingTask, ProblemDifficulty};
use crate::practice::coding::proof::fakes::{sample_problem, FakeHarness};
use crate::practice::test_support::fixture;
use serde_json::json;

fn build_request(operation_id: &str, program_id: &str) -> StartCodingBuildRequestDto {
    StartCodingBuildRequestDto {
        operation_id: operation_id.to_owned(),
        program_id: program_id.to_owned(),
        lesson_id: None,
        task: CodingTask::Problem,
        language: "python".into(),
        difficulty: ProblemDifficulty::Easy,
        focus: None,
    }
}

fn save_of<'a>(job_id: &'a str, program_id: &'a str, lesson_id: &'a str) -> ProblemSave<'a> {
    ProblemSave {
        job_id,
        program_id,
        lesson_id,
        focus: Some("arrays"),
        language: "python",
        model: "scripted-model",
    }
}

fn file(
    path: &str,
    role: PracticalFileRole,
    editable: bool,
    content: &str,
) -> GeneratedPracticalFile {
    GeneratedPracticalFile {
        path: path.to_owned(),
        role,
        content: content.to_owned(),
        editable,
    }
}

fn problem_json() -> String {
    serde_json::to_string(&sample_problem()).unwrap()
}

async fn generate(
    repository: &LearningPracticalRepository,
    program_id: &str,
    lesson_id: &str,
    kind: LearningPracticalActivityKind,
    files: Vec<GeneratedPracticalFile>,
) -> Result<()> {
    let revision = repository.program_revision(program_id).await?;
    let request = GenerateLearningPracticalActivityRequestDto {
        operation_id: Uuid::new_v4().to_string(),
        activity_id: Uuid::new_v4().to_string(),
        program_id: program_id.to_owned(),
        lesson_id: lesson_id.to_owned(),
        expected_program_revision: revision,
        kind,
        learner_brief: String::new(),
        practice_mode: crate::dto::LearningPracticeMode::Practice,
        allowed_aids: Vec::new(),
        runtime_profile_id: None,
        builtin_runtime: Some(LearningBuiltinRuntime::Python),
        interview_target: None,
    };
    let activity = GeneratedPracticalActivity {
        title: "Pair with a given sum".into(),
        brief: "Find the pair.".into(),
        source_version_ids: Vec::new(),
        rubric: Vec::new(),
        files,
    };
    repository
        .save_generated_activity(&request, &activity, "scripted-model", "hash")
        .await
        .map(|_| ())
}

#[test]
fn a_build_that_is_stopped_midway_is_requeued() {
    let config = practice_job_config();
    assert_eq!(PRACTICE_BUILD, "learning.coding_build");
    assert_eq!(PRACTICE_BUILD_STEPS, 3);
    assert!(matches!(config.recovery, RecoveryPolicy::Requeue));
}

#[tokio::test]
async fn queueing_a_build_twice_with_one_operation_id_is_one_job() {
    let (pool, program_id, _, _) = fixture().await.unwrap();
    let operation = Uuid::new_v4().to_string();
    let first = queue_coding_build(
        &Database::from(pool.clone()),
        &build_request(&operation, &program_id),
    )
    .await
    .unwrap();
    let again = queue_coding_build(
        &Database::from(pool.clone()),
        &build_request(&operation, &program_id),
    )
    .await
    .unwrap();
    assert_eq!(first, again);

    let job = JobStore::new(pool.clone()).get(&first).await.unwrap();
    assert_eq!(job.kind, PRACTICE_BUILD);
    assert_eq!(job.subject_id.as_deref(), Some(program_id.as_str()));
    assert_eq!(job.progress_total, PRACTICE_BUILD_STEPS);

    let other = queue_coding_build(
        &Database::from(pool.clone()),
        &build_request(&Uuid::new_v4().to_string(), &program_id),
    )
    .await
    .unwrap();
    assert_ne!(first, other);
}

#[test]
fn a_programs_build_request_names_its_kind_form_and_options() {
    let request = practice_build_request(&build_request("operation", "program-1")).unwrap();
    assert_eq!(request.operation_id, "operation");
    assert_eq!(request.program_id, "program-1");
    assert_eq!(request.lesson_id, None);
    assert_eq!(request.kind, KindId::new("program"));
    assert_eq!(request.form, Form::Build);
    assert_eq!(
        request.options,
        json!({"language": "python", "difficulty": "easy", "focus": null})
    );
}

#[test]
fn a_bug_fix_is_a_repair_and_keeps_its_focus() {
    let mut asked = build_request("operation", "program-1");
    asked.task = CodingTask::BugFix;
    asked.focus = Some("arrays".into());
    asked.lesson_id = Some("lesson-1".into());
    let request = practice_build_request(&asked).unwrap();
    assert_eq!(request.form, Form::Repair);
    assert_eq!(request.lesson_id.as_deref(), Some("lesson-1"));
    assert_eq!(request.options["focus"], json!("arrays"));
}

#[tokio::test]
async fn the_queued_job_carries_the_kind_in_its_payload() {
    let (pool, program_id, _, _) = fixture().await.unwrap();
    let id = queue_coding_build(
        &Database::from(pool.clone()),
        &build_request(&Uuid::new_v4().to_string(), &program_id),
    )
    .await
    .unwrap();
    let job = JobStore::new(pool.clone()).get(&id).await.unwrap();
    assert_eq!(job.requested["kind"], json!("program"));
    assert_eq!(job.requested["form"], json!("build"));
    assert_eq!(job.requested["programId"], json!(program_id));
    assert_eq!(job.requested["options"]["language"], json!("python"));
}

#[tokio::test]
async fn an_operation_id_reused_for_another_request_is_refused() {
    let (pool, program_id, _, _) = fixture().await.unwrap();
    let operation = Uuid::new_v4().to_string();
    queue_coding_build(
        &Database::from(pool.clone()),
        &build_request(&operation, &program_id),
    )
    .await
    .unwrap();
    let mut different = build_request(&operation, &program_id);
    different.focus = Some("hashing".into());
    assert!(
        queue_coding_build(&Database::from(pool.clone()), &different)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn the_build_saves_a_coding_problem_activity_with_a_private_problem_file() {
    let (pool, program_id, lesson_id, outcome_id) = fixture().await.unwrap();
    let job_id = Uuid::new_v4().to_string();
    let activity_id = save_problem(
        &Database::from(pool.clone()),
        &save_of(&job_id, &program_id, &lesson_id),
        &sample_problem(),
        &FakeHarness,
    )
    .await
    .unwrap();

    let repository = LearningPracticalRepository::new(pool.clone());
    let stored = repository
        .stored_problem(&program_id, &activity_id)
        .await
        .unwrap();
    assert_eq!(stored.problem, sample_problem());
    assert_eq!(stored.lesson_id, lesson_id);
    assert_eq!(stored.outcome_ids, [outcome_id]);
    assert_eq!(stored.builtin_runtime, Some(LearningBuiltinRuntime::Python));
    assert_eq!(
        stored.builtin_runtime_version.as_deref(),
        Some(LearningBuiltinRuntime::Python.version())
    );

    let workspace = repository.workspace(&program_id).await.unwrap().workspace;
    let activity = workspace
        .activities
        .iter()
        .find(|activity| activity.id == activity_id)
        .unwrap();
    assert_eq!(activity.kind, LearningPracticalActivityKind::CodingProblem);
    assert_eq!(activity.title, "Pair with a given sum");
    assert_eq!(activity.generator_model, "scripted-model");
    assert!(activity
        .files
        .iter()
        .all(|file| file.path != PROBLEM_FILE_PATH && file.path != "checks.py"));
    let starter = activity
        .files
        .iter()
        .find(|file| file.path == SOLUTION_PATH)
        .unwrap();
    assert_eq!(starter.role, LearningPracticalPublicFileRole::Starter);
    assert!(starter.editable);
    assert_eq!(starter.content, sample_problem().spec.starter);
}

#[tokio::test]
async fn a_build_that_runs_again_after_saving_finds_its_own_activity() {
    let (pool, program_id, lesson_id, _) = fixture().await.unwrap();
    let job_id = Uuid::new_v4().to_string();
    let save = save_of(&job_id, &program_id, &lesson_id);
    let first = save_problem(
        &Database::from(pool.clone()),
        &save,
        &sample_problem(),
        &FakeHarness,
    )
    .await
    .unwrap();
    let again = save_problem(
        &Database::from(pool.clone()),
        &save,
        &sample_problem(),
        &FakeHarness,
    )
    .await
    .unwrap();
    assert_eq!(first, again);

    let workspace = LearningPracticalRepository::new(pool)
        .workspace(&program_id)
        .await
        .unwrap()
        .workspace;
    let saved = workspace
        .activities
        .iter()
        .filter(|activity| activity.kind == LearningPracticalActivityKind::CodingProblem)
        .count();
    assert_eq!(saved, 1);
}

#[tokio::test]
async fn the_lesson_of_a_build_is_the_one_asked_for_then_the_current_one() {
    let (pool, program_id, lesson_id, _) = fixture().await.unwrap();
    let repository = LearningPracticalRepository::new(pool);
    let asked = repository
        .build_lesson(&program_id, Some(&lesson_id))
        .await
        .unwrap();
    assert_eq!(asked.id, lesson_id);
    let fallback = repository.build_lesson(&program_id, None).await.unwrap();
    assert!(!fallback.title.is_empty());
    assert!(repository
        .build_lesson(&program_id, Some(&Uuid::new_v4().to_string()))
        .await
        .is_err());
}

#[tokio::test]
async fn a_problem_file_is_refused_unless_it_is_a_private_read_only_check_of_a_coding_problem() {
    let (pool, program_id, lesson_id, _) = fixture().await.unwrap();
    let repository = LearningPracticalRepository::new(pool);
    let evaluator = file("checks.py", PracticalFileRole::Check, false, "# evaluator");
    let starter = file(SOLUTION_PATH, PracticalFileRole::Starter, true, "pass");

    let as_starter = file(
        PROBLEM_FILE_PATH,
        PracticalFileRole::Starter,
        true,
        &problem_json(),
    );
    let as_reference = file(
        PROBLEM_FILE_PATH,
        PracticalFileRole::Reference,
        false,
        &problem_json(),
    );
    let editable = file(
        PROBLEM_FILE_PATH,
        PracticalFileRole::Check,
        true,
        &problem_json(),
    );
    let garbled = file(
        PROBLEM_FILE_PATH,
        PracticalFileRole::Check,
        false,
        "{ not a problem",
    );
    for bad in [as_starter, as_reference, editable, garbled] {
        let refused = generate(
            &repository,
            &program_id,
            &lesson_id,
            LearningPracticalActivityKind::CodingProblem,
            vec![starter.clone(), evaluator.clone(), bad],
        )
        .await;
        assert!(refused.is_err());
    }

    let valid = file(
        PROBLEM_FILE_PATH,
        PracticalFileRole::Check,
        false,
        &problem_json(),
    );
    let other_kind = generate(
        &repository,
        &program_id,
        &lesson_id,
        LearningPracticalActivityKind::CodeLab,
        vec![starter.clone(), evaluator.clone(), valid.clone()],
    )
    .await;
    assert!(other_kind.is_err());

    let missing = generate(
        &repository,
        &program_id,
        &lesson_id,
        LearningPracticalActivityKind::CodingProblem,
        vec![starter.clone(), evaluator.clone()],
    )
    .await;
    assert!(missing.is_err());

    generate(
        &repository,
        &program_id,
        &lesson_id,
        LearningPracticalActivityKind::CodingProblem,
        vec![starter, evaluator, valid],
    )
    .await
    .unwrap();
}
