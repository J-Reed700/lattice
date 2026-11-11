use super::fake::{case, sample_spec};
use super::*;
use crate::embedded_runtime::LearningBuiltinRuntime;
use crate::lab_runtime::{LearningLabExecutionResult, LearningLabRunStatus};
use crate::persistence::{db, now};
use crate::practical_dto::{LearningPracticalCheckStatus, StartLearningPracticalRunRequestDto};
use crate::practice::coding::problem::{CaseCompare, CaseStatus, CaseVisibility, ProblemCase};
use crate::practice::kinds::program::dto::CodingRunScope;
use crate::practice::practical_repository::{
    LearningPracticalRepository, PreparedExecution, PreparedHarness, PreparedRun,
};
use crate::practice::practical_runs::CodingRunStart;
use lattice_core::shared::error::Result;
use lattice_core::shared::hash::sha256_hex;
use serde_json::json;
use sqlx::SqlitePool;

const NONCE: &str = "0123456789abcdef0123456789abcdef";

fn line(
    id: &str,
    status: &str,
    actual: Option<&str>,
    printed: &str,
    error: Option<&str>,
) -> String {
    line_with(NONCE, id, status, actual, printed, error)
}

fn line_with(
    nonce: &str,
    id: &str,
    status: &str,
    actual: Option<&str>,
    printed: &str,
    error: Option<&str>,
) -> String {
    format!(
        "{nonce}{}\n",
        json!({
            "caseId": id,
            "status": status,
            "actual": actual,
            "printed": printed,
            "error": error,
            "elapsedMs": 4,
        })
    )
}

fn three_cases() -> Vec<ProblemCase> {
    vec![
        case("exact", ["[2, 7]", "9"], Some("[0, 1]"), false),
        ProblemCase {
            compare: CaseCompare::Unordered,
            ..case("unordered", ["[3, 2, 4]", "6"], Some("[1, 2]"), false)
        },
        ProblemCase {
            compare: CaseCompare::FloatWithin { tolerance: 0.01 },
            ..case("floats", ["[1.0, 2.0]", "3"], Some("[0.5, 0.5]"), true)
        },
    ]
}

#[test]
fn three_cases_read_back_in_case_order_with_their_lines() {
    let stdout = format!(
        "setup noise\n{}{}{}trailing noise\n",
        line("floats", "passed", Some("[0.5, 0.5]"), "", None),
        line("exact", "passed", Some("[0, 1]"), "adding", None),
        line("unordered", "failed", Some("[2, 1]"), "", None),
    );
    let results = PythonHarness::new().results(&three_cases(), &stdout, NONCE, RunEnd::Finished);
    let ids: Vec<&str> = results.iter().map(|r| r.case_id.as_str()).collect();
    assert_eq!(ids, ["exact", "unordered", "floats"]);
    assert_eq!(results[0].status, CaseStatus::Passed);
    assert_eq!(results[0].printed, "adding");
    assert_eq!(results[0].actual.as_deref(), Some("[0, 1]"));
    assert_eq!(results[0].elapsed_ms, Some(4));
    assert_eq!(results[1].status, CaseStatus::Failed);
    assert_eq!(results[2].status, CaseStatus::Passed);
}

#[test]
fn a_timeout_keeps_finished_cases_marks_the_running_one_and_leaves_the_rest_not_run() {
    let cases = vec![
        case("one", ["1", "1"], Some("2"), false),
        case("two", ["2", "2"], Some("4"), false),
        case("three", ["3", "3"], Some("6"), true),
        ProblemCase {
            scale: true,
            ..case("big", ["4", "4"], None, true)
        },
    ];
    let stdout = line("one", "passed", Some("2"), "", None);
    let results = PythonHarness::new().results(&cases, &stdout, NONCE, RunEnd::TimedOut);
    let statuses: Vec<CaseStatus> = results.iter().map(|r| r.status).collect();
    assert_eq!(
        statuses,
        [
            CaseStatus::Passed,
            CaseStatus::TimedOut,
            CaseStatus::NotRun,
            CaseStatus::NotRun
        ]
    );
    assert!(results[1].error.is_some());
    assert!(results[2].error.is_none());
}

