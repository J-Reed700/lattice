//! The Python harness: `checks.py` runs each case of `cases.json` against `solution.py` and prints one line per
//! case, `<nonce>{json}`, as soon as the case ends.

use super::{CaseHarness, RunEnd};
use crate::embedded_runtime::LearningBuiltinRuntime;
use crate::lab_runtime::LearningLabFile;
use crate::practice::coding::problem::{CaseResult, CaseStatus, ProblemCase, ProblemSpec};
use lattice_core::shared::error::{AppError, Result};
use serde::Deserialize;
use std::collections::HashMap;

const CHECKS_PY: &str = include_str!("checks.py");
const LATTICE_CHECK_PY: &str = include_str!("lattice_check.py");
const NONCE_MARK: &str = "__NONCE__";
const CASES_PATH: &str = "cases.json";
const TIMED_OUT: &str = "The run exceeded the runtime's time limit and was stopped.";

#[derive(Debug, Clone, Default)]
pub struct PythonHarness;

impl PythonHarness {
    pub fn new() -> Self {
        Self
    }
}

/// The helper a project's `checks.py` imports as `lattice_check`, bound to this run's nonce.
pub fn lattice_check_file(nonce: &str) -> LearningLabFile {
    LearningLabFile {
        path: "lattice_check.py".to_owned(),
        content: LATTICE_CHECK_PY.replace(NONCE_MARK, nonce),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HarnessLine {
    case_id: String,
    status: CaseStatus,
    #[serde(default)]
    actual: Option<String>,
    #[serde(default)]
    printed: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    elapsed_ms: Option<u64>,
}

fn harness_lines<'a>(stdout: &'a str, nonce: &'a str) -> impl Iterator<Item = &'a str> {
    stdout.lines().filter_map(move |line| {
        if nonce.is_empty() {
            None
        } else {
            line.strip_prefix(nonce)
        }
    })
}

impl CaseHarness for PythonHarness {
    fn evaluator(
        &self,
        spec: &ProblemSpec,
        cases: &[ProblemCase],
        nonce: &str,
    ) -> Result<Vec<LearningLabFile>> {
        let cases_json = serde_json::to_string(&serde_json::json!({
            "function": spec.signature.function,
            "nonce": nonce,
            "cases": cases,
        }))
        .map_err(|error| AppError::Serialization(error.to_string()))?;
        Ok(vec![
            LearningLabFile {
                path: LearningBuiltinRuntime::Python.entrypoint().to_owned(),
                content: CHECKS_PY.to_owned(),
            },
            LearningLabFile {
                path: CASES_PATH.to_owned(),
                content: cases_json,
            },
        ])
    }

    fn results(
        &self,
        cases: &[ProblemCase],
        stdout: &str,
        nonce: &str,
        end: RunEnd,
    ) -> Vec<CaseResult> {
        let mut lines: HashMap<String, HarnessLine> = HashMap::new();
        for rest in harness_lines(stdout, nonce) {
            if let Ok(line) = serde_json::from_str::<HarnessLine>(rest) {
                lines.entry(line.case_id.clone()).or_insert(line);
            }
        }
        let mut timed_out_seen = false;
        cases
            .iter()
            .map(|case| match lines.remove(&case.id) {
                Some(line) => CaseResult {
                    case_id: case.id.clone(),
                    status: line.status,
                    actual: line.actual,
                    printed: line.printed.unwrap_or_default(),
                    error: line.error,
                    elapsed_ms: line.elapsed_ms,
                },
                None if end == RunEnd::TimedOut && !timed_out_seen => {
                    timed_out_seen = true;
                    CaseResult {
                        case_id: case.id.clone(),
                        status: CaseStatus::TimedOut,
                        actual: None,
                        printed: String::new(),
                        error: Some(TIMED_OUT.to_owned()),
                        elapsed_ms: None,
                    }
                }
                None => CaseResult {
                    case_id: case.id.clone(),
                    status: CaseStatus::NotRun,
                    actual: None,
                    printed: String::new(),
                    error: None,
                    elapsed_ms: None,
                },
            })
            .collect()
    }

    fn program_output(&self, stdout: &str, nonce: &str) -> String {
        stdout
            .split_inclusive('\n')
            .filter(|line| nonce.is_empty() || !line.starts_with(nonce))
            .collect()
    }
}
