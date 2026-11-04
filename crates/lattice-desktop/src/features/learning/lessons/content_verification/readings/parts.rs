//! `Parts`: the part-wise judge on a statement's own citation. A
//! quotation found in the source it names, or a claim the user resolved with
//! evidence of their own, is settled without a call; otherwise the judge reads
//! the passages its sentence cites that it holds, with the section's passed
//! example runs. A statement with no passage at all is read on the course's
//! page, as `Page` reads it: nothing it cites is there to be read, so the page
//! is the only place its support can be.
use super::super::evidence_checks::{as_checked, attribution_page};
use super::page::Page;
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
        let mut bare = Vec::new();
        for (ordinal, statement) in statements.iter().enumerate() {
            let read = read_one(at, ordinal, statement).await?;
            if read.is_none() {
                bare.push((ordinal, *statement));
            }
            reads.push(read);
        }
        if !bare.is_empty() {
            let (ordinals, those): (Vec<usize>, Vec<Statement<'_>>) = bare.into_iter().unzip();
            for (ordinal, read) in ordinals.into_iter().zip(Page.read(at, &those).await?) {
                if let Some(slot) = reads.get_mut(ordinal) {
                    *slot = Some(read);
                }
            }
        }
        reads
            .into_iter()
            .map(|read| {
                read.ok_or_else(|| {
                    AppError::InternalError("A statement was read without a reading".into())
                })
            })
            .collect()
    }
}

/// `statement`'s reading on its own passages, or `None` when it has none.
async fn read_one(
    at: &ReadContext<'_>,
    ordinal: usize,
    statement: &Statement<'_>,
) -> Result<Option<Read>> {
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
        return Ok(Some(Read {
            outcome: parts_outcome(&finding),
            finding,
            reused,
        }));
    }
    let checked = as_checked(claim, at.citations, at.references);
    let (evidence, _) = attribution_page(
        at.executions,
        at.citations,
        (unit, &claim.statement),
        &checked.statement,
        parts_fit(at),
    );
    if evidence.is_empty() {
        return Ok(None);
    }
    let (mut finding, reused) = judge_parts(at, ordinal, statement, &checked, evidence).await?;
    finding.class = Some(classed);
    Ok(Some(Read {
        outcome: parts_outcome(&finding),
        finding,
        reused,
    }))
}