#[test]
fn a_stopped_run_leaves_every_case_not_run() {
    let results = PythonHarness::new().results(&three_cases(), "", NONCE, RunEnd::Stopped);
    assert!(results.iter().all(|r| r.status == CaseStatus::NotRun));
    let finished = PythonHarness::new().results(&three_cases(), "", NONCE, RunEnd::Finished);
    assert!(finished.iter().all(|r| r.status == CaseStatus::NotRun));
}

#[test]
fn a_line_forged_with_another_nonce_is_ignored_and_stays_in_the_program_output() {
    let forged = line_with(
        "ffffffffffffffffffffffffffffffff",
        "exact",
        "passed",
        None,
        "",
        None,
    );
    let stdout = format!(
        "{forged}{}",
        line("unordered", "passed", Some("[1, 2]"), "", None)
    );
    let harness = PythonHarness::new();
    let results = harness.results(&three_cases(), &stdout, NONCE, RunEnd::Finished);
    assert_eq!(results[0].status, CaseStatus::NotRun);
    assert_eq!(results[1].status, CaseStatus::Passed);
    assert_eq!(harness.program_output(&stdout, NONCE), forged);
}

#[test]
fn program_output_drops_only_the_harness_lines() {
    let stdout = format!(
        "before\n{}between\n{}after",
        line("exact", "passed", None, "", None),
        line("unordered", "passed", None, "", None),
    );
    assert_eq!(
        PythonHarness::new().program_output(&stdout, NONCE),
        "before\nbetween\nafter"
    );
}

#[test]
fn lines_of_unknown_cases_are_ignored() {
    let stdout = line("somebody else", "passed", Some("1"), "", None);
    let results = PythonHarness::new().results(&three_cases(), &stdout, NONCE, RunEnd::Finished);
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|r| r.status == CaseStatus::NotRun));
}

#[test]
fn the_evaluator_holds_checks_and_cases_and_nothing_of_the_problem() {
    let files = PythonHarness::new()
        .evaluator(&sample_spec(), &three_cases(), NONCE)
        .unwrap();
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["checks.py", "cases.json"]);
    let cases: serde_json::Value = serde_json::from_str(&files[1].content).unwrap();
    assert_eq!(cases["function"], "add");
    assert_eq!(cases["nonce"], NONCE);
    assert_eq!(cases["cases"].as_array().map(Vec::len), Some(3));
    assert_eq!(cases["cases"][1]["compare"]["kind"], "unordered");
    assert_eq!(cases["cases"][2]["compare"]["tolerance"], 0.01);
    assert!(!files[0].content.contains(NONCE));
}

#[test]
fn a_harness_exists_for_python_only_and_nonces_are_fresh_hex() {
    assert!(harness_for("python").is_some());
    assert!(harness_for("cobol").is_none());
    let (first, second) = (new_nonce(), new_nonce());
    assert_eq!(first.len(), 32);
    assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(first, second);
}

struct Setup {
    repo: LearningPracticalRepository,
    program_id: String,
    activity_id: String,
    session_id: String,
}

