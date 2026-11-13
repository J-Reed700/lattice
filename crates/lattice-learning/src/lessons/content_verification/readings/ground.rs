//! `Ground`: the part-wise judge on the section's cited passages, as many as it
//! holds, asked only whether they ground a statement a closed-book reading
//! called false. It is the same judge as `Parts` on other passages, read as
//! another question. Every receipt of its batch is restored before its first
//! call.
use super::super::evidence_checks::{as_checked, attribution_page};
use super::*;

pub(super) struct Ground;

#[async_trait]
impl Reader for Ground {
    #[cfg(test)]
    fn reading(&self) -> Reading {
        Reading::Ground
    }

    async fn read(&self, at: &ReadContext<'_>, statements: &[Statement<'_>]) -> Result<Vec<Read>> {
        let mut made = Made::new(statements.len());
        let mut asks = Vec::new();
        for (ordinal, statement) in statements.iter().enumerate() {
            let Statement { unit, claim, .. } = *statement;
            let checked = as_checked(claim, at.citations, at.references);
            let (evidence, _) = attribution_page(
                at.executions,
                at.citations,
                (unit, &claim.quote),
                &checked.statement,
                parts_fit(at),
            );
            match restore_parts(at, ordinal, statement, &checked, evidence).await? {
                Restored::Receipt(finding, reused) => {
                    made.put(at, ordinal, read_of(statement, *finding, reused));
                }
                Restored::Ask(pending) => asks.push((ordinal, *statement, pending)),
            }
        }
        let mut asked: futures::stream::FuturesUnordered<_> = asks
            .into_iter()
            .map(|(ordinal, statement, pending)| async move {
                let finding = ask_parts(at, statement.unit, pending).await?;
                Ok::<_, AppError>((ordinal, statement, finding))
            })
            .collect();
        while let Some(asked) = futures::StreamExt::next(&mut asked).await {
            let (ordinal, statement, finding) = asked?;
            made.put(at, ordinal, read_of(&statement, finding, false));
        }
        made.all()
    }
}

/// The reading of `statement` that `finding` on the section's passages makes.
fn read_of(statement: &Statement<'_>, mut finding: Finding, reused: bool) -> Read {
    finding.class = Some(statement.classed);
    Read::new(ground_outcome(&finding), finding, reused)
}
