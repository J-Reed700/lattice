//! Where coding problems come from: `ProblemSource` trait and `GeneratedProblems`
//! which uses an LLM to generate problems, proves them, and reports progress via `BuildSteps`.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::drills::runner::CodeRunner;
use crate::practice::coding::harness::CaseHarness;
use crate::practice::coding::problem::{CodingTask, Difficulty, Problem, Setting};
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::practice::problem_patterns::statement_names_pattern;
use lattice_core::shared::error::{AppError, Result};

/// The three named build steps in order: drafting, proving reference, and verifying misreadings.
pub const BUILD_STEP_NAMES: [&str; 3] = [
    "Drafting problem statement and cases",
    "Proving reference solution",
    "Verifying misreadings",
];

/// Progress reporter for problem build steps.
#[async_trait]
pub trait BuildSteps: Send + Sync {
    async fn step(&self, index: u32, message: &str) -> Result<bool>;
}

/// A request to generate or locate a coding problem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProblemRequest {
    pub program_id: String,
    pub lesson_id: Option<String>,
    pub task: CodingTask,
    pub setting: Setting,
    pub language: String,
    pub difficulty: Difficulty,
    pub focus: Option<String>,
    pub preparing_for: Option<String>,
    pub patterns: Vec<String>,
}

/// A source of proven coding problems.
#[async_trait]
pub trait ProblemSource: Send + Sync {
    async fn problem(
        &self,
        request: &ProblemRequest,
        steps: &dyn BuildSteps,
        cancel: CancellationToken,
    ) -> Result<Problem>;
}

/// Generates coding problems via LLM and proves them through the code runner and harness.
pub struct GeneratedProblems<'a> {
    pub llm: &'a dyn LLMPort,
    pub runner: &'a dyn CodeRunner,
    pub harness: &'a dyn CaseHarness,
}

impl<'a> GeneratedProblems<'a> {
    pub fn new(
        llm: &'a dyn LLMPort,
        runner: &'a dyn CodeRunner,
        harness: &'a dyn CaseHarness,
    ) -> Self {
        Self {
            llm,
            runner,
            harness,
        }
    }

    /// Verifies that the problem statement does not name its pattern.
    pub fn validate_statement(&self, statement: &str, patterns: &[String]) -> Result<()> {
        if let Some(named) = statement_names_pattern(statement, patterns) {
            return Err(AppError::InvalidInput(format!(
                "Problem statement must not name pattern '{named}'"
            )));
        }
        Ok(())
    }
}