async fn setup(pool: &SqlitePool) -> Result<Setup> {
    let repo = LearningPracticalRepository::new(pool.clone());
    let program = crate::tests::fixture();
    let program_id = program.summary.id.clone();
    let lesson_id = program.modules[0].lessons[0].id.clone();
    let programs = crate::repository::LearningRepository::new(pool.clone());
    programs.create(&program).await?;
    programs
        .accept(&crate::dto::AcceptLearningProgramRequestDto {
            program_id: program_id.clone(),
            expected_revision: 0,
            title: program.summary.title.clone(),
        })
        .await?;
    let activity_id = uuid::Uuid::new_v4().to_string();
    let session_id = uuid::Uuid::new_v4().to_string();
    let timestamp = now();
    sqlx::query(
        "INSERT INTO learning_practical_activities(id, program_id, lesson_id, predecessor_id, kind, title, brief, status, practice_mode, allowed_aids_json, outcome_ids_json, source_version_ids_json, rubric_json, runtime_kind, runtime_profile_id, runtime_engine, runtime_image_id, runtime_command_json, runtime_limits_json, generator_model, revision, created_at, updated_at, builtin_runtime, builtin_runtime_version) \
         VALUES(?, ?, ?, NULL, 'exercise', 'Add two numbers', 'Brief', 'ready', 'practice', '[]', '[]', '[]', '[]', 'none', NULL, NULL, NULL, NULL, NULL, 'fixture-model', 0, ?, ?, 'python', ?)",
    )
    .bind(&activity_id)
    .bind(&program_id)
    .bind(&lesson_id)
    .bind(timestamp)
    .bind(timestamp)
    .bind(LearningBuiltinRuntime::Python.version())
    .execute(pool)
    .await
    .map_err(db)?;
    let files = [
        ("solution.py", "starter", "def add(a, b):\n    pass\n", 1),
        ("problem/problem.json", "check", "{\"hidden\": true}", 0),
        (
            "reference/solution.py",
            "reference",
            "def add(a, b):\n    return a + b\n",
            0,
        ),
    ];
    for (ordinal, (path, role, content, editable)) in files.into_iter().enumerate() {
        sqlx::query("INSERT INTO learning_practical_files(activity_id,ordinal,path,role,content,content_sha256,editable) VALUES(?,?,?,?,?,?,?)")
            .bind(&activity_id)
            .bind(ordinal as i64)
            .bind(path)
            .bind(role)
            .bind(content)
            .bind(sha256_hex(content))
            .bind(editable)
            .execute(pool)
            .await
            .map_err(db)?;
    }
    sqlx::query("INSERT INTO learning_simulation_sessions(id,program_id,activity_id,activity_revision,activity_snapshot_json,practice_session_id,operation_id,payload_hash,learner_role,counterpart_role,status,revision,created_at,updated_at) VALUES(?,?,?,0,'{}',NULL,?,'hash','Candidate','Coach','active',0,?,?)")
        .bind(&session_id)
        .bind(&program_id)
        .bind(&activity_id)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(timestamp)
        .bind(timestamp)
        .execute(pool)
        .await
        .map_err(db)?;
    sqlx::query("INSERT INTO learning_exercise_sessions(id,program_id,activity_id,kind,form,setting,stage,draft_json,created_at) VALUES(?,?,?,'program','build','coached','understand','[]',?)")
        .bind(&session_id)
        .bind(&program_id)
        .bind(&activity_id)
        .bind(timestamp)
        .execute(pool)
        .await
        .map_err(db)?;
    Ok(Setup {
        repo,
        program_id,
        activity_id,
        session_id,
    })
}

fn run_request(setup: &Setup, files: Vec<(&str, &str)>) -> StartLearningPracticalRunRequestDto {
    StartLearningPracticalRunRequestDto {
        operation_id: uuid::Uuid::new_v4().to_string(),
        run_id: uuid::Uuid::new_v4().to_string(),
        program_id: setup.program_id.clone(),
        activity_id: setup.activity_id.clone(),
        expected_activity_revision: 0,
        practice_session_id: None,
        learner_files: files
            .into_iter()
            .map(|(path, content)| crate::lab_runtime::LearningLabFile {
                path: path.into(),
                content: content.into(),
            })
            .collect(),
    }
}

fn coding_start(
    setup: &Setup,
    request: StartLearningPracticalRunRequestDto,
    scope: CodingRunScope,
) -> CodingRunStart {
    CodingRunStart {
        base: request,
        exercise_session_id: setup.session_id.clone(),
        scope,
        cases: three_cases(),
        mine_ids: vec!["exact".into()],
        spec: sample_spec(),
    }
}

