//! `Page`: the strict judge on the course's first page for a statement (pass
//! B), without the passages its own citation shows. The page is the one
//! `corroborate` reads: restored from its saved selection or read now, sized
//! by what the job's answers have cost. Pages are packed into requests as the
//! pass packs them; while the job has measured no answer, a page is asked
//! about alone, so the next is sized by it.
use super::super::evidence_checks::{
    as_checked, attribution_page, combine, group, pack, PageBudget, Reach, Sizing,
};
use super::*;

pub(super) struct Page;

/// What a statement's page was read from, and the sources its sentence cites.
struct Paged {
    reach: Option<Reach>,
    cited: Vec<String>,
}

#[async_trait]
impl Reader for Page {
    fn reading(&self) -> Reading {
        Reading::Page
    }

    async fn read(&self, at: &ReadContext<'_>, statements: &[Statement<'_>]) -> Result<Vec<Read>> {
        let mut found: Vec<Option<Finding>> = vec![None; statements.len()];
        let mut paged = Vec::with_capacity(statements.len());
        let mut waiting = Vec::new();
        for (ordinal, statement) in statements.iter().enumerate() {
            let (page, opened) = open(at, ordinal, statement).await?;
            paged.push(page);
            match opened {
                Restored::Receipt(finding) => keep(&mut found, ordinal, finding),
                Restored::Ask(pending) if answers(at.retained).per_claim().is_some() => {
                    waiting.push(pending);
                }
                Restored::Ask(pending) => ask(at, vec![pending], &mut found).await?,
            }
        }
        ask(at, waiting, &mut found).await?;
        let mut reads = Vec::with_capacity(statements.len());
        for ((statement, page), finding) in statements.iter().zip(paged).zip(found) {
            let finding = finding.ok_or_else(|| {
                AppError::InternalError("A page was read without a finding".into())
            })?;
            reads.push(read_of(statement, page, finding));
        }
        Ok(reads)
    }
}

/// `statement`'s first page and its comparison, restored or left to ask.
async fn open(
    at: &ReadContext<'_>,
    ordinal: usize,
    statement: &Statement<'_>,
) -> Result<(Paged, Restored)> {
    let Statement { unit, claim, .. } = *statement;
    let checked = as_checked(claim, at.citations, at.references);
    let (_, shown) = attribution_page(
        at.llm,
        at.executions,
        at.citations,
        (unit, &claim.statement),
        &checked.statement,
    );
    let sizing = Sizing::Sized(PageBudget::of(at.llm, answers(at.retained)));
    let Some(selected) = evidence_selection::select(
        at.llm,
        (claim, &checked.statement, unit),
        at.references,
        &at.sources,
        at.retained,
        (&shown, &sizing),
    )
    .await?
    else {
        return Err(AppError::InternalError("A page was not sized".into()));
    };
    let mut cited: Vec<String> = at
        .citations
        .of_claim(unit, &claim.statement)
        .iter()
        .map(|passage| passage.source_id.clone())
        .collect();
    cited.dedup();
    let pending = comparison(
        at,
        ordinal,
        statement,
        &checked,
        selected.evidence,
        Pass::Corroboration,
    );
    let page = Paged {
        reach: selected.reach,
        cited,
    };
    Ok((page, restored(at, pending).await?))
}

/// Asks the strict judge about `pending`, packed into requests, and keeps
/// each answer under its statement.
async fn ask(
    at: &ReadContext<'_>,
    pending: Vec<PendingClaim>,
    found: &mut [Option<Finding>],
) -> Result<()> {
    for request in pack(at.llm, pending, &at.labels) {
        let (results, answered) =
            group(at.llm, request, &|_: Pass, _: &CheckedClaim| {}, &at.labels).await?;
        tally(at.retained, answered);
        for (_, (ordinal, _, finding, _)) in results {
            keep(found, ordinal, finding);
        }
    }
    Ok(())
}

fn keep(found: &mut [Option<Finding>], ordinal: usize, finding: Finding) {
    if let Some(slot) = found.get_mut(ordinal) {
        *slot = Some(finding);
    }
}

/// What the judge's finding on a statement's page comes to: it stands when the
/// passage that supports it is of a source its sentence cites, and otherwise
/// the support is elsewhere, which the finding names.
fn read_of(statement: &Statement<'_>, page: Paged, finding: Finding) -> Read {
    let mut finding = combine(None, finding, (page.reach, 1), &page.cited);
    let on_cited = finding
        .two_pass
        .supported_by
        .as_ref()
        .is_some_and(|found| page.cited.contains(&found.source_id));
    let outcome = page_outcome(finding.verdict, on_cited);
    if outcome == Outcome::Stands {
        finding.two_pass.supported_by = None;
    }
    finding.class = Some(statement.classed);
    Read { outcome, finding }
}
