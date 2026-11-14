//! `Page`: the strict judge on the course's first page for a statement,
//! without the passages its own citation shows. The page is restored from its
//! saved selection or read now, sized by what the job's answers have cost.
//! Every statement whose page is saved is opened, and its receipt restored,
//! before the first request. Pages are packed into requests by section; while
//! the job has measured no answer, a page is asked about alone, so the next is
//! sized by it.
use super::super::evidence_checks::{
    as_checked, attribution_page, page_finding, PageBudget, Reach,
};
use super::*;

pub(super) struct Page;

/// What a statement's page was read from, and the sources its sentence cites.
struct Paged {
    reach: Option<Reach>,
    cited: Vec<String>,
}

/// A statement of a batch, and what has been made of it.
struct Held<'s> {
    statement: Statement<'s>,
    /// Its page, once opened.
    paged: Option<Paged>,
    /// Its comparison, once opened and until it is asked.
    pending: Option<PendingClaim>,
    /// Its reading, once known.
    read: Option<Read>,
}

/// The statements of a batch the page reader reads, between restoring what a
/// saved page and receipt give and asking the rest.
pub(super) struct Batch<'s> {
    held: Vec<Held<'s>>,
}

#[async_trait]
impl Reader for Page {
    #[cfg(test)]
    fn reading(&self) -> Reading {
        Reading::Page
    }

    async fn read(&self, at: &ReadContext<'_>, statements: &[Statement<'_>]) -> Result<Vec<Read>> {
        let sized = answers(at.retained).per_claim().is_some();
        let mut batch = Batch::restore(at, statements, sized).await?;
        batch.ask(at).await?;
        batch.reads()
    }
}

impl<'s> Batch<'s> {
    /// The batch with every statement opened whose page can be had before the
    /// first request: all of them when the job's answers are `sized`, otherwise
    /// those whose page is saved. A page read now waits for the answers that
    /// size it.
    pub(super) async fn restore(
        at: &ReadContext<'_>,
        statements: &[Statement<'s>],
        sized: bool,
    ) -> Result<Self> {
        let mut held: Vec<Held<'s>> = statements
            .iter()
            .map(|statement| Held {
                statement: *statement,
                paged: None,
                pending: None,
                read: None,
            })
            .collect();
        for (ordinal, one) in held.iter_mut().enumerate() {
            if sized || saved(at, &one.statement).await? {
                one.load(at, ordinal).await?;
            }
        }
        Ok(Self { held })
    }

    /// Opens the statements left, in order, and asks about every comparison no
    /// receipt restored: while the job has measured no answer, the first alone,
    /// so the next page is sized by it, then the rest packed into requests.
    pub(super) async fn ask(&mut self, at: &ReadContext<'_>) -> Result<()> {
        let mut waiting = Vec::new();
        for ordinal in 0..self.held.len() {
            let Some(one) = self.held.get_mut(ordinal) else {
                continue;
            };
            if one.paged.is_none() {
                one.load(at, ordinal).await?;
            }
            let Some(pending) = one.pending.take() else {
                continue;
            };
            if answers(at.retained).per_claim().is_some() {
                waiting.push(pending);
            } else {
                self.answer(at, vec![pending]).await?;
            }
        }
        self.answer(at, waiting).await
    }

    /// The reading of each statement, in the batch's order.
    pub(super) fn reads(self) -> Result<Vec<Read>> {
        self.held
            .into_iter()
            .map(|one| {
                one.read.ok_or_else(|| {
                    AppError::InternalError("A page was read without a finding".into())
                })
            })
            .collect()
    }

    /// Asks the strict judge about `pending`, packed into requests by section,
    /// and keeps each answer under its statement. The observer hears of each
    /// as soon as its comparison is finished, before the next request is sent.
    async fn answer(&mut self, at: &ReadContext<'_>, pending: Vec<PendingClaim>) -> Result<()> {
        let held = &self.held;
        let tell = |_: Pass, finished: &CheckedClaim| {
            let (ordinal, _, finding, _) = finished;
            let Some(one) = held.get(*ordinal) else {
                return;
            };
            if let Some(page) = &one.paged {
                let read = read_of(&one.statement, page, finding.clone(), false);
                at.report(*ordinal, &read);
            }
        };
        let answered = ask_strict(at, pending, &tell).await?;
        for (ordinal, finding) in answered {
            if let Some(one) = self.held.get_mut(ordinal) {
                one.settle(finding)?;
            }
        }
        Ok(())
    }
}

impl Held<'_> {
    /// Opens the statement: restores its page or reads it now, and restores
    /// its comparison from its receipt.
    async fn load(&mut self, at: &ReadContext<'_>, ordinal: usize) -> Result<()> {
        let (page, opened) = open(at, ordinal, &self.statement).await?;
        match opened {
            Restored::Receipt(finding, reused) => {
                let read = read_of(&self.statement, &page, *finding, reused);
                at.report(ordinal, &read);
                self.read = Some(read);
            }
            Restored::Ask(pending) => self.pending = Some(pending),
        }
        self.paged = Some(page);
        Ok(())
    }

    /// Keeps the strict judge's `finding` on the statement's page.
    fn settle(&mut self, finding: Finding) -> Result<()> {
        let page = self.paged.as_ref().ok_or_else(|| {
            AppError::InternalError("A page was answered before it was opened".into())
        })?;
        self.read = Some(read_of(&self.statement, page, finding, false));
        Ok(())
    }
}

/// Whether `statement`'s first page is saved and restores, so that no page
/// budget sizes it.
async fn saved(at: &ReadContext<'_>, statement: &Statement<'_>) -> Result<bool> {
    let Statement { unit, claim, .. } = *statement;
    evidence_selection::is_saved(
        at.llm,
        (claim, unit),
        at.references,
        &at.sources,
        at.retained,
    )
    .await
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
        at.executions,
        at.citations,
        (unit, &claim.quote),
        &checked.statement,
        parts_fit(at),
    );
    let budget = PageBudget::of(at.llm, answers(at.retained));
    let selected = evidence_selection::select(
        at.llm,
        (claim, &checked.statement, unit),
        at.references,
        &at.sources,
        at.retained,
        (&shown, &budget),
    )
    .await?;
    let mut cited: Vec<String> = at
        .citations
        .of_claim(unit, &claim.quote)
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
    Ok((page, restored(pending).await?))
}

/// What the judge's finding on a statement's page comes to: it stands when the
/// passage that supports it is of a source its sentence cites, and otherwise
/// the support is elsewhere, which the finding names. A sentence whose section
/// cites nothing has no citation to move: a passage that supports it stands.
fn read_of(statement: &Statement<'_>, page: &Paged, finding: Finding, reused: bool) -> Read {
    let mut finding = page_finding(finding, (page.reach, 1), &page.cited);
    let on_cited = page.cited.is_empty()
        || finding
            .two_pass
            .supported_by
            .as_ref()
            .is_some_and(|found| page.cited.contains(&found.source_id));
    let outcome = page_outcome(finding.verdict, on_cited);
    finding.class = Some(statement.classed);
    Read::new(outcome, finding, reused)
}
