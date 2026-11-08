use super::*;
use crate::drills::runner::{CodeRunner, RunOutcome};
use crate::lab_runtime::{
    LearningLabExecutionResult, LearningLabFile, LearningLabLimits, LearningLabRunStatus,
};
use crate::practice::coding::problem::{
    Case, CaseResult, CaseStatus, Compare, Difficulty, Param, ProblemSpec, Signature, Visibility,
};
use crate::practice::practical_dto::{CaseCheckDetailDto, LearningPracticalCheckStatus};
use crate::practice::practical_repository::{
    LearningPracticalRepository, PreparedExecution, PreparedRun,
};
use async_trait::async_trait;
use lattice_core::domain::learning::drills::DrillFile;
use lattice_core::shared::error::Result;
use tokio_util::sync::CancellationToken;

struct MockRunner {
    outcome: RunOutcome,
    output: String,
}

#[async_trait]
impl CodeRunner for MockRunner {
    fn name(&self) -> &str {
        "mock-python"
    }

    fn entrypoint(&self) -> &str {
        "checks.py"
    }

    fn contract(&self) -> &str {
        "mock contract"
    }

    fn running(&self, _path: &str) -> String {
        String::new()
    }

    async fn run(
        &self,
        _files: &[DrillFile],
        _test: &str,
        _cancel: CancellationToken,
    ) -> Result<RunOutcome> {
        Ok(self.outcome.clone())
    }

    async fn run_capturing(
        &self,
        _files: &[LearningLabFile],
        _read_only_paths: &[String],
        _cancel: CancellationToken,
    ) -> Result<(RunOutcome, String)> {
        Ok((self.outcome.clone(), self.output.clone()))
    }
}

fn sample_spec() -> ProblemSpec {
    ProblemSpec {
        title: "Two Sum".into(),
        statement: "Given numbers and target, return indices.".into(),
        signature: Signature {
            function: "two_sum".into(),
            params: vec![
                Param {
                    name: "numbers".into(),
                    annotation: "list[int]".into(),
                },
                Param {
                    name: "target".into(),
                    annotation: "int".into(),
                },
            ],
            returns: "list[int]".into(),
        },
        constraints: vec!["2 <= len(numbers) <= 10^4".into()],
        clarifications: vec![],
        language: "python".into(),
        difficulty: Difficulty::Easy,
        target_complexity: "O(n)".into(),
        patterns: vec!["lookup".into()],
        starter: "def two_sum(numbers: list[int], target: int) -> list[int]:\n    pass\n".into(),
    }
}

async fn setup_test_run(
    pool: &sqlx::SqlitePool,
) -> Result<(LearningPracticalRepository, PreparedRun)> {
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
    let run_id = uuid::Uuid::new_v4().to_string();
    let now = crate::persistence::now();

    sqlx::query(
        "INSERT INTO learning_practical_activities(id, program_id, lesson_id, predecessor_id, kind, title, brief, status, practice_mode, allowed_aids_json, outcome_ids_json, source_version_ids_json, rubric_json, runtime_kind, runtime_profile_id, runtime_engine, runtime_image_id, runtime_command_json, runtime_limits_json, generator_model, revision, created_at, updated_at, builtin_runtime, builtin_runtime_version) \
         VALUES(?, ?, ?, NULL, 'code_lab', 'Test Activity', 'Brief', 'ready', 'practice', '[]', '[]', '[]', '[]', 'none', NULL, NULL, NULL, NULL, NULL, 'fixture-model', 0, ?, ?, 'python', '3.14')",
    )
    .bind(&activity_id)
    .bind(&program_id)
    .bind(&lesson_id)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| lattice_core::shared::error::AppError::Database(e.to_string()))?;

    sqlx::query(
        "INSERT INTO learning_practical_runs(id, program_id, activity_id, activity_revision, activity_snapshot_json, \
         practice_session_id, operation_id, payload_hash, engine, image_id, status, learner_files_json, created_at, \
         builtin_runtime, builtin_runtime_version) \
         VALUES(?, ?, ?, 0, '{}', NULL, ?, 'hash', NULL, NULL, 'running', '[]', ?, 'python', '3.14')",
    )
    .bind(&run_id)
    .bind(&program_id)
    .bind(&activity_id)
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| lattice_core::shared::error::AppError::Database(e.to_string()))?;

    let run_dto = repo.run(&run_id).await?;
    let prepared = PreparedRun {
        dto: run_dto,
        execution: PreparedExecution::Builtin {
            spec: crate::embedded_runtime::BuiltinLabExecutionSpec {
                run_id: run_id.clone(),
                runtime: crate::embedded_runtime::LearningBuiltinRuntime::Python,
                files: vec![],
                read_only_paths: vec![],
                limits: LearningLabLimits::default(),
            },
            version: "3.14".into(),
        },
        outcome_ids: vec![],
        practice_mode: crate::dto::LearningPracticeMode::Practice,
        allowed_aids: vec![],
        workspace_owner_token: None,
    };

    Ok((repo, prepared))
}

