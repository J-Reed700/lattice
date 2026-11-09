//! Proves coding problems before they are shown:
//! 1. The reference solution must pass every case, recording timings on scale cases.
//! 2. Misreadings are run against every case: any misreading passing all cases is dropped,
//!    failing misreadings record their `killed_by` case IDs, and at least 2 killed misreadings are required.

use std::collections::BTreeMap;
use tokio_util::sync::CancellationToken;

use crate::drills::runner::{CodeRunner, RunOutcome};
use crate::practice::coding::harness::{run_cases, CaseHarness};
use crate::practice::coding::problem::{Case, CaseResult, CaseStatus, Misreading, ProblemSpec};

/// The minimum number of killed misreadings required for a problem to be proven.
pub const MIN_KILLED_MISREADINGS: usize = 2;

/// Errors that can occur during problem proof verification.
#[derive(Debug, Clone, PartialEq)]
pub enum ProofError {
    /// The reference solution did not pass all cases.
    ReferenceFailed {
        failed_cases: Vec<String>,
        results: Vec<CaseResult>,
    },
    /// Fewer than `MIN_KILLED_MISREADINGS` were killed by the test cases.
    InsufficientMisreadings { killed: usize, required: usize },
    /// The proof operation was cancelled.
    Cancelled,
}

/// The verified outputs of a problem proof.
#[derive(Debug, Clone, PartialEq)]
pub struct ProvedProblem {
    pub proven_misreadings: Vec<Misreading>,
    pub reference_ms: BTreeMap<String, u64>,
}

/// Proves the reference solution passes all cases and records timings for scale cases.
pub async fn prove_reference(
    runner: &dyn CodeRunner,
    harness: &dyn CaseHarness,
    spec: &ProblemSpec,
    reference: &str,
    cases: &[Case],
    cancel: CancellationToken,
) -> Result<BTreeMap<String, u64>, ProofError> {
    if cancel.is_cancelled() {
        return Err(ProofError::Cancelled);
    }

    let (outcome, results) = run_cases(runner, harness, spec, reference, cases, cancel)
        .await
        .map_err(|_| ProofError::Cancelled)?;

    let mut failed_cases = Vec::new();
    let mut reference_ms = BTreeMap::new();

    for result in &results {
        if result.status != CaseStatus::Passed {
            failed_cases.push(result.case_id.clone());
        }
    }

    if !failed_cases.is_empty() || outcome != RunOutcome::Passed {
        return Err(ProofError::ReferenceFailed {
            failed_cases,
            results,
        });
    }

    for case in cases {
        if case.scale {
            let elapsed = results
                .iter()
                .find(|r| r.case_id == case.id)
                .and_then(|r| r.elapsed_ms)
                .unwrap_or(0);
            reference_ms.insert(case.id.clone(), elapsed);
        }
    }

    Ok(reference_ms)
}

/// Proves misreadings against all test cases:
/// - Drops misreadings that pass every case.
/// - Records `killed_by` case IDs for misreadings that fail at least one case.
/// - Requires at least `MIN_KILLED_MISREADINGS` (2) killed misreadings.
pub async fn prove_misreadings(
    runner: &dyn CodeRunner,
    harness: &dyn CaseHarness,
    spec: &ProblemSpec,
    candidate_misreadings: &[Misreading],
    cases: &[Case],
    cancel: CancellationToken,
) -> Result<Vec<Misreading>, ProofError> {
    if cancel.is_cancelled() {
        return Err(ProofError::Cancelled);
    }

    let mut proven_misreadings = Vec::new();

    for candidate in candidate_misreadings {
        if cancel.is_cancelled() {
            return Err(ProofError::Cancelled);
        }

        let (outcome, results) = run_cases(
            runner,
            harness,
            spec,
            &candidate.code,
            cases,
            cancel.clone(),
        )
        .await
        .map_err(|_| ProofError::Cancelled)?;

        let mut killed_by = Vec::new();
        for result in &results {
            if result.status != CaseStatus::Passed {
                killed_by.push(result.case_id.clone());
            }
        }

        if killed_by.is_empty() && outcome != RunOutcome::Passed {
            if let Some(first_case) = cases.first() {
                killed_by.push(first_case.id.clone());
            }
        }

        if !killed_by.is_empty() {
            let mut misreading = candidate.clone();
            misreading.killed_by = killed_by;
            proven_misreadings.push(misreading);
        }
    }

    if proven_misreadings.len() < MIN_KILLED_MISREADINGS {
        return Err(ProofError::InsufficientMisreadings {
            killed: proven_misreadings.len(),
            required: MIN_KILLED_MISREADINGS,
        });
    }

    Ok(proven_misreadings)
}

