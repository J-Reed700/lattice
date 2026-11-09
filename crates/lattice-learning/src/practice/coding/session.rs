//! Coding session domain model, DTOs, and lifecycle management
//! (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md sections 4 and 7).

use serde::{Deserialize, Serialize};

use crate::drills::runner::CodeRunner;
use crate::lab_runtime::LearningLabFile;
use crate::practice::coding::debrief::CodingDebriefDto;
use crate::practice::coding::harness::CaseHarness;
use crate::practice::coding::hints::{CheckedCodeHint, CodeHintRung};
use crate::practice::coding::problem::{Case, CodingTask, Problem, Setting, Signature};
use crate::practice::practical_dto::{LearningPracticalFileDto, LearningPracticalRunDto, LearningSimulationTurnDto};
use lattice_core::application::ports::llm_port::LLMPort;
use lattice_core::shared::error::AppError;

/// Stages of a coding session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CodingStage {
    Understand,
    Plan,
    Code,
    Verify,
}

impl CodingStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Understand => "understand",
            Self::Plan => "plan",
            Self::Code => "code",
            Self::Verify => "verify",
        }
    }
}

/// The result of checking a learner-provided example against the reference solution and misreadings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ExampleVerdict {
    Right,
    Wrong,
    Invalid,
}

/// The scope of test cases executed in a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum RunScope {
    Examples,
    All,
    Input,
}

/// Subgoal label for instructional guidance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Subgoal {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub completed: bool,
}

/// A learner-entered example and its verification status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CodingExampleDto {
    pub ordinal: u32,
    pub args_json: String,
    pub expected_json: String,
    pub verdict: ExampleVerdict,
    /// Descriptions of misreadings this example catches.
    pub catches: Vec<String>,
}

/// Problem information visible to the learner, redacted according to the setting and session state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PublicProblemDto {
    pub title: String,
    pub statement: String,
    pub signature: Signature,
    pub examples: Vec<Case>,
    pub constraints: Vec<String>,
    pub subgoals: Vec<Subgoal>,
    pub files: Vec<LearningPracticalFileDto>,
    /// Present only when `discloses(setting, Hidden::WorkedSolution, submitted, finished)` is true.
    pub worked_solution: Option<String>,
    pub target_complexity: Option<String>,
    /// Total number of misreadings in the problem.
    pub total_misreadings: u32,
}

/// Elapsed active time recorded for a specific stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StageTimingDto {
    pub stage: CodingStage,
    pub elapsed_ms: u64,
}

/// Complete state of a coding session returned to the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CodingSessionDto {
    pub id: String,
    pub program_id: String,
    pub activity_id: String,
    pub task: CodingTask,
    pub setting: Setting,
    pub stage: CodingStage,
    pub budget_seconds: Option<u32>,
    pub active_ms: u64,
    pub opened_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub revision: i64,
    pub problem: PublicProblemDto,
    pub draft_files: Vec<LearningLabFile>,
    pub examples: Vec<CodingExampleDto>,
    pub hints: Vec<CheckedCodeHint>,
    pub available_next_rung: Option<CodeHintRung>,
    pub turns: Vec<LearningSimulationTurnDto>,
    pub runs: Vec<LearningPracticalRunDto>,
    pub stage_timings: Vec<StageTimingDto>,
    pub debrief: Option<CodingDebriefDto>,
}

/// Hidden items that may be redacted during a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Hidden {
    FailingCaseInput,
    UncaughtMisreading,
    WorkedSolution,
}

/// Whether the given setting discloses the hidden item at the current session stage.
/// - `Hidden::FailingCaseInput`: disclosed in Coached; hidden in Mock and Watched until finished.
/// - `Hidden::UncaughtMisreading`: disclosed in Coached after submit and in debrief; hidden during Mock and Watched.
/// - `Hidden::WorkedSolution`: disclosed in Coached after first submit (All run), or in any setting once finished.
pub fn discloses(_setting: Setting, _hidden: Hidden, _submitted: bool, _finished: bool) -> bool {
    false
}

/// Extension trait providing `discloses` method on `Setting`.
pub trait CodingSettingExt {
    fn discloses_item(self, hidden: Hidden, submitted: bool, finished: bool) -> bool;
}

impl CodingSettingExt for Setting {
    fn discloses_item(self, hidden: Hidden, submitted: bool, finished: bool) -> bool {
        discloses(self, hidden, submitted, finished)
    }
}