#[tokio::test]
async fn test_run_cases_three_cases_reports_three_results() -> Result<()> {
    let case1 = Case {
        id: "case_exact".into(),
        category: "exact match".into(),
        args: vec!["[2, 7, 11, 15]".into(), "9".into()],
        expected: Some("[0, 1]".into()),
        compare: Compare::Exact,
        visibility: Visibility::Example,
        scale: false,
    };
    let case2 = Case {
        id: "case_unordered".into(),
        category: "unordered elements".into(),
        args: vec!["[3, 2, 4]".into(), "6".into()],
        expected: Some("[1, 2]".into()),
        compare: Compare::Unordered,
        visibility: Visibility::Example,
        scale: false,
    };
    let case3 = Case {
        id: "case_float".into(),
        category: "float tolerance".into(),
        args: vec!["[1.0, 2.0]".into(), "3".into()],
        expected: Some("[0.5, 0.5]".into()),
        compare: Compare::FloatWithin { tolerance: 0.01 },
        visibility: Visibility::Hidden,
        scale: false,
    };

    let runner_stdout = format!(
        "{nonce}{{\"case_id\":\"case_exact\",\"status\":\"passed\",\"actual\":\"[0, 1]\",\"printed\":\"computing case 1\",\"error\":null,\"elapsed_ms\":5}}\n\
         {nonce}{{\"case_id\":\"case_unordered\",\"status\":\"passed\",\"actual\":\"[2, 1]\",\"printed\":\"unordered test\",\"error\":null,\"elapsed_ms\":3}}\n\
         {nonce}{{\"case_id\":\"case_float\",\"status\":\"passed\",\"actual\":\"[0.5001, 0.4999]\",\"printed\":\"float test\",\"error\":null,\"elapsed_ms\":4}}\n",
        nonce = DEFAULT_NONCE
    );
    let runner = MockRunner {
        outcome: RunOutcome::Passed,
        output: runner_stdout,
    };
    let harness = PythonHarness::new();
    let spec = sample_spec();
    let cases = vec![case1, case2, case3];
    let (outcome, results) = run_cases(
        &runner,
        &harness,
        &spec,
        "def two_sum(n, t): return [0, 1]",
        &cases,
        CancellationToken::new(),
    )
    .await?;

    assert_eq!(outcome, RunOutcome::Passed);
    assert_eq!(results.len(), 3);
    assert_eq!(results.get(0).map(|r| r.case_id.as_str()), Some("case_exact"));
    assert_eq!(results.get(0).map(|r| r.status), Some(CaseStatus::Passed));
    assert_eq!(
        results.get(0).map(|r| r.printed.as_str()),
        Some("computing case 1")
    );
    assert_eq!(
        results.get(0).and_then(|r| r.actual.as_deref()),
        Some("[0, 1]")
    );

    assert_eq!(
        results.get(1).map(|r| r.case_id.as_str()),
        Some("case_unordered")
    );
    assert_eq!(results.get(1).map(|r| r.status), Some(CaseStatus::Passed));

    assert_eq!(results.get(2).map(|r| r.case_id.as_str()), Some("case_float"));
    assert_eq!(results.get(2).map(|r| r.status), Some(CaseStatus::Passed));
    Ok(())
}