/// A run row that is still running, as `prepare_run` leaves it, with the harness a coding run holds.
async fn running(
    pool: &SqlitePool,
    setup: &Setup,
    coding: bool,
    outcome_ids: Vec<String>,
) -> Result<PreparedRun> {
    let run_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO learning_practical_runs(id, program_id, activity_id, activity_revision, activity_snapshot_json, practice_session_id, operation_id, payload_hash, engine, image_id, status, learner_files_json, created_at, builtin_runtime, builtin_runtime_version) \
         VALUES(?, ?, ?, 0, '{}', NULL, ?, 'hash', NULL, NULL, 'running', '[]', ?, 'python', ?)",
    )
    .bind(&run_id)
    .bind(&setup.program_id)
    .bind(&setup.activity_id)
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(now())
    .bind(LearningBuiltinRuntime::Python.version())
    .execute(pool)
    .await
    .map_err(db)?;
    Ok(PreparedRun {
        dto: setup.repo.run(&run_id).await?,
        execution: PreparedExecution::Builtin {
            spec: crate::embedded_runtime::BuiltinLabExecutionSpec {
                run_id,
                runtime: LearningBuiltinRuntime::Python,
                files: vec![],
                read_only_paths: vec![],
                limits: crate::lab_runtime::LearningLabLimits::default(),
            },
            version: LearningBuiltinRuntime::Python.version().into(),
        },
        outcome_ids,
        practice_mode: crate::dto::LearningPracticeMode::Practice,
        allowed_aids: vec![],
        workspace_owner_token: None,
        harness: coding.then(|| PreparedHarness {
            harness: Box::new(PythonHarness::new()),
            nonce: NONCE.into(),
            cases: three_cases(),
            mine_ids: vec!["exact".into()],
        }),
    })
}

fn execution(status: LearningLabRunStatus, stdout: String) -> LearningLabExecutionResult {
    LearningLabExecutionResult {
        status,
        exit_code: Some(0),
        stdout,
        stderr: String::new(),
        output_truncated: false,
        duration_ms: 120,
    }
}

#[tokio::test]
async fn a_coding_run_stores_a_row_per_case_with_the_definition_beside_the_result() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let setup = setup(&pool).await?;
    let prepared = running(&pool, &setup, true, vec![]).await?;
    let claiming = format!(
        "{NONCE}{}\n",
        json!({
            "caseId": "exact",
            "status": "passed",
            "actual": "[0, 1]",
            "printed": "debug print",
            "error": null,
            "elapsedMs": 3,
            "category": "claimed by the line",
            "visibility": "hidden",
            "expected": "[9]",
            "args": ["claimed"],
        })
    );
    let stdout = format!(
        "learner noise\n{claiming}{}{}",
        line("unordered", "passed", Some("[2, 1]"), "", None),
        line("floats", "passed", Some("[0.5, 0.5]"), "", None),
    );
    setup
        .repo
        .finish_run(&prepared, execution(LearningLabRunStatus::Passed, stdout))
        .await?;
    let run = setup.repo.run(&prepared.dto.id).await?;
    let names: Vec<&str> = run.checks.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["exact", "unordered", "floats"]);
    assert!(run
        .checks
        .iter()
        .all(|c| c.status == LearningPracticalCheckStatus::Passed));
    assert!(run.stdout.contains("learner noise"));
    assert!(!run.stdout.contains(NONCE));
    let exact = run.checks[0].detail.as_ref().unwrap();
    assert_eq!(exact.case_id, "exact");
    assert_eq!(exact.category, "category of exact");
    assert_eq!(exact.visibility, CaseVisibility::Example);
    assert_eq!(exact.expected.as_deref(), Some("[0, 1]"));
    assert_eq!(
        exact.args.as_deref(),
        Some(&["[2, 7]".to_owned(), "9".to_owned()][..])
    );
    assert_eq!(exact.printed.as_deref(), Some("debug print"));
    assert_eq!(exact.status, CaseStatus::Passed);
    assert!(exact.mine);
    assert!(!run.checks[1].detail.as_ref().unwrap().mine);
    Ok(())
}