/// Evaluates a learner-provided example against the reference solution and misreadings.
pub async fn evaluate_example(
    _runner: &dyn CodeRunner,
    _harness: &dyn CaseHarness,
    _problem: &Problem,
    _args_json: &str,
    _expected_json: &str,
) -> Result<CodingExampleDto, AppError> {
    Err(AppError::InvalidInput("not implemented".into()))
}

/// Computes cumulative stage timings from recorded stage transition timestamps.
pub fn calculate_stage_timings(
    _stage_events: &[(CodingStage, i64)],
    _current_stage: CodingStage,
    _active_ms: u64,
) -> Vec<StageTimingDto> {
    vec![]
}

/// Updates active clock accounting upon window focus/blur or open/close.
pub fn compute_active_clock(
    _previous_active_ms: u64,
    _last_opened_at: Option<i64>,
    _now: i64,
    _open: bool,
) -> (u64, Option<i64>) {
    (0, None)
}

/// Starts a new coding session.
pub async fn start_coding_session(
    _pool: &sqlx::SqlitePool,
    _session_id: &str,
    _program_id: &str,
    _activity_id: &str,
    _setting: Setting,
    _budget_seconds: Option<u32>,
) -> Result<CodingSessionDto, AppError> {
    Err(AppError::InvalidInput("not implemented".into()))
}

/// Retrieves a coding session with contents redacted for the current setting and lifecycle.
pub async fn get_coding_session(
    _pool: &sqlx::SqlitePool,
    _session_id: &str,
) -> Result<CodingSessionDto, AppError> {
    Err(AppError::InvalidInput("not implemented".into()))
}

/// Moves a coding session to a new stage.
pub async fn move_coding_stage(
    _pool: &sqlx::SqlitePool,
    _session_id: &str,
    _stage: CodingStage,
    _expected_revision: i64,
) -> Result<CodingSessionDto, AppError> {
    Err(AppError::InvalidInput("not implemented".into()))
}

/// Checks a learner example against the problem's reference solution and misreadings.
pub async fn check_coding_example(
    _pool: &sqlx::SqlitePool,
    _runner: &dyn CodeRunner,
    _harness: &dyn CaseHarness,
    _session_id: &str,
    _args_json: &str,
    _expected_json: &str,
) -> Result<CodingExampleDto, AppError> {
    Err(AppError::InvalidInput("not implemented".into()))
}

/// Runs the learner's draft code under the requested scope.
pub async fn run_coding_session(
    _pool: &sqlx::SqlitePool,
    _runner: &dyn CodeRunner,
    _harness: &dyn CaseHarness,
    _session_id: &str,
    _scope: RunScope,
    _files: Vec<LearningLabFile>,
    _custom_input_json: Option<String>,
) -> Result<CodingSessionDto, AppError> {
    Err(AppError::InvalidInput("not implemented".into()))
}

/// Unlocks and returns the next hint rung for the learner.
pub async fn ask_coding_hint(
    _pool: &sqlx::SqlitePool,
    _llm: &dyn LLMPort,
    _runner: &dyn CodeRunner,
    _harness: &dyn CaseHarness,
    _session_id: &str,
    _expected_revision: i64,
) -> Result<CodingSessionDto, AppError> {
    Err(AppError::InvalidInput("not implemented".into()))
}

/// Records a learner turn and prompts the counterpart if the policy cues.
pub async fn send_coding_turn(
    _pool: &sqlx::SqlitePool,
    _llm: &dyn LLMPort,
    _runner: &dyn CodeRunner,
    _harness: &dyn CaseHarness,
    _session_id: &str,
    _content: &str,
    _spoken: bool,
    _expected_revision: i64,
) -> Result<CodingSessionDto, AppError> {
    Err(AppError::InvalidInput("not implemented".into()))
}

/// Records a draft checkpoint in Watched setting and cues the counterpart if appropriate.
pub async fn observe_coding_draft(
    _pool: &sqlx::SqlitePool,
    _llm: &dyn LLMPort,
    _runner: &dyn CodeRunner,
    _harness: &dyn CaseHarness,
    _session_id: &str,
    _files: Vec<LearningLabFile>,
) -> Result<CodingSessionDto, AppError> {
    Err(AppError::InvalidInput("not implemented".into()))
}

