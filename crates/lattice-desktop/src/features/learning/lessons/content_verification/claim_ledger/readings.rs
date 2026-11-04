//! The ledger's record of one reading and the policy's decision on it
//! (acceptance design §2.9, §4).

use super::super::readings::driver::ReadStep;
use super::super::*;
use super::observe::assessed;
use super::ClaimLedger;
use lattice_core::domain::learning::acceptance::{AcceptancePolicy, Outcome, Step};
use lattice_core::domain::learning::claim_review::{
    Check, CheckResult, ClaimEvent, ClaimId, ClassifiedBy,
};

/// Whether a reading's outcome and the check it observed say the same: a
/// statement that stands passed, and any other failed.
fn agrees(step: &ReadStep, check: &Check) -> bool {
    let stands = matches!(
        step.read.outcome,
        Outcome::Stands | Outcome::StandsElsewhere
    );
    let passed = !matches!(check.result, CheckResult::Failed { .. });
    stands == passed
}

impl ClaimLedger {
    /// Records one reading of `id` and the policy's decision on it: the check
    /// it observed (and the gate's assessment), the `Stepped`, and a
    /// `Classified { by: Evidence }` when the step moved it to another class.
    /// A check that disagrees with the outcome is a fault, and nothing is
    /// recorded: the decision and the observation never drift apart. A claim
    /// the user settled first keeps its state, and takes no class change.
    pub(in crate::features::learning::lessons::content_verification) async fn record_reading(
        &mut self,
        policy: &dyn AcceptancePolicy,
        id: &ClaimId,
        step: &ReadStep,
    ) -> Result<()> {
        let finding = &step.read.finding;
        let (check, trust) = self.check_of_reading(step.reading, finding);
        if !agrees(step, &check) {
            return Err(AppError::InternalError(format!(
                "Claim {}: its {:?} reading came to {:?}, but the check it observed is {:?} ({})",
                id.0, step.reading, step.read.outcome, check.result, check.reason
            )));
        }
        self.append(id, ClaimEvent::Checked(check)).await?;
        if let Some(trust) = trust {
            self.append(id, assessed(trust)).await?;
        }
        let stepped = ClaimEvent::Stepped {
            policy: policy.version().to_owned(),
            step: Step::Read(step.reading),
            outcome: step.read.outcome,
            then: step.then.clone(),
            reason: finding.reason.clone(),
            rewrite_ahead: step.rewrite_ahead,
        };
        if !self.append(id, stepped).await? {
            return Ok(());
        }
        if let Some(class) = step.class {
            let evidence = ClaimEvent::Classified {
                class,
                by: ClassifiedBy::Evidence,
            };
            self.append(id, evidence).await?;
        }
        Ok(())
    }
}
