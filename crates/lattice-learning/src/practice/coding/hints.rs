//! Graded coding hint ladder and validation checks (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md section 4.5).

use serde::{Deserialize, Serialize};

use crate::drills::runner::CodeRunner;
use crate::practice::coding::harness::CaseHarness;
use crate::practice::coding::problem::Problem;
use crate::practice::coding::session::CodingExampleDto;
use crate::practice::practical_dto::LearningPracticalRunDto;
use lattice_core::application::ports::llm_port::LLMPort;
use lattice_core::shared::error::AppError;

/// The four rungs of the coding hint ladder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CodeHintRung {
    /// Rung 1: A question about what to look at. Never gives code.
    Nudge,
    /// Rung 2: The idea or structure that fits, and why. Never gives code.
    Idea,
    /// Rung 3: Steps in pseudo-code, each with its reason. Never gives runnable Python.
    Plan,
    /// Rung 4: The lines in your code that are wrong, and why. Never gives the fixed line.
    PointAtLine,
}

impl CodeHintRung {
    /// The next rung in sequence, or `None` if already at the top rung.
    pub fn next(self) -> Option<Self> {
        match self {
            Self::Nudge => Some(Self::Idea),
            Self::Idea => Some(Self::Plan),
            Self::Plan => Some(Self::PointAtLine),
            Self::PointAtLine => None,
        }
    }
}

/// The level of assistance permitted in the current setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum HelpLevel {
    FullHelp,
    HintsOnly,
    NoHelp,
}

/// A kind of help that may be requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Help {
    CodeHint(CodeHintRung),
    WorkedSolution,
}

/// What help a surface allows for a given help level.
pub trait AssistancePolicy: Send + Sync {
    fn allows(&self, help: Help, level: HelpLevel) -> bool;
}

/// Assistance policy for coding sessions.
#[derive(Debug, Clone, Copy, Default)]
pub struct CodingAssistancePolicy;

impl AssistancePolicy for CodingAssistancePolicy {
    fn allows(&self, _help: Help, _level: HelpLevel) -> bool {
        false
    }
}

/// A validated hint safe to display to the learner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CheckedCodeHint {
    pub rung: CodeHintRung,
    pub content: String,
    pub reason: String,
    #[serde(default)]
    pub lines: Vec<u32>,
}

/// Validates that a generated hint obeys the constraints for its rung and does not leak solutions.
pub async fn check_code_hint(
    _hint: &CheckedCodeHint,
    _draft: &str,
    _problem: &Problem,
    _runner: &dyn CodeRunner,
    _harness: &dyn CaseHarness,
) -> Result<(), AppError> {
    Err(AppError::InvalidInput("not implemented".into()))
}