#[test]
fn test_python_harness_results_comparisons_stdout_and_exceptions() {
    let stdout = format!(
        "random setup line before cases\n\
         {nonce}{{\"case_id\":\"c1\",\"status\":\"passed\",\"actual\":\"[0, 1]\",\"printed\":\"print from c1\",\"error\":null,\"elapsed_ms\":5}}\n\
         intermediate non-nonce log message\n\
         {nonce}{{\"case_id\":\"c2\",\"status\":\"passed\",\"actual\":\"[2, 1]\",\"printed\":\"\",\"error\":null,\"elapsed_ms\":7}}\n\
         {nonce}{{\"case_id\":\"c3\",\"status\":\"error\",\"actual\":null,\"printed\":\"crashing now\",\"error\":\"ZeroDivisionError: division by zero\",\"elapsed_ms\":2}}\n\
         trailing summary line\n",
        nonce = DEFAULT_NONCE
    );
    let harness = PythonHarness::new();
    let results = harness.results(&stdout, DEFAULT_NONCE);

    assert_eq!(results.len(), 3);
    assert_eq!(results.get(0).map(|r| r.case_id.as_str()), Some("c1"));
    assert_eq!(results.get(0).map(|r| r.status), Some(CaseStatus::Passed));
    assert_eq!(results.get(0).map(|r| r.printed.as_str()), Some("print from c1"));
    assert_eq!(results.get(0).and_then(|r| r.actual.as_deref()), Some("[0, 1]"));

    assert_eq!(results.get(1).map(|r| r.case_id.as_str()), Some("c2"));
    assert_eq!(results.get(1).map(|r| r.status), Some(CaseStatus::Passed));

    assert_eq!(results.get(2).map(|r| r.case_id.as_str()), Some("c3"));
    assert_eq!(results.get(2).map(|r| r.status), Some(CaseStatus::Error));
    assert_eq!(
        results.get(2).and_then(|r| r.error.as_deref()),
        Some("ZeroDivisionError: division by zero")
    );
    assert_eq!(
        results.get(2).map(|r| r.printed.as_str()),
        Some("crashing now")
    );
}

#[test]
fn test_python_harness_evaluator_generates_checks_and_cases() {
    let harness = PythonHarness::new();
    let spec = sample_spec();
    let cases = vec![Case {
        id: "c1".into(),
        category: "basic case".into(),
        args: vec!["[1, 2]".into(), "3".into()],
        expected: Some("[0, 1]".into()),
        compare: Compare::Exact,
        visibility: Visibility::Example,
        scale: false,
    }];
    let files = harness.evaluator(&spec, &cases, DEFAULT_NONCE);
    assert!(!files.is_empty(), "evaluator must produce lab files");
    assert!(
        files.iter().any(|f| f.path == "checks.py"),
        "checks.py entrypoint must be generated"
    );
    assert!(
        files.iter().any(|f| f.path == "cases.json"),
        "cases.json must be generated"
    );
}

#[tokio::test]
async fn test_run_cases_timeout_marks_inflight_and_remaining_not_run_with_scale_last() -> Result<()> {
    let case_scale = Case {
        id: "case_scale".into(),
        category: "scale test".into(),
        args: vec!["list(range(10000))".into(), "19997".into()],
        expected: Some("[9998, 9999]".into()),
        compare: Compare::Exact,
        visibility: Visibility::Hidden,
        scale: true,
    };
    let case_1 = Case {
        id: "case_1".into(),
        category: "case 1".into(),
        args: vec!["[1, 2]".into(), "3".into()],
        expected: Some("[0, 1]".into()),
        compare: Compare::Exact,
        visibility: Visibility::Example,
        scale: false,
    };
    let case_2 = Case {
        id: "case_2".into(),
        category: "case 2".into(),
        args: vec!["[2, 3]".into(), "5".into()],
        expected: Some("[0, 1]".into()),
        compare: Compare::Exact,
        visibility: Visibility::Example,
        scale: false,
    };
    let case_3 = Case {
        id: "case_3".into(),
        category: "case 3".into(),
        args: vec!["[3, 4]".into(), "7".into()],
        expected: Some("[0, 1]".into()),
        compare: Compare::Exact,
        visibility: Visibility::Hidden,
        scale: false,
    };

    // Runner output only has case_1 before timeout occurs while case_2 was running
    let runner_stdout = format!(
        "{nonce}{{\"case_id\":\"case_1\",\"status\":\"passed\",\"actual\":\"[0, 1]\",\"printed\":\"\",\"error\":null,\"elapsed_ms\":5}}\n",
        nonce = DEFAULT_NONCE
    );
    let runner = MockRunner {
        outcome: RunOutcome::TimedOut {
            output: runner_stdout.clone(),
        },
        output: runner_stdout,
    };
    let harness = PythonHarness::new();
    let spec = sample_spec();
    // Pass cases with scale case first to verify scale case is ordered last in execution
    let cases = vec![case_scale, case_1, case_2, case_3];
    let (outcome, results) = run_cases(
        &runner,
        &harness,
        &spec,
        "def two_sum(n, t): return [0, 1]",
        &cases,
        CancellationToken::new(),
    )
    .await?;

    assert!(matches!(outcome, RunOutcome::TimedOut { .. }));
    assert_eq!(results.len(), 4);

    // Finished case
    assert_eq!(results.get(0).map(|r| r.case_id.as_str()), Some("case_1"));
    assert_eq!(results.get(0).map(|r| r.status), Some(CaseStatus::Passed));

    // In-flight case at timeout
    assert_eq!(results.get(1).map(|r| r.case_id.as_str()), Some("case_2"));
    assert_eq!(results.get(1).map(|r| r.status), Some(CaseStatus::TimedOut));

    // Remaining non-scale case
    assert_eq!(results.get(2).map(|r| r.case_id.as_str()), Some("case_3"));
    assert_eq!(results.get(2).map(|r| r.status), Some(CaseStatus::NotRun));

    // Scale case scheduled last
    assert_eq!(results.get(3).map(|r| r.case_id.as_str()), Some("case_scale"));
    assert_eq!(results.get(3).map(|r| r.status), Some(CaseStatus::NotRun));

    Ok(())
}

