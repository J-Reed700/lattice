//! Counterpart workspace observation builder (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md section 4.6).

use serde::{Deserialize, Serialize};

use crate::lab_runtime::LearningLabFile;
use crate::practice::coding::hints::CheckedCodeHint;
use crate::practice::coding::problem::{Clarification, CodingTask, Problem, Setting};
use crate::practice::coding::session::{CodingExampleDto, CodingSessionDto, CodingStage, PublicProblemDto};
use crate::practice::practical_dto::{LearningPracticalRunDto, LearningSimulationTurnDto};
use lattice_core::shared::error::AppError;

/// What the counterpart (coach or interviewer) sees during a coding session.
/// Never contains `reference` or `misreadings`!
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceObservation {
    pub task: CodingTask,
    pub setting: Setting,
    pub stage: CodingStage,
    pub public_problem: PublicProblemDto,
    pub clarifications: Vec<Clarification>,
    pub target_complexity: String,
    pub patterns: Vec<String>,
    pub active_ms: u64,
    pub budget_seconds: Option<u32>,
    /// Current learner files, plus numbered lines of `solution.py`.
    pub files: Vec<LearningLabFile>,
    pub numbered_solution: String,
    /// Summary or diff of what changed since the counterpart last observed the draft.
    pub changed_since_last_seen: Option<String>,
    pub last_run: Option<LearningPracticalRunDto>,
    pub examples: Vec<CodingExampleDto>,
    pub hints_given: Vec<CheckedCodeHint>,
    pub turns: Vec<LearningSimulationTurnDto>,
}

/// Formats code with 1-based line numbers for counterpart reference.
pub fn build_numbered_solution(_solution_code: &str) -> String {
    String::new()
}

/// Computes a human-readable diff or summary of changes since the counterpart last inspected the draft.
pub fn compute_changed_since_last_seen(
    _previous_draft: Option<&str>,
    _current_draft: &str,
) -> Option<String> {
    None
}

impl WorkspaceObservation {
    /// Builds a workspace observation from the session, draft, and problem.
    /// Strictly excludes any reference solution or misreadings.
    pub fn build(
        _session: &CodingSessionDto,
        _problem: &Problem,
        _draft_files: &[LearningLabFile],
        _last_seen_solution: Option<&str>,
    ) -> Result<Self, AppError> {
        Err(AppError::InvalidInput("not implemented".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::practice::coding::hints::CodeHintRung;
    use crate::practice::coding::problem::{
        Case, Compare, Difficulty, Misreading, Param, Problem, ProblemSpec, Signature, Visibility,
    };
    use crate::practice::coding::session::{CodingSessionDto, PublicProblemDto};
    use std::collections::BTreeMap;

    fn sample_problem_with_secrets() -> Problem {
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
            clarifications: vec![Clarification {
                question: "Can numbers be negative?".into(),
                answer: "Yes, numbers can be negative.".into(),
            }],
            language: "python".into(),
            difficulty: Difficulty::Easy,
            target_complexity: "O(n) time, O(n) space".into(),
            patterns: vec!["hash_lookup".into()],
            starter: "def two_sum(numbers: list[int], target: int) -> list[int]:\n    pass\n".into(),
        };

        Problem {
            spec,
            reference: "SECRET_REFERENCE_CODE_HASHMAP_SOLVE_XYZ123".into(),
            misreadings: vec![Misreading {
                id: "SECRET_MISREADING_ID_QUADRATIC_LOOP".into(),
                description: "SECRET_MISREADING_DESCRIPTION_FORGOT_INDEX".into(),
                code: "SECRET_MISREADING_CODE_WRONG_RESULT".into(),
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

    fn sample_session(problem: &Problem) -> CodingSessionDto {
        let public_problem = PublicProblemDto {
            title: problem.spec.title.clone(),
            statement: problem.spec.statement.clone(),
            signature: problem.spec.signature.clone(),
            examples: problem.cases.iter().filter(|c| c.visibility == Visibility::Example).cloned().collect(),
            constraints: problem.spec.constraints.clone(),
            subgoals: vec![],
            files: vec![],
            worked_solution: None,
            target_complexity: Some(problem.spec.target_complexity.clone()),
            total_misreadings: problem.misreadings.len() as u32,
        };

        CodingSessionDto {
            id: "session-123".into(),
            program_id: "prog-1".into(),
            activity_id: "act-1".into(),
            task: CodingTask::Problem,
            setting: Setting::Coached,
            stage: CodingStage::Understand,
            budget_seconds: None,
            active_ms: 12000,
            opened_at: Some(1000),
            finished_at: None,
            revision: 1,
            problem: public_problem,
            draft_files: vec![LearningLabFile {
                path: "solution.py".into(),
                content: "def two_sum(numbers, target):\n    # TODO\n    return []\n".into(),
            }],
            examples: vec![],
            hints: vec![],
            available_next_rung: Some(CodeHintRung::Nudge),
            turns: vec![],
            runs: vec![],
            stage_timings: vec![],
            debrief: None,
        }
    }

    #[test]
    fn test_numbered_solution_formatting() {
        let code = "def two_sum(numbers, target):\n    # line 2\n    return []\n";
        let numbered = build_numbered_solution(code);
        let expected = "1 | def two_sum(numbers, target):\n2 |     # line 2\n3 |     return []\n";
        assert_eq!(numbered, expected);
    }

    #[test]
    fn test_changed_since_last_seen_diff() {
        let prev = "def two_sum(numbers, target):\n    pass\n";
        let curr = "def two_sum(numbers, target):\n    seen = {}\n    return []\n";
        let diff = compute_changed_since_last_seen(Some(prev), curr);
        assert!(diff.is_some(), "Expected diff when solution changed");
        let diff_text = diff.unwrap();
        assert!(diff_text.contains("seen = {}"), "Diff should mention added lines");

        let identical = compute_changed_since_last_seen(Some(curr), curr);
        assert!(identical.is_none(), "Expected no diff for identical content");
    }

    #[test]
    fn test_workspace_observation_never_leaks_reference_or_misreadings() {
        let problem = sample_problem_with_secrets();
        let session = sample_session(&problem);
        let draft_files = session.draft_files.clone();

        let obs = WorkspaceObservation::build(&session, &problem, &draft_files, None)
            .expect("WorkspaceObservation::build should succeed");

        let json = serde_json::to_string(&obs).expect("WorkspaceObservation should serialize to JSON");

        assert!(
            !json.contains("SECRET_REFERENCE_CODE_HASHMAP_SOLVE_XYZ123"),
            "Observation leaked reference code!"
        );
        assert!(
            !json.contains("SECRET_MISREADING_ID_QUADRATIC_LOOP"),
            "Observation leaked misreading ID!"
        );
        assert!(
            !json.contains("SECRET_MISREADING_DESCRIPTION_FORGOT_INDEX"),
            "Observation leaked misreading description!"
        );
        assert!(
            !json.contains("SECRET_MISREADING_CODE_WRONG_RESULT"),
            "Observation leaked misreading code!"
        );

        assert_eq!(obs.task, CodingTask::Problem);
        assert_eq!(obs.setting, Setting::Coached);
        assert_eq!(obs.stage, CodingStage::Understand);
        assert_eq!(obs.clarifications.len(), 1);
        assert_eq!(obs.target_complexity, "O(n) time, O(n) space");
    }
}
