//! `Parts`: the part-wise judge on a statement's own citation. A
//! quotation found in the source it names, or a claim the user resolved with
//! evidence of their own, is settled without a call; otherwise the judge reads
//! the passages its sentence cites that it holds, with the section's passed
//! example runs. Every reading a settle or a receipt gives is made before the
//! first call. A statement with no passage at all is read on the course's
//! page, as `Page` reads it: nothing it cites is there to be read, so the page
//! is the only place its support can be.
use super::super::evidence_checks::{as_checked, attribution_page};
use super::page::Batch;
use super::*;

pub(super) struct Parts;

/// What a statement's own passages give without a call.
enum Own {
    /// Its reading: settled, or restored from its receipt.
    Read(Box<Read>),
    /// A comparison with its passages that no receipt restored.
    Ask(PendingClaim),
    /// It has no passage of its own.
    Bare,
}

#[async_trait]
impl Reader for Parts {
    #[cfg(test)]
    fn reading(&self) -> Reading {
        Reading::Parts
    }

    async fn read(&self, at: &ReadContext<'_>, statements: &[Statement<'_>]) -> Result<Vec<Read>> {
        let mut made = Made::new(statements.len());
        let mut asks = Vec::new();
        let mut bare = Vec::new();
        for (ordinal, statement) in statements.iter().enumerate() {
            match restore(at, ordinal, statement).await? {
                Own::Read(read) => made.put(at, ordinal, *read),
                Own::Ask(pending) => asks.push((ordinal, *statement, pending)),
                Own::Bare => bare.push((ordinal, *statement)),
            }
        }
        // The page tells of the statements it reads by their place among
        // those it was handed, which are not the places in this batch.
        let (ordinals, those): (Vec<usize>, Vec<Statement<'_>>) = bare.into_iter().unzip();
        let forward = |inner: usize, read: &Read| {
            if let Some(ordinal) = ordinals.get(inner) {
                at.report(*ordinal, read);
            }
        };
        let handed = at.observed(&forward);
        let mut page = Batch::restore(&handed, &those, false).await?;
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
        page.ask(&handed).await?;
        for (ordinal, read) in ordinals.iter().zip(page.reads()?) {
            made.keep(*ordinal, read);
        }
        made.all()
    }
}

/// What `statement`'s own passages give without a call: its reading when a
/// quotation or the user's evidence settles it or its receipt is saved, the
/// comparison to ask otherwise, or that it has no passage of its own.
async fn restore(at: &ReadContext<'_>, ordinal: usize, statement: &Statement<'_>) -> Result<Own> {
    let Statement { unit, claim, .. } = *statement;
    // A quotation found in its source is settled by the match; evidence the
    // user brought is a saved check.
    let settled = match quote_claims::settle(at.references, at.citations, unit, claim)? {
        Some(finding) => Some((finding, false)),
        None => at
            .retained
            .settled_finding(unit, claim)
            .map(|finding| (finding, true)),
    };
    if let Some((finding, reused)) = settled {
        return Ok(Own::Read(Box::new(read_of(statement, finding, reused))));
    }
    let checked = as_checked(claim, at.citations, at.references);
    let (evidence, _) = attribution_page(
        at.executions,
        at.citations,
        (unit, &claim.quote),
        &checked.statement,
        parts_fit(at),
    );
    if evidence.is_empty() {
        return Ok(Own::Bare);
    }
    Ok(
        match restore_parts(at, ordinal, statement, &checked, evidence).await? {
            Restored::Receipt(finding, reused) => {
                Own::Read(Box::new(read_of(statement, *finding, reused)))
            }
            Restored::Ask(pending) => Own::Ask(pending),
        },
    )
}

/// The reading of `statement` that `finding` on its own passages makes.
fn read_of(statement: &Statement<'_>, mut finding: Finding, reused: bool) -> Read {
    finding.class = Some(statement.classed);
    Read::new(parts_outcome(&finding), finding, reused)
}
