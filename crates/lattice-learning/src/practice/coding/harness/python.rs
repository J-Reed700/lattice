//! Python case harness: generates checks and parses per-case test results.

use super::CaseHarness;
use crate::lab_runtime::LearningLabFile;
use crate::practice::coding::problem::{Case, CaseResult, ProblemSpec};

#[derive(Debug, Clone, Default)]
pub struct PythonHarness;

impl PythonHarness {
    pub fn new() -> Self {
        Self
    }
}

impl CaseHarness for PythonHarness {
    fn evaluator(&self, _problem: &ProblemSpec, _cases: &[Case], _nonce: &str) -> Vec<LearningLabFile> {
        Vec::new()
    }

    fn results(&self, _stdout: &str, _nonce: &str) -> Vec<CaseResult> {
        Vec::new()
    }
}