#[async_trait]
impl<'a> ProblemSource for GeneratedProblems<'a> {
    async fn problem(
        &self,
        _request: &ProblemRequest,
        _steps: &dyn BuildSteps,
        _cancel: CancellationToken,
    ) -> Result<Problem> {
        // Stage 1 non-panicking stub: returns an error indicating generation is not yet implemented.
        Err(AppError::InvalidInput(
            "Stage 1 stub: problem generation not implemented".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drills::runner::RunOutcome;
    use crate::lab_runtime::LearningLabFile;
    use crate::practice::coding::problem::{Case, CaseResult};
    use async_trait::async_trait;
    use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
    use lattice_core::domain::learning::drills::DrillFile;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    struct MockLLM {
        call_count: AtomicUsize,
    }

    impl MockLLM {
        fn new() -> Self {
            Self {
                call_count: AtomicUsize::new(0),
            }
        }

        fn calls(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl LLMPort for MockLLM {
        async fn complete(&self, _request: &CompletionRequest) -> Result<CompletionResponse> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(CompletionResponse::from_text("{}"))
        }

        fn model_name(&self) -> &str {
            "mock-llm"
        }

        fn max_context_tokens(&self) -> usize {
            8192
        }

        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }

    struct MockRunner;

    #[async_trait]
    impl CodeRunner for MockRunner {
        fn name(&self) -> &str {
            "mock-runner"
        }
        fn entrypoint(&self) -> &str {
            "solution.py"
        }
        fn contract(&self) -> &str {
            "Python 3.14"
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
            Ok(RunOutcome::Passed)
        }
    }

    struct MockHarness;

    impl CaseHarness for MockHarness {
        fn evaluator(
            &self,
            _problem: &crate::practice::coding::problem::ProblemSpec,
            _cases: &[Case],
            _nonce: &str,
        ) -> Vec<LearningLabFile> {
            Vec::new()
        }
        fn results(&self, _stdout: &str, _nonce: &str) -> Vec<CaseResult> {
            Vec::new()
        }
    }

    struct RecordingSteps {
        steps: Mutex<Vec<(u32, String)>>,
    }

    impl RecordingSteps {
        fn new() -> Self {
            Self {
                steps: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl BuildSteps for RecordingSteps {
        async fn step(&self, index: u32, message: &str) -> Result<bool> {
            if let Ok(mut lock) = self.steps.lock() {
                lock.push((index, message.to_string()));
            }
            Ok(true)
        }
    }

    #[tokio::test]
    async fn test_reference_solution_fails_case_is_rejected_and_asked_for_again() {
        let llm = MockLLM::new();
        let runner = MockRunner;
        let harness = MockHarness;
        let generator = GeneratedProblems::new(&llm, &runner, &harness);
        let steps = RecordingSteps::new();

        let request = ProblemRequest {
            program_id: "prog-1".into(),
            lesson_id: Some("les-1".into()),
            task: CodingTask::Problem,
            setting: Setting::Coached,
            language: "python".into(),
            difficulty: Difficulty::Easy,
            focus: None,
            preparing_for: None,
            patterns: vec!["hash_map_lookup".into()],
        };

        let result = generator
            .problem(&request, &steps, CancellationToken::new())
            .await;
        assert!(
            result.is_ok(),
            "problem generation should succeed after re-asking"
        );
        assert!(
            llm.calls() >= 2,
            "LLM must be queried again when reference solution fails a case"
        );
    }

    #[test]
    fn test_problem_statement_naming_pattern_is_refused() {
        let llm = MockLLM::new();
        let runner = MockRunner;
        let harness = MockHarness;
        let generator = GeneratedProblems::new(&llm, &runner, &harness);

        let statement_with_pattern =
            "Solve this problem by using a Hash Map Lookup to store items.";
        let patterns = vec!["hash_map_lookup".to_string()];
        let result = generator.validate_statement(statement_with_pattern, &patterns);
        assert!(
            result.is_err(),
            "problem statement naming its pattern must be refused"
        );

        let clean_statement = "Given a list of numbers, find two numbers that sum to the target.";
        let clean_result = generator.validate_statement(clean_statement, &patterns);
        assert!(
            clean_result.is_ok(),
            "problem statement without pattern name must be accepted"
        );
    }

    #[tokio::test]
    async fn test_build_steps_records_three_named_build_steps_in_order() {
        let llm = MockLLM::new();
        let runner = MockRunner;
        let harness = MockHarness;
        let generator = GeneratedProblems::new(&llm, &runner, &harness);
        let steps = RecordingSteps::new();

        let request = ProblemRequest {
            program_id: "prog-1".into(),
            lesson_id: Some("les-1".into()),
            task: CodingTask::Problem,
            setting: Setting::Coached,
            language: "python".into(),
            difficulty: Difficulty::Easy,
            focus: None,
            preparing_for: None,
            patterns: vec!["two_pointers".into()],
        };

        let result = generator
            .problem(&request, &steps, CancellationToken::new())
            .await;
        assert!(result.is_ok(), "problem generation should succeed");

        let recorded = steps.steps.lock().unwrap().clone();
        assert_eq!(recorded.len(), 3, "must record exactly 3 build steps");
        assert_eq!(recorded[0].0, 0);
        assert_eq!(recorded[0].1, BUILD_STEP_NAMES[0]);
        assert_eq!(recorded[1].0, 1);
        assert_eq!(recorded[1].1, BUILD_STEP_NAMES[1]);
        assert_eq!(recorded[2].0, 2);
        assert_eq!(recorded[2].1, BUILD_STEP_NAMES[2]);
    }
}