#[tokio::test]
async fn a_timed_out_coding_run_keeps_finished_cases_and_marks_the_rest() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let setup = setup(&pool).await?;
    let prepared = running(&pool, &setup, true, vec![]).await?;
    let stdout = line("exact", "passed", Some("[0, 1]"), "", None);
    setup
        .repo
        .finish_run(&prepared, execution(LearningLabRunStatus::TimedOut, stdout))
        .await?;
    let run = setup.repo.run(&prepared.dto.id).await?;
    let statuses: Vec<CaseStatus> = run
        .checks
        .iter()
        .map(|c| c.detail.as_ref().unwrap().status)
        .collect();
    assert_eq!(
        statuses,
        [CaseStatus::Passed, CaseStatus::TimedOut, CaseStatus::NotRun]
    );
    Ok(())
}

#[tokio::test]
async fn a_hidden_cases_content_never_reaches_a_generic_run_read() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let setup = setup(&pool).await?;
    let prepared = running(&pool, &setup, true, vec![]).await?;
    let stdout = format!(
        "{}{}{}",
        line("exact", "passed", Some("[0, 1]"), "", None),
        line("unordered", "passed", Some("[2, 1]"), "", None),
        line(
            "floats",
            "error",
            None,
            "secret print",
            Some("ValueError: secret")
        ),
    );
    setup
        .repo
        .finish_run(&prepared, execution(LearningLabRunStatus::Failed, stdout))
        .await?;
    let run = setup.repo.run(&prepared.dto.id).await?;
    let hidden = run.checks[2].detail.as_ref().unwrap();
    assert_eq!(hidden.visibility, CaseVisibility::Hidden);
    assert_eq!(hidden.category, "category of floats");
    assert_eq!(hidden.status, CaseStatus::Error);
    assert!(hidden.args.is_none());
    assert!(hidden.expected.is_none());
    assert!(hidden.actual.is_none());
    assert!(hidden.printed.is_none());
    assert!(hidden.error.is_none());
    assert!(!run.checks[2].message.contains("secret"));
    assert!(!serde_json::to_string(&run).unwrap().contains("secret"));
    assert!(run.checks[0].detail.as_ref().unwrap().args.is_some());
    Ok(())
}

#[tokio::test]
async fn a_coding_run_writes_no_evidence_events() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let setup = setup(&pool).await?;
    let outcome_id = uuid::Uuid::new_v4().to_string();
    let prepared = running(&pool, &setup, true, vec![outcome_id]).await?;
    let stdout = line("exact", "passed", Some("[0, 1]"), "", None);
    setup
        .repo
        .finish_run(&prepared, execution(LearningLabRunStatus::Passed, stdout))
        .await?;
    let events: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM learning_evidence_events WHERE source_id=?")
            .bind(&prepared.dto.id)
            .fetch_one(&pool)
            .await
            .map_err(db)?;
    assert_eq!(events, 0);
    Ok(())
}

#[tokio::test]
async fn a_run_without_a_harness_keeps_the_single_contained_evaluator_row() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let setup = setup(&pool).await?;
    let prepared = running(&pool, &setup, false, vec![]).await?;
    let stdout = format!(
        "Ran 5 tests in 0.02s\nOK\n{}",
        line("exact", "passed", None, "", None)
    );
    setup
        .repo
        .finish_run(
            &prepared,
            execution(LearningLabRunStatus::Passed, stdout.clone()),
        )
        .await?;
    let run = setup.repo.run(&prepared.dto.id).await?;
    assert_eq!(run.checks.len(), 1);
    assert_eq!(run.checks[0].name, "Contained evaluator");
    assert_eq!(run.checks[0].status, LearningPracticalCheckStatus::Passed);
    assert!(run.checks[0].detail.is_none());
    assert_eq!(run.stdout, stdout);
    Ok(())
}

