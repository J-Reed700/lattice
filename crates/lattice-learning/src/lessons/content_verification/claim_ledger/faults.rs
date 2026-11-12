//! The faults the machine met in a statement and contained: each is an event
//! in that claim's history, and moves no state.

use super::super::*;
use super::ClaimLedger;
use lattice_core::domain::learning::claim_review::{ClaimEvent, ClaimId, FaultStage};

/// An invariant breach met while taking one statement through the pipeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatementFault {
    pub claim: ClaimId,
    pub stage: FaultStage,
    pub message: String,
}

/// `result` with an invariant breach given back as its message; any other
/// error is the `Err`.
pub(in crate::lessons::content_verification) fn contain<T>(
    result: Result<T>,
) -> Result<std::result::Result<T, String>> {
    match result {
        Err(AppError::InternalError(message)) => Ok(Err(message)),
        other => other.map(Ok),
    }
}

impl ClaimLedger {
    /// Records `fault` in its claim's history.
    pub async fn record_fault(&mut self, fault: StatementFault) -> Result<()> {
        tracing::error!(
            target: "lattice::fault",
            claim = %fault.claim.0,
            stage = ?fault.stage,
            message = %fault.message,
            "A statement was held unjudged"
        );
        let StatementFault {
            claim,
            stage,
            message,
        } = fault;
        self.append(&claim, ClaimEvent::Faulted { stage, message })
            .await?;
        Ok(())
    }

    /// Every fault recorded, claim by claim, in the order each claim met them.
    pub fn faults(&self) -> Vec<StatementFault> {
        self.entries()
            .iter()
            .flat_map(|entry| {
                entry
                    .events()
                    .iter()
                    .filter_map(|record| match &record.event {
                        ClaimEvent::Faulted { stage, message } => Some(StatementFault {
                            claim: entry.claim_id().clone(),
                            stage: stage.clone(),
                            message: message.clone(),
                        }),
                        _ => None,
                    })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lessons::content_verification::tests::acceptance_switch_tests::{
        claim, open, SOURCED,
    };

    #[tokio::test]
    async fn a_recorded_fault_is_listed_with_its_claim_and_moves_no_state() -> Result<()> {
        let mut ledger = open().await?;
        let text = "Mitochondria make most of a cell's ATP.";
        let id = claim(&mut ledger, "atp", (text, text), text, SOURCED).await?;
        let before = ledger.entry(&id).map(|entry| entry.state().clone());
        let fault = StatementFault {
            claim: id.clone(),
            stage: FaultStage::Take,
            message: "no edge".into(),
        };

        ledger.record_fault(fault.clone()).await?;

        assert_eq!(ledger.faults(), vec![fault]);
        assert_eq!(ledger.entry(&id).map(|entry| entry.state().clone()), before);
        Ok(())
    }
}