/// Generates and validates the next hint for the learner's current draft and progress.
pub async fn generate_code_hint(
    _llm: &dyn LLMPort,
    _runner: &dyn CodeRunner,
    _harness: &dyn CaseHarness,
    _rung: CodeHintRung,
    _problem: &Problem,
    _draft: &str,
    _last_run: Option<&LearningPracticalRunDto>,
    _examples: &[CodingExampleDto],
) -> Result<CheckedCodeHint, AppError> {
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
            statement: "Find indices of two numbers that add up to target.".into(),
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
            constraints: vec!["2 <= len(numbers) <= 1000".into()],
            clarifications: vec![],
            language: "python".into(),
            difficulty: Difficulty::Easy,
            target_complexity: "O(n) time, O(n) space".into(),
            patterns: vec!["hash_lookup".into()],
            starter: "def two_sum(numbers: list[int], target: int) -> list[int]:\n    pass\n".into(),
        };

        Problem {
            spec,
            reference: "def two_sum(numbers: list[int], target: int) -> list[int]:\n    seen = {}\n    for i, num in enumerate(numbers):\n        diff = target - num\n        if diff in seen:\n            return [seen[diff], i]\n        seen[num] = i\n    return []\n".into(),
            misreadings: vec![Misreading {
                id: "mis-1".into(),
                description: "Reusing same element".into(),
                code: "def two_sum(numbers, target):\n    return [0, 0]\n".into(),
                killed_by: vec!["case-1".into()],
            }],
            cases: vec![Case {
                id: "case-1".into(),
                category: "basic".into(),
                args: vec!["[2, 7, 11, 15]".into(), "9".into()],
                expected: Some("[0, 1]".into()),
                compare: Compare::Exact,
                visibility: Visibility::Example,
                scale: false,
            }],
            reference_ms: BTreeMap::new(),
        }
    }

    #[tokio::test]
    async fn test_nudge_rejects_fenced_code_or_python_statements() {
        let runner = TestRunner {
            outcome: RunOutcome::Passed,
            output: String::new(),
        };
        let harness = PythonHarness::new();
        let problem = sample_problem();
        let draft = "def two_sum(numbers, target):\n    pass\n";

        let bad_nudge_fenced = CheckedCodeHint {
            rung: CodeHintRung::Nudge,
            content: "Why not try:\n```python\nseen = {}\n```".into(),
            reason: "Encouraging a dict".into(),
            lines: vec![],
        };
        let err = check_code_hint(&bad_nudge_fenced, draft, &problem, &runner, &harness)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("cannot contain code"), "got {err}");

        let bad_nudge_statement = CheckedCodeHint {
            rung: CodeHintRung::Nudge,
            content: "Consider using return [0, 1]".into(),
            reason: "Pointing at return".into(),
            lines: vec![],
        };
        let err2 = check_code_hint(&bad_nudge_statement, draft, &problem, &runner, &harness)
            .await
            .unwrap_err();
        assert!(err2.to_string().contains("cannot contain code"), "got {err2}");

        let good_nudge = CheckedCodeHint {
            rung: CodeHintRung::Nudge,
            content: "What data structure allows you to look up previously seen values in O(1) time?".into(),
            reason: "Guiding toward hash map".into(),
            lines: vec![],
        };
        assert!(check_code_hint(&good_nudge, draft, &problem, &runner, &harness).await.is_ok());
    }

    #[tokio::test]
    async fn test_idea_rejects_fenced_code_or_python_statements() {
        let runner = TestRunner {
            outcome: RunOutcome::Passed,
            output: String::new(),
        };
        let harness = PythonHarness::new();
        let problem = sample_problem();
        let draft = "def two_sum(numbers, target):\n    pass\n";

        let bad_idea = CheckedCodeHint {
            rung: CodeHintRung::Idea,
            content: "Store complements: `diff = target - num`".into(),
            reason: "Showing the equation".into(),
            lines: vec![],
        };
        let err = check_code_hint(&bad_idea, draft, &problem, &runner, &harness)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("cannot contain code"), "got {err}");

        let good_idea = CheckedCodeHint {
            rung: CodeHintRung::Idea,
            content: "As you iterate through the list, keep track of each number and its index in a hash map so you can check if the complement was already seen.".into(),
            reason: "Explaining single-pass hash map strategy".into(),
            lines: vec![],
        };
        assert!(check_code_hint(&good_idea, draft, &problem, &runner, &harness).await.is_ok());
    }

    #[tokio::test]
    async fn test_plan_rejects_runnable_python() {
        let runner = TestRunner {
            outcome: RunOutcome::Passed,
            output: String::new(),
        };
        let harness = PythonHarness::new();
        let problem = sample_problem();
        let draft = "def two_sum(numbers, target):\n    pass\n";

        let bad_plan = CheckedCodeHint {
            rung: CodeHintRung::Plan,
            content: "```python\ndef solve():\n    seen = {}\n    for i, x in enumerate(nums):\n        if target - x in seen:\n            return [seen[target - x], i]\n        seen[x] = i\n```".into(),
            reason: "Complete runnable solution".into(),
            lines: vec![],
        };
        let err = check_code_hint(&bad_plan, draft, &problem, &runner, &harness)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("runnable Python"), "got {err}");

        let good_plan = CheckedCodeHint {
            rung: CodeHintRung::Plan,
            content: "1. Initialize an empty dictionary mapping numbers to their indices.\n2. Iterate through numbers with their index:\n   - Compute complement = target - current number\n   - If complement is in the dictionary, return [complement_index, current_index]\n   - Otherwise, record current number and its index in the dictionary.\n3. Return empty list if no pair is found.".into(),
            reason: "Algorithmic steps in pseudo-code".into(),
            lines: vec![],
        };
        assert!(check_code_hint(&good_plan, draft, &problem, &runner, &harness).await.is_ok());
    }

    #[tokio::test]
    async fn test_point_at_line_requires_non_empty_lines_within_draft() {
        let runner = TestRunner {
            outcome: RunOutcome::Passed,
            output: String::new(),
        };
        let harness = PythonHarness::new();
        let problem = sample_problem();
        let draft = "def two_sum(numbers, target):\n    seen = {}\n    for i in numbers:\n        return [0]\n"; // 4 lines

        let empty_lines = CheckedCodeHint {
            rung: CodeHintRung::PointAtLine,
            content: "Look at the return statement".into(),
            reason: "Wrong return".into(),
            lines: vec![],
        };
        let err = check_code_hint(&empty_lines, draft, &problem, &runner, &harness)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("non-empty lines"), "got {err}");

        let line_zero = CheckedCodeHint {
            rung: CodeHintRung::PointAtLine,
            content: "Look at the definition".into(),
            reason: "Zero line".into(),
            lines: vec![0],
        };
        let err2 = check_code_hint(&line_zero, draft, &problem, &runner, &harness)
            .await
            .unwrap_err();
        assert!(err2.to_string().contains("1..=draft_line_count"), "got {err2}");

        let line_out_of_bounds = CheckedCodeHint {
            rung: CodeHintRung::PointAtLine,
            content: "Look past the end".into(),
            reason: "Line 10 does not exist".into(),
            lines: vec![10],
        };
        let err3 = check_code_hint(&line_out_of_bounds, draft, &problem, &runner, &harness)
            .await
            .unwrap_err();
        assert!(err3.to_string().contains("1..=draft_line_count"), "got {err3}");

        let valid_line = CheckedCodeHint {
            rung: CodeHintRung::PointAtLine,
            content: "You are returning too early inside the loop without checking the complement.".into(),
            reason: "Premature return on line 4".into(),
            lines: vec![4],
        };
        assert!(check_code_hint(&valid_line, draft, &problem, &runner, &harness).await.is_ok());
    }

    #[tokio::test]
    async fn test_hint_rejects_code_passing_all_problem_cases() {
        let runner = TestRunner {
            outcome: RunOutcome::Passed,
            output: "__LATTICE_CASE__:{\"case_id\":\"case-1\",\"status\":\"passed\",\"actual\":\"[0, 1]\",\"printed\":\"\",\"elapsed_ms\":1}\n".into(),
        };
        let harness = PythonHarness::new();
        let problem = sample_problem();
        let draft = "def two_sum(numbers, target):\n    pass\n";

        let passing_code_hint = CheckedCodeHint {
            rung: CodeHintRung::Plan,
            content: "Try writing:\ndef two_sum(numbers, target):\n    return [0, 1]".into(),
            reason: "Passing solution snippet".into(),
            lines: vec![],
        };
        let err = check_code_hint(&passing_code_hint, draft, &problem, &runner, &harness)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("passes all cases"), "got {err}");
    }

    #[tokio::test]
    async fn test_hint_rejects_verbatim_reference_line() {
        let runner = TestRunner {
            outcome: RunOutcome::Failed { failed: 1, passed: 0 },
            output: String::new(),
        };
        let harness = PythonHarness::new();
        let problem = sample_problem();
        let draft = "def two_sum(numbers, target):\n    pass\n";

        let verbatim_hint = CheckedCodeHint {
            rung: CodeHintRung::Plan,
            content: "You should use `seen[num] = i` in your loop.".into(),
            reason: "Verbatim line from reference".into(),
            lines: vec![],
        };
        let err = check_code_hint(&verbatim_hint, draft, &problem, &runner, &harness)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("verbatim reference line"), "got {err}");
    }

    #[test]
    fn test_coding_assistance_policy_allows() {
        let policy = CodingAssistancePolicy;

        assert!(policy.allows(Help::CodeHint(CodeHintRung::Nudge), HelpLevel::FullHelp));
        assert!(policy.allows(Help::CodeHint(CodeHintRung::Idea), HelpLevel::FullHelp));
        assert!(policy.allows(Help::CodeHint(CodeHintRung::Plan), HelpLevel::FullHelp));
        assert!(policy.allows(Help::CodeHint(CodeHintRung::PointAtLine), HelpLevel::FullHelp));
        assert!(policy.allows(Help::WorkedSolution, HelpLevel::FullHelp));

        assert!(policy.allows(Help::CodeHint(CodeHintRung::Nudge), HelpLevel::HintsOnly));
        assert!(policy.allows(Help::CodeHint(CodeHintRung::Idea), HelpLevel::HintsOnly));
        assert!(policy.allows(Help::CodeHint(CodeHintRung::Plan), HelpLevel::HintsOnly));
        assert!(policy.allows(Help::CodeHint(CodeHintRung::PointAtLine), HelpLevel::HintsOnly));
        assert!(!policy.allows(Help::WorkedSolution, HelpLevel::HintsOnly));

        assert!(!policy.allows(Help::CodeHint(CodeHintRung::Nudge), HelpLevel::NoHelp));
        assert!(!policy.allows(Help::WorkedSolution, HelpLevel::NoHelp));
    }
}