#[tokio::test]
async fn a_coding_runs_files_are_the_learners_and_the_harness_evaluator_only() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let setup = setup(&pool).await?;
    let request = run_request(
        &setup,
        vec![("solution.py", "def add(a, b):\n    return a + b\n")],
    );
    let start = coding_start(&setup, request.clone(), CodingRunScope::All);
    let prepared = setup
        .repo
        .prepare_run(&request, Some(&start), "hash")
        .await?
        .expect("a new run");
    let PreparedExecution::Builtin { spec, .. } = &prepared.execution else {
        panic!("a coding run uses the built-in runtime");
    };
    let mut paths: Vec<&str> = spec.files.iter().map(|f| f.path.as_str()).collect();
    paths.sort_unstable();
    assert_eq!(paths, ["cases.json", "checks.py", "solution.py"]);
    let mut read_only: Vec<&str> = spec.read_only_paths.iter().map(String::as_str).collect();
    read_only.sort_unstable();
    assert_eq!(read_only, ["cases.json", "checks.py"]);
    assert!(prepared.harness.is_some());

    let forged = run_request(
        &setup,
        vec![("solution.py", "x = 1\n"), ("cases.json", "{}")],
    );
    let start = coding_start(&setup, forged.clone(), CodingRunScope::All);
    assert!(setup
        .repo
        .prepare_run(&forged, Some(&start), "hash")
        .await
        .is_err());
    Ok(())
}

#[tokio::test]
async fn each_coding_run_gets_its_own_nonce_and_records_its_session_and_scope() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let setup = setup(&pool).await?;
    let mut nonces = Vec::new();
    for scope in [CodingRunScope::Shown, CodingRunScope::Input] {
        let request = run_request(&setup, vec![("solution.py", "x = 1\n")]);
        let start = coding_start(&setup, request.clone(), scope);
        let prepared = setup
            .repo
            .prepare_run(&request, Some(&start), "hash")
            .await?
            .expect("a new run");
        let harness = prepared.harness.as_ref().expect("a harness");
        assert_eq!(harness.nonce.len(), 32);
        nonces.push(harness.nonce.clone());
        let (session, stored_scope): (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT exercise_session_id, scope FROM learning_practical_runs WHERE id=?",
        )
        .bind(&request.run_id)
        .fetch_one(&pool)
        .await
        .map_err(db)?;
        assert_eq!(session.as_deref(), Some(setup.session_id.as_str()));
        let expected = match start.scope {
            CodingRunScope::Shown => "shown",
            CodingRunScope::All => "all",
            CodingRunScope::Input => "input",
        };
        assert_eq!(stored_scope.as_deref(), Some(expected));
    }
    assert_ne!(nonces[0], nonces[1]);
    Ok(())
}

#[cfg(feature = "learning-labs")]
mod real_python {
    use super::*;
    use crate::drills::runner::{runner_for, CodeRunner};
    use crate::embedded_runtime::BuiltinRuntimes;
    use crate::lab_runtime::LearningLabFile;
    use lattice_core::shared::error::AppError;
    use tokio_util::sync::CancellationToken;

    fn runner() -> Result<Box<dyn CodeRunner>> {
        runner_for("python", &BuiltinRuntimes::for_tests())
            .ok_or_else(|| AppError::InternalError("Python has no runner here".into()))
    }

    async fn run(code: &str, cases: &[ProblemCase]) -> Result<(RunOutcome, Vec<CaseResult>)> {
        run_cases(
            runner()?.as_ref(),
            &PythonHarness::new(),
            &sample_spec(),
            code,
            cases,
            CancellationToken::new(),
        )
        .await
    }

