//! What the ledger tells a repair request of the repairs it has made
//! (acceptance design §2.4, §2.6): the sentences earlier corrections replaced
//! or removed because a check failed them, and why the last correction of a
//! statement could not be applied.

use super::{is_example, ClaimLedger};
use lattice_core::domain::learning::acceptance::{AcceptancePolicy, Goal, Step};
use lattice_core::domain::learning::claim_review::{
    Actor, CheckResult, ClaimEvent, ClaimId, RepairOutcome,
};
use serde_json::{json, Value};

impl ClaimLedger {
    /// The statements that failed a reading and were rewritten or cut, which a
    /// correction must not bring back: for each, its section, the sentence the
    /// repair replaced or removed and the reason of the check that failed it.
    /// Read from the ledger, so a job that resumes has them too. A failed
    /// example is none of them, and neither is a statement that passed its
    /// check and went with a sentence another's repair changed.
    pub(in crate::features::learning::lessons::content_verification) fn repair_history(
        &self,
    ) -> Vec<Value> {
        let mut history: Vec<Value> = Vec::new();
        for entry in self.entries.iter().filter(|entry| !is_example(entry)) {
            let (mut reason, mut failed) = ("", false);
            for record in entry.events() {
                match &record.event {
                    ClaimEvent::Checked(check) => {
                        reason = check.reason.as_str();
                        failed = matches!(check.result, CheckResult::Failed { .. });
                    }
                    ClaimEvent::RepairAttempted {
                        before,
                        outcome: RepairOutcome::Rewritten | RepairOutcome::Removed,
                        ..
                    } if failed && record.provenance.actor == Actor::Machine => {
                        let replaced = json!({
                            "unit": entry.location().unit_index,
                            "sentence": before,
                            "reason": reason,
                        });
                        if !history.contains(&replaced) {
                            history.push(replaced);
                        }
                    }
                    _ => {}
                }
            }
        }
        history
    }

    /// What a rewrite of the statement `id` is asked: the goal of the node it
    /// stands at and, at the retry of a rewrite, why the first correction could
    /// not be applied. None at any node that is no rewrite: the cut is made in
    /// code and is asked of no one.
    pub(in crate::features::learning::lessons::content_verification) fn rewrite_ask(
        &self,
        policy: &dyn AcceptancePolicy,
        id: &ClaimId,
    ) -> Option<(Goal, Option<String>)> {
        let (at, step) = self.waits_at(policy, id)?;
        let Step::Rewrite(goal) = step else {
            return None;
        };
        let retry = policy
            .graph(at.class)
            .node(at.node)
            .is_some_and(|node| node.name.ends_with(".again"));
        let why = if retry { self.not_applied(id) } else { None };
        Some((goal, why))
    }

    /// The reason of the latest correction of `id` that could not be applied.
    fn not_applied(&self, id: &ClaimId) -> Option<String> {
        self.entry(id)?
            .events()
            .iter()
            .rev()
            .find_map(|record| match &record.event {
                ClaimEvent::RepairAttempted {
                    outcome: RepairOutcome::Rejected { reason },
                    ..
                } => Some(reason.clone()),
                _ => None,
            })
    }
}