/// Updates the session's active clock tracking state.
pub async fn set_coding_clock(
    _pool: &sqlx::SqlitePool,
    _session_id: &str,
    _open: bool,
) -> Result<CodingSessionDto, AppError> {
    Err(AppError::InvalidInput("not implemented".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drills::runner::{CodeRunner, RunOutcome};
    use crate::lab_runtime::LearningLabFile;
    use crate::practice::coding::harness::python::PythonHarness;
    use crate::practice::coding::problem::{
        Case, Compare, Difficulty, Misreading, Param, Problem, ProblemSpec, Signature, Visibility,
    };
    use async_trait::async_trait;
    use std::collections::BTreeMap;
    use tokio_util::sync::CancellationToken;

    struct TestRunner {
        outcome: RunOutcome,
        output: String,
    }

    #[async_trait]
    impl CodeRunner for TestRunner {
        fn name(&self) -> &str {
            "test-runner"
        }
        fn entrypoint(&self) -> &str {
            "checks.py"
        }
        fn contract(&self) -> &str {
            "test"
        }
        fn running(&self, _path: &str) -> String {
            String::new()
        }
        async fn run(
            &self,
            _files: &[lattice_core::domain::learning::drills::DrillFile],
            _test: &str,
            _cancel: CancellationToken,
        ) -> lattice_core::shared::error::Result<RunOutcome> {
            Ok(self.outcome.clone())
        }
        async fn run_capturing(
            &self,
            _files: &[LearningLabFile],
            _read_only_paths: &[String],
            _cancel: CancellationToken,
        ) -> lattice_core::shared::error::Result<(RunOutcome, String)> {
            Ok((self.outcome.clone(), self.output.clone()))
        }
    }

    fn sample_problem() -> Problem {
        let spec = ProblemSpec {
            title: "Two Sum".into(),
            statement: "Find indices.".into(),
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
            constraints: vec![],
            clarifications: vec![],
            language: "python".into(),
            difficulty: Difficulty::Easy,
            target_complexity: "O(n)".into(),
            patterns: vec![],
            starter: "def two_sum(numbers, target):\n    pass\n".into(),
        };

        Problem {
            spec,
            reference: "def two_sum(numbers, target):\n    seen = {}\n    for i, x in enumerate(numbers):\n        if target - x in seen:\n            return [seen[target - x], i]\n        seen[x] = i\n    return []\n".into(),
            misreadings: vec![Misreading {
                id: "mis-duplicate".into(),
                description: "Reusing same index".into(),
                code: "def two_sum(numbers, target):\n    return [0, 0]\n".into(),
                killed_by: vec!["case-1".into()],
            }],
            cases: vec![Case {
                id: "case-1".into(),
                category: "basic".into(),
                args: vec!["[2, 7]".into(), "9".into()],
                expected: Some("[0, 1]".into()),
                compare: Compare::Exact,
                visibility: Visibility::Example,
                scale: false,
            }],
            reference_ms: BTreeMap::new(),
        }
    }

    #[test]
    fn test_setting_discloses_all_variants_across_settings() {
        // FailingCaseInput
        assert!(discloses(Setting::Coached, Hidden::FailingCaseInput, false, false));
        assert!(discloses(Setting::Coached, Hidden::FailingCaseInput, true, false));
        assert!(!discloses(Setting::Mock, Hidden::FailingCaseInput, true, false));
        assert!(discloses(Setting::Mock, Hidden::FailingCaseInput, true, true));
        assert!(!discloses(Setting::Watched, Hidden::FailingCaseInput, true, false));
        assert!(discloses(Setting::Watched, Hidden::FailingCaseInput, true, true));

        // UncaughtMisreading
        assert!(!discloses(Setting::Coached, Hidden::UncaughtMisreading, false, false));
        assert!(discloses(Setting::Coached, Hidden::UncaughtMisreading, true, false));
        assert!(discloses(Setting::Coached, Hidden::UncaughtMisreading, false, true));
        assert!(!discloses(Setting::Mock, Hidden::UncaughtMisreading, true, false));
        assert!(discloses(Setting::Mock, Hidden::UncaughtMisreading, true, true));
        assert!(!discloses(Setting::Watched, Hidden::UncaughtMisreading, true, false));
        assert!(discloses(Setting::Watched, Hidden::UncaughtMisreading, true, true));

        // WorkedSolution
        assert!(!discloses(Setting::Coached, Hidden::WorkedSolution, false, false));
        assert!(discloses(Setting::Coached, Hidden::WorkedSolution, true, false));
        assert!(!discloses(Setting::Mock, Hidden::WorkedSolution, true, false));
        assert!(discloses(Setting::Mock, Hidden::WorkedSolution, false, true));
        assert!(!discloses(Setting::Watched, Hidden::WorkedSolution, true, false));
        assert!(discloses(Setting::Watched, Hidden::WorkedSolution, false, true));
    }

    #[tokio::test]
    async fn test_check_coding_example_verdicts_and_catches() {
        let problem = sample_problem();
        let harness = PythonHarness::new();

        // Right example: reference agrees, and catches mis-duplicate (which returns [0, 0] instead of [0, 1])
        let runner_right = TestRunner {
            outcome: RunOutcome::Passed,
            output: "__LATTICE_CASE__:{\"case_id\":\"ex\",\"status\":\"passed\",\"actual\":\"[0, 1]\",\"printed\":\"\",\"elapsed_ms\":1}\n".into(),
        };
        let right_ex = evaluate_example(
            &runner_right,
            &harness,
            &problem,
            r#"["[2, 7]", "9"]"#,
            "[0, 1]",
        )
        .await
        .expect("Right example should evaluate");
        assert_eq!(right_ex.verdict, ExampleVerdict::Right);
        assert!(
            right_ex.catches.iter().any(|c| c.contains("Reusing same index")),
            "Expected caught misreading"
        );

        // Wrong example: reference produces [0, 1], expected says [9, 9]
        let runner_wrong = TestRunner {
            outcome: RunOutcome::Passed,
            output: "__LATTICE_CASE__:{\"case_id\":\"ex\",\"status\":\"passed\",\"actual\":\"[0, 1]\",\"printed\":\"\",\"elapsed_ms\":1}\n".into(),
        };
        let wrong_ex = evaluate_example(
            &runner_wrong,
            &harness,
            &problem,
            r#"["[2, 7]", "9"]"#,
            "[9, 9]",
        )
        .await
        .expect("Wrong example should evaluate");
        assert_eq!(wrong_ex.verdict, ExampleVerdict::Wrong);
        assert!(wrong_ex.catches.is_empty());

        // Invalid example: invalid python literal in args
        let runner_inv = TestRunner {
            outcome: RunOutcome::Failed { failed: 1, passed: 0 },
            output: String::new(),
        };
        let invalid_ex = evaluate_example(
            &runner_inv,
            &harness,
            &problem,
            "INVALID_SYNTAX_ARGS(((",
            "[0, 1]",
        )
        .await
        .expect("Invalid syntax should produce Invalid verdict");
        assert_eq!(invalid_ex.verdict, ExampleVerdict::Invalid);
    }

    #[test]
    fn test_stage_timing_dto_calculation() {
        let stage_events = vec![
            (CodingStage::Understand, 1000),
            (CodingStage::Plan, 5000),
            (CodingStage::Code, 15000),
        ];
        let timings = calculate_stage_timings(&stage_events, CodingStage::Code, 20000);

        assert_eq!(timings.len(), 3);
        assert_eq!(timings[0].stage, CodingStage::Understand);
        assert_eq!(timings[0].elapsed_ms, 4000);
        assert_eq!(timings[1].stage, CodingStage::Plan);
        assert_eq!(timings[1].elapsed_ms, 10000);
        assert_eq!(timings[2].stage, CodingStage::Code);
        assert_eq!(timings[2].elapsed_ms, 5000);
    }

    #[test]
    fn test_set_coding_clock_pause_resume() {
        // Open session at 1000
        let (active, opened) = compute_active_clock(0, None, 1000, true);
        assert_eq!(active, 0);
        assert_eq!(opened, Some(1000));

        // Pause/close session at 5000 (accumulated 4000 ms)
        let (active_paused, opened_paused) = compute_active_clock(active, opened, 5000, false);
        assert_eq!(active_paused, 4000);
        assert_eq!(opened_paused, None);

        // Resume/reopen session at 10000
        let (active_resumed, opened_resumed) = compute_active_clock(active_paused, opened_paused, 10000, true);
        assert_eq!(active_resumed, 4000);
        assert_eq!(opened_resumed, Some(10000));

        // Close session at 13000 (additional 3000 ms -> total 7000 ms)
        let (active_final, opened_final) = compute_active_clock(active_resumed, opened_resumed, 13000, false);
        assert_eq!(active_final, 7000);
        assert_eq!(opened_final, None);
    }
}
