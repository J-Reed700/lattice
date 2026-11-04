//! `Parts`: the part-wise judge on a statement's own citation (pass A). A
//! quotation found in the source it names is settled in code; otherwise the
//! judge reads the passages its sentence cites, with the section's passed
//! example runs, and with no passage at all nothing is asked.
use super::super::evidence_checks::{as_checked, attribution_page};
use super::*;

pub(super) struct Parts;

#[async_trait]
impl Reader for Parts {
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
    if let Some(mut finding) = quote_claims::settle(at.references, at.citations, unit, claim)? {
        finding.class = Some(classed);
        return Ok(Read {
            outcome: Outcome::Stands,
            finding,
        });
    }
    let checked = as_checked(claim, at.citations, at.references);
    let (evidence, _) = attribution_page(
        at.llm,
        at.executions,
        at.citations,
        (unit, &claim.statement),
        &checked.statement,
    );
    let mut finding = if evidence.is_empty() {
        no_passages(statement)
    } else {
        judge_parts(at, ordinal, statement, &checked, evidence).await?
    };
    finding.class = Some(classed);
    Ok(Read {
        outcome: parts_outcome(&finding),
        finding,
    })
}
