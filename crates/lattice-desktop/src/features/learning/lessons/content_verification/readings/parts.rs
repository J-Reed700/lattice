//! `Parts`: the part-wise judge on a statement's own citation (pass A). A
//! quotation found in the source it names, or a claim the user resolved with
//! evidence of their own, is settled without a call; otherwise the judge reads
//! the passages its sentence cites that it holds, with the section's passed
//! example runs, and with no passage at all nothing is asked.
use super::super::evidence_checks::{as_checked, attribution_page};
use super::*;

pub(super) struct Parts;

#[async_trait]
impl Reader for Parts {
    #[cfg(test)]
    fn reading(&self) -> Reading {
        Reading::Parts
    }

    async fn read(&self, at: &ReadContext<'_>, statements: &[Statement<'_>]) -> Result<Vec<Read>> {
        let mut reads = Vec::with_capacity(statements.len());
        for (ordinal, statement) in statements.iter().enumerate() {
            reads.push(read_one(at, ordinal, statement).await?);
        }
        Ok(reads)
    }
}

async fn read_one(at: &ReadContext<'_>, ordinal: usize, statement: &Statement<'_>) -> Result<Read> {
    let Statement {
        unit,
        claim,
        classed,
    } = *statement;
    // A quotation found in its source is settled by the match; evidence the
    // user brought is a saved check.
    let settled = match quote_claims::settle(at.references, at.citations, unit, claim)? {
        Some(finding) => Some((finding, false)),
        None => at
            .retained
            .settled_finding(unit, claim)
            .map(|finding| (finding, true)),
    };
    if let Some((mut finding, reused)) = settled {
        finding.class = Some(classed);
        return Ok(Read {
            outcome: parts_outcome(&finding),
            finding,
            reused,
        });
    }
    let checked = as_checked(claim, at.citations, at.references);
    let (evidence, _) = attribution_page(
        at.executions,
        at.citations,
        (unit, &claim.statement),
        &checked.statement,
        parts_fit(at),
    );
    let (mut finding, reused) = if evidence.is_empty() {
        (no_passages(statement), false)
    } else {
        judge_parts(at, ordinal, statement, &checked, evidence).await?
    };
    finding.class = Some(classed);
    Ok(Read {
        outcome: parts_outcome(&finding),
        finding,
        reused,
    })
}
