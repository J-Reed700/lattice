//! What the acceptance policy's decisions do to a claim: the steps it has
//! decided, and the state each decision sends it to (acceptance design §2.9).

use super::{ClaimLedgerEntry, LedgerError};
use crate::domain::learning::acceptance::{Taken, Then};
use crate::domain::learning::claim_review::replay::{stepped, Cursor, Undecidable};
use crate::domain::learning::claim_review::ClaimEvent;

impl ClaimLedgerEntry {
    /// Where the acceptance policy last sent the statement; `None` until a
    /// `Stepped` event. A claim that has a decision is read from it.
    pub fn decided(&self) -> Option<&Then> {
        self.cursor().decided.as_ref().map(|(_, then)| then)
    }

    /// The steps the policy has decided, in order, each with where it sent the
    /// statement: what [`crate::domain::learning::acceptance::check_record`]
    /// walks.
    pub fn steps(&self) -> Vec<(Taken, Then)> {
        self.events
            .iter()
            .filter_map(|record| match &record.event {
                ClaimEvent::Stepped {
                    step,
                    outcome,
                    then,
                    ..
                } => Some((
                    Taken {
                        step: *step,
                        outcome: *outcome,
                    },
                    then.clone(),
                )),
                _ => None,
            })
            .collect()
    }

    /// The position after a `Stepped` event: the claim goes where the decision
    /// sends it, unless it waits for the user (it stays theirs), and the
    /// decision is remembered. A settled claim takes none, and a claim is
    /// never continued under another policy than its first decision's.
    pub(super) fn apply_step(
        &self,
        next: &mut Cursor,
        event: &ClaimEvent,
        at: i64,
    ) -> Result<(), LedgerError> {
        let ClaimEvent::Stepped {
            policy,
            outcome,
            then,
            reason,
            rewrite_ahead,
            ..
        } = event
        else {
            return Ok(());
        };
        if next.state.is_settled() {
            return Err(self.refuse(event));
        }
        match &next.decided {
            Some((held, _)) if held != policy => {
                return Err(LedgerError::PolicyChanged {
                    claim: self.claim_id.0.clone(),
                    held: held.clone(),
                    recorded: policy.clone(),
                })
            }
            _ => {}
        }
        if !next.state.awaits_user() {
            let state = stepped(next, *outcome, then, reason, *rewrite_ahead, at)
                .map_err(|fault| self.undecidable(fault))?;
            if let Some(state) = state {
                next.state = state;
            }
        }
        next.decided = Some((policy.clone(), then.clone()));
        Ok(())
    }

    fn undecidable(&self, fault: Undecidable) -> LedgerError {
        match fault {
            Undecidable::NoSupport => LedgerError::ResolvedWithoutSupport {
                claim: self.claim_id.0.clone(),
            },
        }
    }
}
