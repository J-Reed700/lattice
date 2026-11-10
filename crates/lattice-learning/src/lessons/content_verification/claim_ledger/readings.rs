//! The ledger's record of one reading and the policy's decision on it
//! (acceptance design §2.9, §4), and of the outcome of a remedy.

use super::super::readings::driver::ReadStep;
use super::super::*;
use super::observe::assessed;
use super::ClaimLedger;
use lattice_core::domain::learning::acceptance::{
    decide, rewrite_ahead, AcceptancePolicy, Outcome, Reading, Step, Taken, Then,
};
use lattice_core::domain::learning::claim_review::{
    Check, CheckResult, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimState, ClassifiedBy,
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
    /// the user settled first keeps its state, and takes no class change. A
    /// stop belongs to its run: its check is recorded and its decision is not,
    /// so the claim stays at its node with its re-ask spent and a resumed run
    /// asks it once more.
    pub(in crate::lessons::content_verification) async fn record_reading(
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
        if step.then == Then::Stop {
            return Ok(());
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

    /// Records the outcome of a remedy for `id`: the `Stepped` the policy's
    /// decision makes of it, and `Classified { by: Evidence }` when the step
    /// moves the claim to another class. Nothing is recorded, and `false` is
    /// returned, for a claim that does not stand at a node of `step`, for one
    /// the machine does not own (a step is the machine's decision), and for
    /// one the user settled first.
    pub(in crate::lessons::content_verification) async fn record_step(
        &mut self,
        policy: &dyn AcceptancePolicy,
        id: &ClaimId,
        step: Step,
        outcome: Outcome,
    ) -> Result<bool> {
        let Some((at, held)) = self.waits_at(policy, id) else {
            return Ok(false);
        };
        if held != step {
            return Ok(false);
        }
        let Some((then, next)) = decide(policy, at, Taken { step, outcome }) else {
            tracing::warn!(claim = %id.0, ?step, ?outcome, "claim.state: the policy has no edge for this outcome; nothing is recorded");
            return Ok(false);
        };
        let reason = match self.entry(id).map(ClaimLedgerEntry::state) {
            Some(ClaimState::Open { reason, .. } | ClaimState::Queued { reason, .. }) => {
                reason.clone()
            }
            _ => String::new(),
        };
        let stepped = ClaimEvent::Stepped {
            policy: policy.version().to_owned(),
            step,
            outcome,
            then,
            reason,
            rewrite_ahead: next.is_some_and(|next| rewrite_ahead(policy, next)),
        };
        if !self.append(id, stepped).await? {
            return Ok(false);
        }
        let class = next
            .map(|next| next.class)
            .filter(|class| *class != at.class);
        if let Some(class) = class {
            let evidence = ClaimEvent::Classified {
                class,
                by: ClassifiedBy::Evidence,
            };
            self.append(id, evidence).await?;
        }
        Ok(true)
    }

    /// Records the page's reading that answers a user's hint: the check it
    /// observed (and the gate's assessment) and nothing else. The claim is the
    /// user's, so it takes no step; the check is the first judged one after the
    /// hint, which the claim's record applies as its own.
    pub(in crate::lessons::content_verification) async fn record_hint_reading(
        &mut self,
        id: &ClaimId,
        finding: &Finding,
    ) -> Result<()> {
        let (check, trust) = self.check_of_reading(Reading::Page, finding);
        self.append(id, ClaimEvent::Checked(check)).await?;
        if let Some(trust) = trust {
            self.append(id, assessed(trust)).await?;
        }
        Ok(())
    }
}