/// Proves a candidate problem by proving its reference and its misreadings.
pub async fn prove_problem(
    runner: &dyn CodeRunner,
    harness: &dyn CaseHarness,
    spec: &ProblemSpec,
    reference: &str,
    candidate_misreadings: &[Misreading],
    cases: &[Case],
    cancel: CancellationToken,
) -> Result<ProvedProblem, ProofError> {
    let reference_ms =
        prove_reference(runner, harness, spec, reference, cases, cancel.clone()).await?;
    let proven_misreadings =
        prove_misreadings(runner, harness, spec, candidate_misreadings, cases, cancel).await?;
    Ok(ProvedProblem {
        proven_misreadings,
        reference_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drills::runner::RunOutcome;
    use crate::lab_runtime::LearningLabFile;
    use crate::practice::coding::problem::{
        Case, CaseResult, Compare, Param, Signature, Visibility,
    };
    use async_trait::async_trait;
    use lattice_core::domain::learning::drills::DrillFile;
    use lattice_core::shared::error::Result;

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
        async fn run_capturing(
            &self,
            _files: &[LearningLabFile],
            _read_only_paths: &[String],
            _cancel: CancellationToken,
        ) -> Result<(RunOutcome, String)> {
            Ok((RunOutcome::Passed, String::new()))
        }
    }

    struct MockHarness;

    impl CaseHarness for MockHarness {
        fn evaluator(
            &self,
            _problem: &ProblemSpec,
            _cases: &[Case],
            _nonce: &str,
        ) -> Vec<LearningLabFile> {
            Vec::new()
        }
        fn results(&self, stdout: &str, _nonce: &str) -> Vec<CaseResult> {
            if stdout.contains("return []") {
                vec![
                    CaseResult {
                        case_id: "case-1".into(),
                        status: CaseStatus::Failed,
                        actual: Some("[]".into()),
                        printed: String::new(),
                        error: None,
                        elapsed_ms: Some(2),
                    },
                    CaseResult {
                        case_id: "case-2".into(),
                        status: CaseStatus::Passed,
                        actual: Some("[0, 1]".into()),
                        printed: String::new(),
                        error: None,
                        elapsed_ms: Some(2),
                    },
                ]
            } else if stdout.contains("return [1, 2]") {
                vec![
                    CaseResult {
                        case_id: "case-1".into(),
                        status: CaseStatus::Failed,
                        actual: Some("[1, 2]".into()),
                        printed: String::new(),
                        error: None,
                        elapsed_ms: Some(2),
                    },
                    CaseResult {
                        case_id: "case-2".into(),
                        status: CaseStatus::Failed,
                        actual: Some("[1, 2]".into()),
                        printed: String::new(),
                        error: None,
                        elapsed_ms: Some(2),
                    },
                ]
            } else {
                vec![
                    CaseResult {
                        case_id: "case-1".into(),
                        status: CaseStatus::Passed,
                        actual: Some("[0, 1]".into()),
                        printed: String::new(),
                        error: None,
                        elapsed_ms: Some(2),
                    },
                    CaseResult {
                        case_id: "case-2".into(),
                        status: CaseStatus::Passed,
                        actual: Some("[0, 1]".into()),
                        printed: String::new(),
                        error: None,
                        elapsed_ms: Some(2),
                    },
                    CaseResult {
                        case_id: "case-scale".into(),
                        status: CaseStatus::Passed,
                        actual: Some("[9998, 9999]".into()),
                        printed: String::new(),
                        error: None,
                        elapsed_ms: Some(42),
                    },
                ]
            }
        }
    }

    fn sample_spec() -> ProblemSpec {
        ProblemSpec {
            title: "Two Sum".into(),
            statement: "Find indices of two numbers that add up to target.".into(),
            signature: Signature {
                function: "two_sum".into(),
                params: vec![
                    Param {
                        name: "nums".into(),
                        annotation: "list[int]".into(),
                    },
                    Param {
                        name: "target".into(),
                        annotation: "int".into(),
                    },
                ],
                returns: "list[int]".into(),
            },
            constraints: vec!["2 <= nums.length <= 10^4".into()],
            clarifications: Vec::new(),
            language: "python".into(),
            difficulty: crate::practice::coding::problem::Difficulty::Easy,
            target_complexity: "O(n) time, O(n) space".into(),
            patterns: vec!["hash_map_lookup".into()],
            starter: "def two_sum(nums: list[int], target: int) -> list[int]:\n    pass\n".into(),
        }
    }

    fn sample_cases() -> Vec<Case> {
        vec![
            Case {
                id: "case-1".into(),
                category: "basic".into(),
                args: vec!["[2, 7, 11, 15]".into(), "9".into()],
                expected: Some("[0, 1]".into()),
                compare: Compare::Exact,
                visibility: Visibility::Example,
                scale: false,
            },
            Case {
                id: "case-2".into(),
                category: "duplicates".into(),
                args: vec!["[3, 3]".into(), "6".into()],
                expected: Some("[0, 1]".into()),
                compare: Compare::Exact,
                visibility: Visibility::Hidden,
                scale: false,
            },
            Case {
                id: "case-scale".into(),
                category: "large scale".into(),
                args: vec!["list(range(10000))".into(), "19997".into()],
                expected: Some("[9998, 9999]".into()),
                compare: Compare::Exact,
                visibility: Visibility::Hidden,
                scale: true,
            },
        ]
    }

    #[tokio::test]
    async fn test_reference_solution_failing_case_is_rejected() {
        let runner = MockRunner;
        let harness = MockHarness;
        let spec = sample_spec();
        let cases = sample_cases();
        let buggy_reference = "def two_sum(nums, target): return []";

        let result = prove_reference(
            &runner,
            &harness,
            &spec,
            buggy_reference,
            &cases,
            CancellationToken::new(),
        )
        .await;

        assert!(
            matches!(result, Err(ProofError::ReferenceFailed { .. })),
            "reference that fails a case must be rejected"
        );
    }

    #[tokio::test]
    async fn test_misreading_passes_every_case_is_dropped_and_failing_records_killed_by() {
        let runner = MockRunner;
        let harness = MockHarness;
        let spec = sample_spec();
        let cases = sample_cases();

        let candidate_misreadings = vec![
            Misreading {
                id: "mis-passing".into(),
                description: "Equivalent logic that passes all test cases".into(),
                code: "def two_sum(nums, target): return [0, 1]".into(),
                killed_by: Vec::new(),
            },
            Misreading {
                id: "mis-fails-1".into(),
                description: "Returns empty list, failing case-1 and case-2".into(),
                code: "def two_sum(nums, target): return []".into(),
                killed_by: Vec::new(),
            },
            Misreading {
                id: "mis-fails-2".into(),
                description: "Off-by-one indices, failing cases".into(),
                code: "def two_sum(nums, target): return [1, 2]".into(),
                killed_by: Vec::new(),
            },
        ];

        let proven = prove_misreadings(
            &runner,
            &harness,
            &spec,
            &candidate_misreadings,
            &cases,
            CancellationToken::new(),
        )
        .await
        .expect("prove_misreadings should succeed with at least 2 killed misreadings");

        assert_eq!(proven.len(), 2, "passing misreadings must be dropped");
        assert!(
            !proven.iter().any(|m| m.id == "mis-passing"),
            "mis-passing must not be in proven list"
        );

        let m1 = proven
            .iter()
            .find(|m| m.id == "mis-fails-1")
            .expect("mis-fails-1 must be present");
        assert!(
            !m1.killed_by.is_empty(),
            "killed_by must not be empty for failing misreading"
        );
        assert!(m1.killed_by.contains(&"case-1".to_string()));

        let m2 = proven
            .iter()
            .find(|m| m.id == "mis-fails-2")
            .expect("mis-fails-2 must be present");
        assert!(
            !m2.killed_by.is_empty(),
            "killed_by must not be empty for failing misreading"
        );
    }
}