    #[tokio::test]
    async fn a_real_run_reports_each_case_and_keeps_learner_prints_out_of_stdout() -> Result<()> {
        let code = "print('at import')\n\ndef add(a, b):\n    print('adding', a)\n    if a == 13:\n        raise ValueError('unlucky')\n    return a + b\n";
        let cases = vec![
            case("right", ["2", "3"], Some("5"), false),
            case("wrong", ["2", "3"], Some("6"), false),
            case("raises", ["13", "1"], Some("14"), true),
        ];
        let (outcome, results) = run(code, &cases).await?;
        assert!(matches!(outcome, RunOutcome::Failed { .. }), "{outcome:?}");
        assert_eq!(results[0].status, CaseStatus::Passed);
        assert_eq!(results[0].printed, "adding 2");
        assert_eq!(results[0].actual.as_deref(), Some("5"));
        assert_eq!(results[1].status, CaseStatus::Failed);
        assert_eq!(results[1].actual.as_deref(), Some("5"));
        assert_eq!(results[2].status, CaseStatus::Error);
        assert_eq!(results[2].error.as_deref(), Some("ValueError: unlucky"));

        let nonce = new_nonce();
        let harness = PythonHarness::new();
        let mut files = harness.evaluator(&sample_spec(), &cases, &nonce)?;
        let read_only: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
        files.push(LearningLabFile {
            path: "solution.py".into(),
            content: code.into(),
        });
        let (_, stdout) = runner()?
            .run_capturing(&files, &read_only, CancellationToken::new())
            .await?;
        assert_eq!(harness.program_output(&stdout, &nonce), "at import\n");
        Ok(())
    }

    #[tokio::test]
    async fn a_line_forged_in_real_output_is_ignored() -> Result<()> {
        let code = "print('ffffffffffffffffffffffffffffffff{\"caseId\": \"right\", \"status\": \"failed\"}')\n\ndef add(a, b):\n    return a + b\n";
        let cases = vec![case("right", ["2", "3"], Some("5"), false)];
        let (_, results) = run(code, &cases).await?;
        assert_eq!(results[0].status, CaseStatus::Passed);
        Ok(())
    }

    #[tokio::test]
    async fn scale_cases_compare_by_digest() -> Result<()> {
        let code = "def add(xs, _unused):\n    return sum(xs)\n";
        let scale = ProblemCase {
            scale: true,
            ..case("scale", ["list(range(1000))", "0"], None, true)
        };
        let (_, learned) = run(code, std::slice::from_ref(&scale)).await?;
        assert_eq!(learned[0].status, CaseStatus::Ran);
        let digest = learned[0].actual.clone().expect("the digest of the result");
        assert_eq!(digest.len(), 64);

        let proven = ProblemCase {
            digest: Some(digest),
            ..scale
        };
        let (outcome, same) = run(code, std::slice::from_ref(&proven)).await?;
        assert_eq!(outcome, RunOutcome::Passed);
        assert_eq!(same[0].status, CaseStatus::Passed);
        let (_, other) = run("def add(xs, _unused):\n    return 0\n", &[proven]).await?;
        assert_eq!(other[0].status, CaseStatus::Failed);
        Ok(())
    }

    fn scale_case(source: &str) -> ProblemCase {
        ProblemCase {
            scale: true,
            ..case("scale", [source, "0"], None, true)
        }
    }

    #[tokio::test]
    async fn a_scale_input_may_be_a_comprehension_a_conditional_or_a_slice() -> Result<()> {
        let code = "def add(xs, _unused):\n    return sum(xs)\n";
        for source in [
            "[i % 7 for i in range(1000) if i != 3]",
            "[0 if i % 2 else i for i in range(100)][::-1]",
        ] {
            let (_, results) = run(code, &[scale_case(source)]).await?;
            assert_eq!(
                results[0].status,
                CaseStatus::Ran,
                "{source}: {:?}",
                results[0].error
            );
            assert_eq!(
                results[0].actual.as_ref().map(String::len),
                Some(64),
                "{source}"
            );
        }
        Ok(())
    }

