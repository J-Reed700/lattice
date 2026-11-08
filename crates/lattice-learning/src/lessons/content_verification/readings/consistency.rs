//! `Consistency`: the consistency judge on the section's passages, as a
//! Reasoning statement meets it today. Every receipt of its batch is restored
//! before its first call.
use super::super::class_checks::{
    consistency_key, consistency_of, finding_of, fitted, saved_consistency, unjudged_finding,
};
use super::*;
use lattice_core::application::services::claim_verification::{
    ConsistencyOutcome, ConsistencyVerdict,
};

pub(super) struct Consistency;

#[async_trait]
impl Reader for Consistency {
    #[cfg(test)]
    fn reading(&self) -> Reading {
        Reading::Consistency
    }

    async fn read(&self, at: &ReadContext<'_>, statements: &[Statement<'_>]) -> Result<Vec<Read>> {
        let mut made = Made::new(statements.len());
        let mut asks = Vec::new();
        for (ordinal, statement) in statements.iter().enumerate() {
            let Statement { unit, claim, .. } = *statement;
            let shown = fitted(at.llm, &claim.statement, at.citations.of(unit));
            let key = consistency_key(at.llm, &claim.statement, &shown);
            match saved_consistency(&key).await? {
                Some(saved) => {
                    made.put(at, ordinal, read_of(statement, &shown, Some((saved, true))));
                }
                None => asks.push((ordinal, *statement, shown, key)),
            }
        }
        for (ordinal, statement, shown, key) in asks {
            let judged = consistency_of(at.llm, &key, &statement.claim.statement, &shown).await?;
            let read = read_of(&statement, &shown, judged.map(|outcome| (outcome, false)));
            made.put(at, ordinal, read);
        }
        made.all()
    }
}

/// The reading of `statement` that the consistency judge's answer on `shown`,
/// the passages it was asked with, makes; `None` when its reply could not be
/// read.
fn read_of(
    statement: &Statement<'_>,
    shown: &[EvidencePassage],
    judged: Option<(ConsistencyOutcome, bool)>,
) -> Read {
    let Statement {
        unit,
        claim,
        classed,
    } = *statement;
    match judged {
        None => Read {
            outcome: Outcome::Unjudged,
            finding: unjudged_finding(unit, claim, classed, shown),
            reused: false,
        },
        Some((consistency, reused)) => {
            let verdict = match consistency.verdict {
                ConsistencyVerdict::Consistent => ClaimVerdict::Supported,
                ConsistencyVerdict::Contradicted => ClaimVerdict::Contradicted,
                ConsistencyVerdict::UnfoundedSpecific => ClaimVerdict::Unsupported,
            };
            Read {
                outcome: consistency.outcome(),
                finding: finding_of(
                    (unit, claim, classed),
                    verdict,
                    consistency.reason,
                    shown,
                    None,
                ),
                reused,
            }
        }
    }
}
