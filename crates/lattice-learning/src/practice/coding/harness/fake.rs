//! A runner that answers case runs without a sandbox, and a small arithmetic problem to run on it.

use crate::drills::runner::{CodeRunner, RunOutcome};
use crate::lab_runtime::LearningLabFile;
use crate::practice::coding::problem::{
    CaseCompare, CaseStatus, CaseVisibility, Misreading, Problem, ProblemCase, ProblemDifficulty,
    ProblemSignature, ProblemSpec, SignatureParam,
};
use async_trait::async_trait;
use lattice_core::domain::learning::drills::DrillFile;
use lattice_core::shared::error::Result;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use tokio_util::sync::CancellationToken;

/// An example argument the fake's literal check refuses.
pub(crate) const NOT_A_LITERAL: &str = "not a literal";
/// The arguments' first number, which makes every code raise.
pub(crate) const UNLUCKY: i64 = 13;
/// A snippet the fake refuses to parse.
pub(crate) const BROKEN_SNIPPET: &str = "def broken(";

pub(crate) const REFERENCE: &str = "reference";
pub(crate) const SUBTRACTS: &str = "subtracts";
pub(crate) const FIRST_ONLY: &str = "first only";
pub(crate) const ALWAYS_ZERO: &str = "always zero";

/// Runs `add(a, b)` for the code names above, deciding each case from its arguments and expected value.
pub(crate) struct ScriptedRunner {
    pub files_seen: std::sync::Mutex<Vec<Vec<LearningLabFile>>>,
}

impl ScriptedRunner {
    pub(crate) fn new() -> Self {
        Self {
            files_seen: std::sync::Mutex::new(Vec::new()),
        }
    }
}

fn answer(code: &str, case: &Value) -> (CaseStatus, Option<String>, Option<String>) {
    let numbers: Vec<i64> = case["args"]
        .as_array()
        .map(|args| {
            args.iter()
                .filter_map(|arg| arg.as_str()?.parse::<i64>().ok())
                .collect()
        })
        .unwrap_or_default();
    let (a, b) = (
        numbers.first().copied().unwrap_or(0),
        numbers.get(1).copied().unwrap_or(0),
    );
    if a == UNLUCKY {
        return (
            CaseStatus::Error,
            None,
            Some("ValueError: unlucky".to_owned()),
        );
    }
    let result = match code {
        REFERENCE => a + b,
        SUBTRACTS => a - b,
        FIRST_ONLY => a,
        ALWAYS_ZERO => 0,
        _ => i64::MIN,
    };
    let status = match case["expected"].as_str() {
        None => CaseStatus::Ran,
        Some(expected) if expected.parse::<i64>().ok() == Some(result) => CaseStatus::Passed,
        Some(_) => CaseStatus::Failed,
    };
    (status, Some(result.to_string()), None)
}

#[async_trait]
impl CodeRunner for ScriptedRunner {
    fn name(&self) -> &str {
        "scripted"
    }

    fn entrypoint(&self) -> &str {
        "checks.py"
    }

    fn contract(&self) -> &str {
        "scripted"
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
        files: &[LearningLabFile],
        _read_only_paths: &[String],
        _cancel: CancellationToken,
    ) -> Result<(RunOutcome, String)> {
        if let Ok(mut seen) = self.files_seen.lock() {
            seen.push(files.to_vec());
        }
        let content_of = |path: &str| {
            files
                .iter()
                .find(|file| file.path == path)
                .map(|file| file.content.clone())
        };
        if let Some(literals) = content_of("literals.json") {
            let sources: Vec<String> = serde_json::from_str(&literals).unwrap_or_default();
            let problems: Vec<Option<String>> = sources
                .iter()
                .map(|source| {
                    (source == NOT_A_LITERAL).then(|| "SyntaxError: invalid syntax".to_owned())
                })
                .collect();
            return Ok((
                RunOutcome::Passed,
                serde_json::to_string(&problems).unwrap_or_default(),
            ));
        }
        if let Some(snippet) = content_of("snippet.txt") {
            let outcome = if snippet.contains(BROKEN_SNIPPET) {
                RunOutcome::Failed {
                    output: String::new(),
                }
            } else {
                RunOutcome::Passed
            };
            return Ok((outcome, String::new()));
        }
        let spec: Value =
            serde_json::from_str(&content_of("cases.json").unwrap_or_default()).unwrap_or_default();
        let nonce = spec["nonce"].as_str().unwrap_or_default().to_owned();
        let code = content_of("solution.py").unwrap_or_default();
        let mut stdout = String::new();
        let mut failing = false;
        for case in spec["cases"].as_array().cloned().unwrap_or_default() {
            let (status, actual, error) = answer(&code, &case);
            failing |= matches!(status, CaseStatus::Failed | CaseStatus::Error);
            stdout.push_str(&format!(
                "{nonce}{}\n",
                json!({
                    "caseId": case["id"],
                    "status": status,
                    "actual": actual,
                    "printed": "",
                    "error": error,
                    "elapsedMs": 1,
                })
            ));
        }
        let outcome = if failing {
            RunOutcome::Failed {
                output: stdout.clone(),
            }
        } else {
            RunOutcome::Passed
        };
        Ok((outcome, stdout))
    }
}

pub(crate) fn case(id: &str, args: [&str; 2], expected: Option<&str>, hidden: bool) -> ProblemCase {
    ProblemCase {
        id: id.to_owned(),
        category: format!("category of {id}"),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        expected: expected.map(str::to_owned),
        compare: CaseCompare::Exact,
        visibility: if hidden {
            CaseVisibility::Hidden
        } else {
            CaseVisibility::Example
        },
        scale: false,
        digest: None,
    }
}

pub(crate) fn sample_spec() -> ProblemSpec {
    ProblemSpec {
        title: "Add two numbers".into(),
        statement: "Return the sum of a and b.".into(),
        signature: ProblemSignature {
            function: "add".into(),
            params: vec![
                SignatureParam {
                    name: "a".into(),
                    annotation: "int".into(),
                },
                SignatureParam {
                    name: "b".into(),
                    annotation: "int".into(),
                },
            ],
            returns: "int".into(),
        },
        constraints: vec![],
        clarifications: vec![],
        language: "python".into(),
        difficulty: ProblemDifficulty::Easy,
        target_complexity: "O(1)".into(),
        patterns: vec![],
        starter: "def add(a: int, b: int) -> int:\n    pass\n".into(),
    }
}

fn misreading(id: &str, code: &str) -> Misreading {
    Misreading {
        id: id.to_owned(),
        description: format!("It {code}."),
        code: code.to_owned(),
        killed_by: vec![],
    }
}

/// `add(a, b)`: three misreadings, two shown examples and a hidden case.
pub(crate) fn sample_problem() -> Problem {
    Problem {
        spec: sample_spec(),
        reference: REFERENCE.to_owned(),
        misreadings: vec![
            misreading("subtracts", SUBTRACTS),
            misreading("first", FIRST_ONLY),
            misreading("zero", ALWAYS_ZERO),
        ],
        cases: vec![
            case("example-1", ["2", "3"], Some("5"), false),
            case("example-2", ["0", "0"], Some("0"), false),
            case("hidden-1", ["5", "-5"], Some("0"), true),
        ],
        reference_ms: BTreeMap::new(),
        bug: None,
    }
}