#[tokio::test]
async fn test_finish_run_project_without_case_helper_reports_one_contained_evaluator_row() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let (repo, prepared) = setup_test_run(&pool).await?;

    let execution = LearningLabExecutionResult {
        status: LearningLabRunStatus::Passed,
        exit_code: Some(0),
        stdout: "Ran 5 tests in 0.02s\nOK\n".into(),
        stderr: String::new(),
        output_truncated: false,
        duration_ms: 120,
    };

    repo.finish_run(&prepared, execution).await?;

    let run = repo.run(&prepared.dto.id).await?;
    assert_eq!(run.checks.len(), 1);
    assert_eq!(run.checks.get(0).map(|c| c.name.as_str()), Some("Contained evaluator"));
    assert_eq!(
        run.checks.get(0).map(|c| c.status),
        Some(LearningPracticalCheckStatus::Passed)
    );
    assert!(run.checks.get(0).and_then(|c| c.detail.as_ref()).is_none());

    Ok(())
}

#[tokio::test]
async fn test_finish_run_with_case_lines_writes_one_row_per_case_with_details() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let (repo, prepared) = setup_test_run(&pool).await?;

    let stdout = format!(
        "{nonce}{{\"case_id\":\"case_exact\",\"status\":\"passed\",\"actual\":\"[0, 1]\",\"printed\":\"debug print\",\"error\":null,\"elapsed_ms\":5}}\n\
         {nonce}{{\"case_id\":\"case_unordered\",\"status\":\"passed\",\"actual\":\"[2, 1]\",\"printed\":\"\",\"error\":null,\"elapsed_ms\":8}}\n\
         {nonce}{{\"case_id\":\"case_float\",\"status\":\"passed\",\"actual\":\"[0.5, 0.5]\",\"printed\":\"\",\"error\":null,\"elapsed_ms\":10}}\n",
        nonce = DEFAULT_NONCE
    );

    let execution = LearningLabExecutionResult {
        status: LearningLabRunStatus::Passed,
        exit_code: Some(0),
        stdout,
        stderr: String::new(),
        output_truncated: false,
        duration_ms: 150,
    };

    repo.finish_run(&prepared, execution).await?;

    let run = repo.run(&prepared.dto.id).await?;
    assert_eq!(run.checks.len(), 3);

    assert_eq!(run.checks.get(0).map(|c| c.name.as_str()), Some("case_exact"));
    assert_eq!(
        run.checks.get(0).map(|c| c.status),
        Some(LearningPracticalCheckStatus::Passed)
    );
    let detail0 = run.checks.get(0).and_then(|c| c.detail.as_ref());
    assert!(detail0.is_some());
    assert_eq!(detail0.map(|d| d.case_id.as_str()), Some("case_exact"));
    assert_eq!(detail0.map(|d| d.printed.as_str()), Some("debug print"));

    assert_eq!(run.checks.get(1).map(|c| c.name.as_str()), Some("case_unordered"));
    assert_eq!(
        run.checks.get(1).map(|c| c.status),
        Some(LearningPracticalCheckStatus::Passed)
    );

    assert_eq!(run.checks.get(2).map(|c| c.name.as_str()), Some("case_float"));
    assert_eq!(
        run.checks.get(2).map(|c| c.status),
        Some(LearningPracticalCheckStatus::Passed)
    );

    Ok(())
}