    #[tokio::test]
    async fn a_scale_input_with_an_attribute_a_lambda_or_a_called_loop_variable_is_refused(
    ) -> Result<()> {
        let code = "def add(xs, _unused):\n    return 0\n";
        for source in [
            "[i for i in range(3)].__class__",
            "(lambda: 1)()",
            "[f(1) for f in [len]]",
        ] {
            let (_, results) = run(code, &[scale_case(source)]).await?;
            assert_eq!(results[0].status, CaseStatus::Error, "{source}");
            let error = results[0].error.clone().unwrap_or_default();
            assert!(
                error.contains("not allowed in a scale input"),
                "{source}: {error}"
            );
        }
        let (_, results) = run(code, &[scale_case("[i for i in range(3)].__class__")]).await?;
        assert!(results[0]
            .error
            .clone()
            .unwrap_or_default()
            .contains("Attribute"));
        Ok(())
    }

    #[tokio::test]
    async fn unordered_and_float_cases_compare_by_their_rule() -> Result<()> {
        let code = "def add(a, b):\n    return a\n";
        let cases = vec![
            ProblemCase {
                compare: CaseCompare::Unordered,
                ..case("unordered", ["[2, 1]", "0"], Some("[1, 2]"), false)
            },
            ProblemCase {
                compare: CaseCompare::Exact,
                ..case("exact", ["[2, 1]", "0"], Some("[1, 2]"), false)
            },
            ProblemCase {
                compare: CaseCompare::FloatWithin { tolerance: 0.01 },
                ..case("close", ["[0.501, 1]", "0"], Some("[0.5, 1]"), false)
            },
            ProblemCase {
                compare: CaseCompare::FloatWithin { tolerance: 0.01 },
                ..case("far", ["[0.6, 1]", "0"], Some("[0.5, 1]"), false)
            },
        ];
        let (_, results) = run(code, &cases).await?;
        let statuses: Vec<CaseStatus> = results.iter().map(|r| r.status).collect();
        assert_eq!(
            statuses,
            [
                CaseStatus::Passed,
                CaseStatus::Failed,
                CaseStatus::Passed,
                CaseStatus::Failed
            ]
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_solution_that_will_not_import_fails_every_case() -> Result<()> {
        let cases = vec![
            case("a", ["1", "1"], Some("2"), false),
            case("b", ["2", "2"], Some("4"), false),
        ];
        let (_, results) = run("def add(a, b:\n", &cases).await?;
        assert!(results.iter().all(|r| r.status == CaseStatus::Error));
        assert!(results[0]
            .error
            .as_deref()
            .is_some_and(|e| e.starts_with("SyntaxError")));
        Ok(())
    }

    #[tokio::test]
    async fn a_project_checks_file_reports_its_cases_through_the_helper() -> Result<()> {
        let nonce = new_nonce();
        let checks = "import lattice_check\n\nwith lattice_check.case('passes'):\n    print('hello')\n    assert 1 + 1 == 2\nwith lattice_check.case('fails'):\n    assert 1 + 1 == 3, 'math'\nwith lattice_check.case('raises'):\n    raise KeyError('k')\n";
        let files = vec![
            LearningLabFile {
                path: "checks.py".into(),
                content: checks.into(),
            },
            lattice_check_file(&nonce),
        ];
        let (outcome, stdout) = runner()?
            .run_capturing(&files, &["checks.py".to_owned()], CancellationToken::new())
            .await?;
        assert!(matches!(outcome, RunOutcome::Failed { .. }), "{outcome:?}");
        let cases = vec![
            case("passes", ["0", "0"], None, false),
            case("fails", ["0", "0"], None, false),
            case("raises", ["0", "0"], None, false),
        ];
        let results = PythonHarness::new().results(&cases, &stdout, &nonce, RunEnd::Finished);
        let statuses: Vec<CaseStatus> = results.iter().map(|r| r.status).collect();
        assert_eq!(
            statuses,
            [CaseStatus::Passed, CaseStatus::Failed, CaseStatus::Error]
        );
        assert_eq!(results[0].printed, "hello");
        assert_eq!(results[1].error.as_deref(), Some("math"));
        Ok(())
    }
}
