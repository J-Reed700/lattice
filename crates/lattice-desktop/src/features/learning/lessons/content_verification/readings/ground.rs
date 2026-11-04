//! `Ground`: the part-wise judge on the section's cited passages, as many as it
//! holds, asked only whether they ground a statement a closed-book reading
//! called false. It is the same judge as `Parts` on other passages, read as
//! another question.
use super::super::evidence_checks::as_checked;
use super::*;

pub(super) struct Ground;

#[async_trait]
impl Reader for Ground {
    #[cfg(test)]
    fn reading(&self) -> Reading {
        Reading::Ground
    }

    async fn read(&self, at: &ReadContext<'_>, statements: &[Statement<'_>]) -> Result<Vec<Read>> {
        let mut reads = Vec::with_capacity(statements.len());
        for (ordinal, statement) in statements.iter().enumerate() {
            let Statement {
                unit,
                claim,
                classed,
            } = *statement;
            let checked = as_checked(claim, at.citations, at.references);
            let mut evidence = take_cited(
                parts_fit(at),
                &checked.statement,
                &mut Vec::new(),
                at.citations.of(unit),
            );
            canonicalize_evidence(&mut evidence);
            let (mut finding, reused) = if evidence.is_empty() {
                (no_passages(statement), false)
            } else {
                judge_parts(at, ordinal, statement, &checked, evidence).await?
            };
            finding.class = Some(classed);
            reads.push(Read {
                outcome: ground_outcome(&finding),
                finding,
                reused,
            });
        }
        Ok(reads)
    }
}
