//! Counterpart turn policies (Coach, Interviewer, WatchingInterviewer)
//! (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md sections 4.6 and 7.1).

use serde::{Deserialize, Serialize};

use crate::drills::runner::CodeRunner;
use crate::practice::coding::harness::CaseHarness;
use crate::practice::coding::observation::WorkspaceObservation;
use crate::practice::coding::problem::Problem;
use crate::practice::coding::session::{CodingStage, RunScope};
use lattice_core::shared::error::AppError;

/// Cues that trigger counterpart speech.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cue {
    Opening,
    StageAdvance,
    ExampleFeedback,
    RunFeedback,
    FollowUp,
    HintProvided,
    DraftCheckpoint,
}

/// Events in a coding session that may cue the counterpart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEvent {
    Started,
    StageChanged(CodingStage),
    ExampleChecked,
    RunFinished(RunScope),
    LearnerTurn(String),
    HintRequested,
    DraftPaused,
}

/// Decides when the counterpart speaks, its prompt, and validates replies.
pub trait TurnPolicy<S>: Send + Sync {
    fn cue(&self, event: &SessionEvent, state: &S) -> Option<Cue>;
    fn system(&self, cue: &Cue, state: &S) -> String;
    fn check(&self, content: &str, cue: &Cue, state: &S) -> Result<(), AppError>;
}

/// Policy for Coached setting: guides and explains, cues frequently, never pauses.
#[derive(Debug, Clone, Copy, Default)]
pub struct CoachPolicy;

impl TurnPolicy<WorkspaceObservation> for CoachPolicy {
    fn cue(&self, _event: &SessionEvent, _state: &WorkspaceObservation) -> Option<Cue> {
        None
    }

    fn system(&self, _cue: &Cue, _state: &WorkspaceObservation) -> String {
        String::new()
    }

    fn check(&self, _content: &str, _cue: &Cue, _state: &WorkspaceObservation) -> Result<(), AppError> {
        Err(AppError::InvalidInput("not implemented".into()))
    }
}

/// Policy for Mock setting: in-role interviewer, asks <= 1 question per turn.
#[derive(Debug, Clone, Copy, Default)]
pub struct InterviewerPolicy;

impl TurnPolicy<WorkspaceObservation> for InterviewerPolicy {
    fn cue(&self, _event: &SessionEvent, _state: &WorkspaceObservation) -> Option<Cue> {
        None
    }

    fn system(&self, _cue: &Cue, _state: &WorkspaceObservation) -> String {
        String::new()
    }

    fn check(&self, _content: &str, _cue: &Cue, _state: &WorkspaceObservation) -> Result<(), AppError> {
        Err(AppError::InvalidInput("not implemented".into()))
    }
}

/// Policy for Watched setting: interviewer watching draft checkpoints and runs; may choose silence.
#[derive(Debug, Clone, Copy, Default)]
pub struct WatchingInterviewerPolicy;

impl TurnPolicy<WorkspaceObservation> for WatchingInterviewerPolicy {
    fn cue(&self, _event: &SessionEvent, _state: &WorkspaceObservation) -> Option<Cue> {
        None
    }

    fn system(&self, _cue: &Cue, _state: &WorkspaceObservation) -> String {
        String::new()
    }

    fn check(&self, _content: &str, _cue: &Cue, _state: &WorkspaceObservation) -> Result<(), AppError> {
        Err(AppError::InvalidInput("not implemented".into()))
    }
}

/// Structured response from model at a watched checkpoint, allowing silence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchedCheckpointResponse {
    pub speak: bool,
    #[serde(default)]
    pub content: String,
}

/// Parses raw model response at a watched checkpoint, returning `Ok(None)` when model chooses silence.
pub fn parse_watched_reply(_raw: &str) -> Result<Option<String>, AppError> {
    Err(AppError::InvalidInput("not implemented".into()))
}

