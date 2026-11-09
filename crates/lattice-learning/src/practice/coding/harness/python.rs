//! The Python harness.

use super::{CaseHarness, RunEnd};
use crate::lab_runtime::LearningLabFile;
use crate::practice::coding::problem::{CaseResult, CaseStatus, ProblemCase, ProblemSpec};
use lattice_core::shared::error::{AppError, Result};

#[derive(Debug, Clone, Default)]
pub struct PythonHarness;

impl PythonHarness {
    pub fn new() -> Self {
        Self
    }
}

/// The helper a project's `checks.py` imports as `lattice_check`, bound to this run's nonce.
pub fn lattice_check_file(_nonce: &str) -> LearningLabFile {
    LearningLabFile {
        path: "lattice_check.py".to_owned(),
        content: String::new(),
    }
}

impl CaseHarness for PythonHarness {
    fn evaluator(
        &self,
        _spec: &ProblemSpec,
        _cases: &[ProblemCase],
        _nonce: &str,
    ) -> Result<Vec<LearningLabFile>> {
        Err(AppError::InvalidState("not built yet".into()))
    }

    fn results(
        &self,
        cases: &[ProblemCase],
        _stdout: &str,
        _nonce: &str,
        _end: RunEnd,
    ) -> Vec<CaseResult> {
        cases
            .iter()
            .map(|case| CaseResult {
                case_id: case.id.clone(),
                status: CaseStatus::NotRun,
                actual: None,
                printed: String::new(),
                error: None,
                elapsed_ms: None,
            })
            .collect()
    }

    fn program_output(&self, stdout: &str, _nonce: &str) -> String {
        stdout.to_owned()
    }
}
