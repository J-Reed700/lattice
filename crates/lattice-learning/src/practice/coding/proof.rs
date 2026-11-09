//! The proof of a drafted problem, run before it is shown. The reference must pass every case; each scale case takes
//! its digest and time from the reference's own run; each misreading must fail a case the reference passes.

#![allow(unused_imports)]

use crate::drills::runner::{CodeRunner, RunOutcome};
use crate::lab_runtime::{LearningLabFile, LearningLabLimits};
use crate::persistence::hash_text;
use crate::practice::coding::harness::{new_nonce, CaseHarness, RunEnd};
use crate::practice::coding::problem::{
    CaseResult, CaseStatus, Misreading, ProblemCase, ProblemSpec, SOLUTION_PATH,
};
use lattice_core::shared::error::{AppError, Result};
use std::collections::BTreeMap;
use tokio_util::sync::CancellationToken;

#[cfg(test)]
pub(crate) mod fakes;
#[cfg(test)]
mod tests;

/// How many misreadings a proven problem must kill. Fewer cannot tell a learner's example from a lucky one, and the
/// examples' coverage line counts them.
pub const MIN_KILLED_MISREADINGS: usize = 2;

/// The share of the runtime's time limit the reference may use on all scale cases together, as a divisor: a correct
/// learner solution that is a few times slower than the reference must still finish inside the limit.
pub const SCALE_TIME_DIVISOR: u64 = 4;

/// The most time, in milliseconds, the reference may spend on the scale cases together.
pub fn scale_budget_ms() -> u64 {
    LearningLabLimits::default()
        .timeout_seconds
        .saturating_mul(1000)
        / SCALE_TIME_DIVISOR
}

/// A problem that passed its proof: the cases with their digests, the misreadings that fail a case, and the time
/// the reference took on each scale case.
#[derive(Debug, Clone, PartialEq)]
pub struct Proven {
    pub cases: Vec<ProblemCase>,
    pub misreadings: Vec<Misreading>,
    pub reference_ms: BTreeMap<String, u64>,
}

/// What a proof found.
#[derive(Debug, Clone, PartialEq)]
pub enum Proof {
    Proven(Proven),
    /// Why the draft cannot stand, written for the model that drafted it.
    Unproven(String),
}

/// Proves a draft. A draft that does not hold is `Proof::Unproven`; a runtime that is not there, or a cancelled
/// build, is an error.
pub async fn prove(
    _runner: &dyn CodeRunner,
    _harness: &dyn CaseHarness,
    _spec: &ProblemSpec,
    _reference: &str,
    _misreadings: &[Misreading],
    _cases: &[ProblemCase],
    _cancel: CancellationToken,
) -> Result<Proof> {
    Ok(Proof::Unproven("The proof is not built yet.".to_owned()))
}
