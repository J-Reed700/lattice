//! The proof of a drafted problem, run before it is shown. The reference must pass every case; each scale case takes
//! its digest and time from the reference's own run; each misreading must fail a case the reference passes.

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
    runner: &dyn CodeRunner,
    harness: &dyn CaseHarness,
    spec: &ProblemSpec,
    reference: &str,
    misreadings: &[Misreading],
    cases: &[ProblemCase],
    cancel: CancellationToken,
) -> Result<Proof> {
    let proving = Proving {
        runner,
        harness,
        spec,
        cancel,
    };

    let first = proving.run(reference, cases).await?;
    if let Some(reason) = reference_fault(cases, &first) {
        return Ok(Proof::Unproven(reason));
    }
    let mut cases = cases.to_vec();
    for case in cases.iter_mut().filter(|case| case.scale) {
        let actual = first
            .iter()
            .find(|result| result.case_id == case.id)
            .and_then(|result| result.actual.as_deref());
        let Some(actual) = actual else {
            return Ok(Proof::Unproven(format!(
                "The reference returned nothing on the scale case {}.",
                case.id
            )));
        };
        case.digest = Some(hash_text(actual));
    }

    let second = proving.run(reference, &cases).await?;
    if let Some(reason) = reference_fault(&cases, &second) {
        return Ok(Proof::Unproven(reason));
    }
    let mut reference_ms = BTreeMap::new();
    for case in cases.iter().filter(|case| case.scale) {
        let elapsed = second
            .iter()
            .find(|result| result.case_id == case.id)
            .and_then(|result| result.elapsed_ms);
        let Some(elapsed) = elapsed else {
            return Ok(Proof::Unproven(format!(
                "The run reported no time for the scale case {}.",
                case.id
            )));
        };
        reference_ms.insert(case.id.clone(), elapsed);
    }
    let total: u64 = reference_ms.values().sum();
    if total > scale_budget_ms() {
        return Ok(Proof::Unproven(format!(
            "The reference took {total} ms on the scale cases together, more than the {} ms they may take. Use smaller scale inputs, or a reference that does less work.",
            scale_budget_ms()
        )));
    }

    let mut kept = Vec::new();
    let mut notes = Vec::new();
    for misreading in misreadings {
        let results = proving.run(&misreading.code, &cases).await?;
        let killed_by: Vec<String> = results
            .iter()
            .filter(|result| {
                matches!(
                    result.status,
                    CaseStatus::Failed | CaseStatus::Error | CaseStatus::TimedOut
                )
            })
            .map(|result| result.case_id.clone())
            .collect();
        let never_runs = !results.is_empty()
            && results
                .iter()
                .all(|result| result.status == CaseStatus::Error);
        if killed_by.is_empty() {
            notes.push(format!(
                "Misreading {} passes every case, so nothing tells it from the reference.",
                misreading.id
            ));
        } else if never_runs {
            notes.push(format!(
                "Misreading {} raises on every case, so it is not a plausible reading.",
                misreading.id
            ));
        } else {
            kept.push(Misreading {
                killed_by,
                ..misreading.clone()
            });
        }
    }
    if kept.len() < MIN_KILLED_MISREADINGS {
        return Ok(Proof::Unproven(format!(
            "Only {} of the {} misreadings fail a case and run; {MIN_KILLED_MISREADINGS} are needed. {} Write misreadings that are plausible wrong readings of the statement, and add cases that tell each one from the reference.",
            kept.len(),
            misreadings.len(),
            notes.join(" ")
        )));
    }
    Ok(Proof::Proven(Proven {
        cases,
        misreadings: kept,
        reference_ms,
    }))
}

/// What a proof runs code with.
struct Proving<'a> {
    runner: &'a dyn CodeRunner,
    harness: &'a dyn CaseHarness,
    spec: &'a ProblemSpec,
    cancel: CancellationToken,
}

impl Proving<'_> {
    /// Runs `code` as `solution.py` against `cases` and reads one result per case.
    async fn run(&self, code: &str, cases: &[ProblemCase]) -> Result<Vec<CaseResult>> {
        if self.cancel.is_cancelled() {
            return Err(cancelled());
        }
        let nonce = new_nonce();
        let mut files = self.harness.evaluator(self.spec, cases, &nonce)?;
        let read_only: Vec<String> = files.iter().map(|file| file.path.clone()).collect();
        files.push(LearningLabFile {
            path: SOLUTION_PATH.to_owned(),
            content: code.to_owned(),
        });
        let (outcome, stdout) = self
            .runner
            .run_capturing(&files, &read_only, self.cancel.clone())
            .await?;
        let end = match outcome {
            RunOutcome::Passed | RunOutcome::Failed { .. } => RunEnd::Finished,
            RunOutcome::TimedOut { .. } => RunEnd::TimedOut,
            RunOutcome::Cancelled => return Err(cancelled()),
            RunOutcome::Unavailable { why } => {
                return Err(AppError::ServiceNotAvailable(format!(
                    "The problem could not be proven: {why}"
                )))
            }
        };
        Ok(self.harness.results(cases, &stdout, &nonce, end))
    }
}

fn cancelled() -> AppError {
    AppError::InvalidState("The coding build was cancelled.".into())
}

/// What is wrong with the reference's results, if anything: every case must have passed, and a scale case without
/// a digest only ran.
fn reference_fault(cases: &[ProblemCase], results: &[CaseResult]) -> Option<String> {
    let faults: Vec<String> = cases
        .iter()
        .filter_map(|case| {
            let wanted = if case.scale && case.digest.is_none() {
                CaseStatus::Ran
            } else {
                CaseStatus::Passed
            };
            let Some(result) = results.iter().find(|result| result.case_id == case.id) else {
                return Some(format!("- {} ({}): no result", case.id, case.category));
            };
            if result.status == wanted {
                return None;
            }
            let what = match result.status {
                CaseStatus::Failed => format!(
                    "the reference returned {} where {} was expected",
                    result.actual.as_deref().unwrap_or("nothing"),
                    case.expected.as_deref().unwrap_or("the stored digest")
                ),
                CaseStatus::Error => format!(
                    "the reference raised {}",
                    result.error.as_deref().unwrap_or("an error")
                ),
                CaseStatus::TimedOut => {
                    "the reference did not finish within the time limit".to_owned()
                }
                CaseStatus::NotRun => "the run stopped before this case".to_owned(),
                CaseStatus::Passed | CaseStatus::Ran => {
                    format!("the case ended {:?}, not {wanted:?}", result.status)
                }
            };
            Some(format!("- {} ({}): {what}", case.id, case.category))
        })
        .collect();
    if faults.is_empty() {
        None
    } else {
        Some(format!(
            "The reference solution did not pass every case:\n{}\nFix the reference, or the cases that disagree with the statement.",
            faults.join("\n")
        ))
    }
}