/// Checks counterpart reply against shared constraints:
/// - Policy's own text rules (e.g. <=1 question for interviewers)
/// - Valid line references (1..=draft_line_count)
/// - No internal UUID leaks
/// - Any fenced code block is tested via harness and rejected if it passes all cases
pub async fn check_counterpart_reply(
    _reply: &str,
    _policy: &dyn TurnPolicy<WorkspaceObservation>,
    _cue: &Cue,
    _observation: &WorkspaceObservation,
    _problem: &Problem,
    _runner: &dyn CodeRunner,
    _harness: &dyn CaseHarness,
) -> Result<(), AppError> {
    Err(AppError::InvalidInput("not implemented".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drills::runner::{CodeRunner, RunOutcome};
    use crate::lab_runtime::LearningLabFile;
    use crate::practice::coding::harness::python::PythonHarness;
    use crate::practice::coding::problem::{
        Case, Compare, Difficulty, Param, Problem, ProblemSpec, Setting, Signature, Visibility,
    };
    use crate::practice::coding::session::{CodingStage, PublicProblemDto, RunScope};
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
            constraints: vec![],
            clarifications: vec![],
            language: "python".into(),
            difficulty: Difficulty::Easy,
            target_complexity: "O(n) time, O(n) space".into(),
            patterns: vec![],
            starter: "def two_sum(numbers: list[int], target: int) -> list[int]:\n    pass\n".into(),
        };

        Problem {
            spec,
            reference: "def two_sum(numbers: list[int], target: int) -> list[int]:\n    seen = {}\n    for i, num in enumerate(numbers):\n        diff = target - num\n        if diff in seen:\n            return [seen[diff], i]\n        seen[num] = i\n    return []\n".into(),
            misreadings: vec![],
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

    fn sample_observation(setting: Setting, stage: CodingStage) -> WorkspaceObservation {
        let problem = sample_problem();
        WorkspaceObservation {
            task: crate::practice::coding::problem::CodingTask::Problem,
            setting,
            stage,
            public_problem: PublicProblemDto {
                title: problem.spec.title.clone(),
                statement: problem.spec.statement.clone(),
                signature: problem.spec.signature.clone(),
                examples: problem.cases.clone(),
                constraints: vec![],
                subgoals: vec![],
                files: vec![],
                worked_solution: None,
                target_complexity: Some(problem.spec.target_complexity.clone()),
                total_misreadings: 0,
            },
            clarifications: vec![],
            target_complexity: "O(n)".into(),
            patterns: vec![],
            active_ms: 5000,
            budget_seconds: Some(1800),
            files: vec![LearningLabFile {
                path: "solution.py".into(),
                content: "def two_sum(numbers, target):\n    # line 2\n    # line 3\n    # line 4\n    return []\n".into(),
            }],
            numbered_solution: "1 | def two_sum(numbers, target):\n2 |     # line 2\n3 |     # line 3\n4 |     # line 4\n5 |     return []\n".into(),
            changed_since_last_seen: None,
            last_run: None,
            examples: vec![],
            hints_given: vec![],
            turns: vec![],
        }
    }

    #[test]
    fn test_coach_policy_cue() {
        let policy = CoachPolicy;
        let obs = sample_observation(Setting::Coached, CodingStage::Understand);

        assert_eq!(policy.cue(&SessionEvent::Started, &obs), Some(Cue::Opening));
        assert_eq!(
            policy.cue(&SessionEvent::StageChanged(CodingStage::Plan), &obs),
            Some(Cue::StageAdvance)
        );
        assert_eq!(policy.cue(&SessionEvent::ExampleChecked, &obs), Some(Cue::ExampleFeedback));
        assert_eq!(
            policy.cue(&SessionEvent::RunFinished(RunScope::Examples), &obs),
            Some(Cue::RunFeedback)
        );
        assert_eq!(
            policy.cue(&SessionEvent::LearnerTurn("How should I start?".into()), &obs),
            Some(Cue::FollowUp)
        );
        assert_eq!(policy.cue(&SessionEvent::HintRequested, &obs), Some(Cue::HintProvided));
        assert_eq!(policy.cue(&SessionEvent::DraftPaused, &obs), None);
    }

    #[test]
    fn test_interviewer_policy_cue() {
        let policy = InterviewerPolicy;
        let obs = sample_observation(Setting::Mock, CodingStage::Understand);

        assert_eq!(policy.cue(&SessionEvent::Started, &obs), Some(Cue::Opening));
        assert_eq!(
            policy.cue(&SessionEvent::StageChanged(CodingStage::Code), &obs),
            Some(Cue::StageAdvance)
        );
        assert_eq!(
            policy.cue(&SessionEvent::RunFinished(RunScope::All), &obs),
            Some(Cue::RunFeedback)
        );
        assert_eq!(
            policy.cue(&SessionEvent::LearnerTurn("I have an approach in mind.".into()), &obs),
            Some(Cue::FollowUp)
        );

        assert_eq!(policy.cue(&SessionEvent::DraftPaused, &obs), None);
        assert_eq!(policy.cue(&SessionEvent::ExampleChecked, &obs), None);
        assert_eq!(policy.cue(&SessionEvent::RunFinished(RunScope::Examples), &obs), None);
    }

    #[test]
    fn test_watching_interviewer_policy_cue() {
        let policy = WatchingInterviewerPolicy;
        let obs = sample_observation(Setting::Watched, CodingStage::Code);

        assert_eq!(policy.cue(&SessionEvent::Started, &obs), Some(Cue::Opening));
        assert_eq!(
            policy.cue(&SessionEvent::RunFinished(RunScope::Examples), &obs),
            Some(Cue::RunFeedback)
        );
        assert_eq!(policy.cue(&SessionEvent::DraftPaused, &obs), Some(Cue::DraftCheckpoint));
    }

    #[test]
    fn test_interviewer_policy_reply_check_question_limit() {
        let policy = InterviewerPolicy;
        let obs = sample_observation(Setting::Mock, CodingStage::Plan);

        let zero_q = "Let's begin when you are ready.";
        assert!(policy.check(zero_q, &Cue::FollowUp, &obs).is_ok());

        let one_q = "What data structure would give you O(1) lookups?";
        assert!(policy.check(one_q, &Cue::FollowUp, &obs).is_ok());

        let two_q = "What is the time complexity? Can we do it with less space?";
        let err = policy.check(two_q, &Cue::FollowUp, &obs).unwrap_err();
        assert!(err.to_string().contains("at most one question"), "got {err}");
    }

    #[tokio::test]
    async fn test_reply_check_validates_draft_line_numbers() {
        let policy = InterviewerPolicy;
        let obs = sample_observation(Setting::Mock, CodingStage::Code); // 5 lines in draft
        let problem = sample_problem();
        let runner = TestRunner { outcome: RunOutcome::Passed, output: String::new() };
        let harness = PythonHarness::new();

        let valid_line = "Notice what happens on line 3.";
        assert!(check_counterpart_reply(valid_line, &policy, &Cue::FollowUp, &obs, &problem, &runner, &harness).await.is_ok());

        let invalid_line = "Look at line 12 where the loop terminates.";
        let err = check_counterpart_reply(invalid_line, &policy, &Cue::FollowUp, &obs, &problem, &runner, &harness)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("draft line count"), "got {err}");
    }

    #[tokio::test]
    async fn test_reply_check_rejects_internal_uuids() {
        let policy = CoachPolicy;
        let obs = sample_observation(Setting::Coached, CodingStage::Code);
        let problem = sample_problem();
        let runner = TestRunner { outcome: RunOutcome::Passed, output: String::new() };
        let harness = PythonHarness::new();

        let leaked_uuid = "According to criterion 550e8400-e29b-41d4-a716-446655440000, your code is clear.";
        let err = check_counterpart_reply(leaked_uuid, &policy, &Cue::FollowUp, &obs, &problem, &runner, &harness)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("internal UUID"), "got {err}");
    }

    #[tokio::test]
    async fn test_reply_check_rejects_code_passing_all_cases() {
        let policy = CoachPolicy;
        let obs = sample_observation(Setting::Coached, CodingStage::Code);
        let problem = sample_problem();
        let runner = TestRunner {
            outcome: RunOutcome::Passed,
            output: "__LATTICE_CASE__:{\"case_id\":\"case-1\",\"status\":\"passed\",\"actual\":\"[0, 1]\",\"printed\":\"\",\"elapsed_ms\":1}\n".into(),
        };
        let harness = PythonHarness::new();

        let passing_code_reply = "Here is how to do it:\n```python\ndef two_sum(numbers, target):\n    return [0, 1]\n```";
        let err = check_counterpart_reply(passing_code_reply, &policy, &Cue::FollowUp, &obs, &problem, &runner, &harness)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("passes all cases"), "got {err}");
    }

    #[test]
    fn test_watching_interviewer_silence() {
        let silent_raw = r#"{"speak": false, "content": ""}"#;
        let parsed = parse_watched_reply(silent_raw).expect("should parse silence");
        assert_eq!(parsed, None);

        let spoken_raw = r#"{"speak": true, "content": "How are you handling duplicate values?"}"#;
        let parsed_spoken = parse_watched_reply(spoken_raw).expect("should parse spoken reply");
        assert_eq!(parsed_spoken, Some("How are you handling duplicate values?".into()));
    }
}
