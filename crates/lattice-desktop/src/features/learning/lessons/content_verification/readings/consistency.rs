//! `Consistency`: the consistency judge on the section's passages, as a
//! Reasoning statement meets it today.
use super::super::class_checks::{consistency_of, finding_of, fitted, unjudged_finding};
use super::*;
use lattice_core::application::services::claim_verification::ConsistencyVerdict;

pub(super) struct Consistency;

#[async_trait]
impl Reader for Consistency {
    #[cfg(test)]
    fn reading(&self) -> Reading {
        Reading::Consistency
    }

    async fn read(&self, at: &ReadContext<'_>, statements: &[Statement<'_>]) -> Result<Vec<Read>> {
        let mut reads = Vec::with_capacity(statements.len());
        for statement in statements {
            let Statement {
                unit,
                claim,
                classed,
            } = *statement;
            let shown = fitted(at.llm, &claim.statement, at.citations.of(unit));
            let read = match consistency_of(at.llm, &claim.statement, &shown).await? {
                None => Read {
                    outcome: Outcome::Unjudged,
                    finding: unjudged_finding(unit, claim, classed, &shown),
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
                            &shown,
                            None,
                        ),
                        reused,
                    }
                }
            };
            reads.push(read);
        }
        Ok(reads)
    }
}
