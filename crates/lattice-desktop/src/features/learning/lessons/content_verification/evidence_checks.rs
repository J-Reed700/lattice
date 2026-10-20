//! Claim checks in two passes (course evidence index §5). Retrieve and
//! restore individual comparisons, then batch only unfinished ones; every
//! decision keeps its own evidence binding and durable receipt.
//!
//! Pass A, attribution: a claim against the passages its own section cites
//! (and the section's passed example runs), nothing else. Its key names the
//! claim and those passages only, so it stays the same across runs for as
//! long as the citation does, whatever the course's ranking. A section that
//! cites nothing has no pass A.
//!
//! Pass B, corroboration: the claim against the course by relevance (see
//! `walk`), never showing what pass A shows. Its first page is checked for
//! every claim (D1), so a contradiction anywhere in the course is caught and
//! a weak citation repaired; it is restored across runs while the course's
//! sources are unchanged (`evidence_selection`). A page is sized by what
//! the job's answers cost, so a job's first request carries pass A only and
//! measures it; every later request packs a claim's pass A and pass B
//! comparisons together (see [`check_with_paging`]). A claim neither page
//! supports reads on through pass B, page by page, until a page decides it
//! or its lists run out, and then goes to web research. What the two passes
//! decide together is `outcome`'s.
//!
//! Requests hold the claims of one section, a claim's comparisons together,
//! while the batch estimate says the window holds them all with their
//! answers; no count caps a request. A comparison too long for its claim
//! checked alone loses its least needed passages, and a request the backend
//! counts past the window is checked in halves on the same evidence. A pass
//! B page is read with its most relevant passages at the edges; its
//! canonical order still keys the comparison.
//!
//! Every check reports what its answer cost in output tokens; the job's
//! mean, with the backend's measured pace, sizes pass B's pages
//! (`page_budget`). Progress counts each claim once: a deeper page's check
//! adds no check, and moves its claim out of the unresolved count when it
//! supports it.
use super::*;
use lattice_core::application::services::claim_verification::LocatedClaim;
use std::collections::BTreeMap;

mod claims;
mod outcome;
mod page_budget;
mod requests;
mod run;
mod walk;

use claims::{Compared, Comparison, Context, Prepared};
#[cfg(test)]
pub(in crate::features::learning) use outcome::ContraryPassage;
use outcome::{combine, decision_key};
pub(in crate::features::learning) use outcome::{Pass, TwoPass};
pub(in crate::features::learning) use page_budget::{page_budget, page_seconds, AnswerSize};
#[cfg(test)]
pub(super) use requests::{combined, packed};
use requests::{finish, group, pack};
use run::Run;
use walk::{deepen, log_page, walk_budget, Walk};
#[cfg(test)]
pub(super) use walk::{first_page, page_tokens};
pub(super) use walk::{page_of, page_one, within_page, PageBudget, PageNote, Sizing};
#[cfg(test)]
pub(in crate::features::learning) use walk::{walk_pages, WalkedPage};
#[cfg(test)]
pub(in crate::features::learning) use walk::{Depth, ORDERS, REACHES};
pub(in crate::features::learning) use walk::{Paging, Reach};

/// The most output a claim check may write, which the checker is built
/// with: the largest an answer can be.
pub(in crate::features::learning) const CHECKER_OUTPUT_TOKENS: u32 = 512;

/// The page budget pass B pages by after what the job's checks have cost so
/// far, as the verification report says it: its tokens, or that no measured
/// pace gives one.
pub(super) fn page_budget_note(llm: &dyn LLMPort, retained: &ClaimChecks) -> String {
    PageBudget::of(
        llm,
        *retained
            .answers
            .lock()
            .unwrap_or_else(|error| error.into_inner()),
    )
    .text()
}

/// Each passage's label, by source version (`provenance_gate::passage_labels`).
type Labels = HashMap<String, String>;

/// Tells a pass's bookkeeping that one comparison finished.
type Answered<'a> = dyn Fn(Pass, &CheckedClaim) + Sync + 'a;

/// What a finished comparison reports to lesson progress.
#[derive(Clone, Copy)]
enum Check<'a> {
    /// A claim's pass A comparison or its pass B first page: each is told
    /// as it finishes, so a claim is counted the moment its last comparison
    /// is done, even while its request's other comparisons are pending.
    First(&'a Answered<'a>),
    /// An unsupported claim's check against a deeper pass B page.
    Deeper,
}

struct PendingClaim {
    ordinal: usize,
    key: String,
    receipt_key: String,
    unit: usize,
    claim: Claim,
    /// The claim's statement as the checker reads it (see [`as_checked`]).
    checked: String,
    evidence: Vec<EvidencePassage>,
    pass: Pass,
}

/// `claim` as the checker reads it: each "Source N" of the lesson's text
/// written as that source's title, so the index cannot be read as passage N.
/// Its key names this text, so a check of the bare index is not restored.
fn as_checked(claim: &Claim, citations: &Citations, references: &ReferenceCollection<'_>) -> Claim {
    Claim {
        quote: claim.quote.clone(),
        statement: citations.label(&claim.statement, references),
    }
}

#[cfg(test)]
pub(super) async fn check(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    inventory: &Inventory,
    executions: &[execution::Observation],
    retained: &ClaimChecks,
) -> Result<Vec<CheckedClaim>> {
    check_with_citations(
        llm,
        references,
        inventory,
        executions,
        retained,
        &Citations::default(),
    )
    .await
}

/// Check every claim of `inventory` in two passes: against the passages its
/// own section cites (`citations`, by unit), and across the course.
pub(super) async fn check_with_citations(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    inventory: &Inventory,
    executions: &[execution::Observation],
    retained: &ClaimChecks,
    citations: &Citations,
) -> Result<Vec<CheckedClaim>> {
    check_with_paging(
        llm,
        references,
        inventory,
        executions,
        retained,
        (citations, Paging::SET),
    )
    .await
}

/// [`check_with_citations`] with pass B paged by `paging`.
///
/// Pass B's pages are sized by the job's measured answers (§5.3), so until
/// one is measured the job's first request carries pass A comparisons only,
/// packed by section as usual; every first page left is then read and sized
/// by what those answers cost, and every later request packs pass A and
/// pass B together. A job with no pass A to measure by asks its first claim
/// waiting for a first page alone, that page bounded by the window, and
/// sizes the rest by its answer.
pub(super) async fn check_with_paging(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    inventory: &Inventory,
    executions: &[execution::Observation],
    retained: &ClaimChecks,
    (citations, paging): (&Citations, Paging),
) -> Result<Vec<CheckedClaim>> {
    let entries: Vec<_> = inventory
        .units
        .iter()
        .flat_map(|unit| {
            unit.claims
                .iter()
                .cloned()
                .map(move |claim| (unit.index, claim))
        })
        .collect();
    let total = entries.len();
    claim_reuse::load_prior_claims(retained).await?;
    let labels = provenance_gate::passage_labels(references).await?;
    let sources = evidence_selection::sources(references);
    let context = Context {
        llm,
        references,
        sources: &sources,
        executions,
        retained,
        citations,
        paging,
    };
    crate::features::learning::lesson_progress::phase(
        crate::features::learning::lesson_progress::Phase::Evidence,
        format!("Looking for reusable checks across {total} lesson claims"),
    );
    crate::features::learning::lesson_progress::begin_checks(total);
    let mut answers = *retained
        .answers
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let sizing = match answers.per_claim() {
        Some(_) => Sizing::Sized(PageBudget::of(llm, answers)),
        None => Sizing::Unmeasured,
    };
    // Restore ALL reusable results before the first model request. Otherwise a
    // slow early comparison hides saved work later in the inventory, making a
    // resumed or expanded-evidence pass look like a complete restart.
    let mut run = Run::default();
    // A section at a time, its claims together, as they are packed.
    let mut sections: Vec<Vec<(usize, (usize, Claim))>> = Vec::new();
    for (ordinal, entry) in entries.into_iter().enumerate() {
        match sections.last_mut() {
            Some(section)
                if section
                    .first()
                    .is_some_and(|(_, (unit, _))| *unit == entry.0) =>
            {
                section.push((ordinal, entry));
            }
            _ => sections.push(vec![(ordinal, entry)]),
        }
    }
    for section in sections {
        let claims = section.len();
        // Drain the reads before any subsequent model work/checkpoint writes.
        // Unpolled prefetches can otherwise hold scarce database pool slots.
        let prepared: Vec<_> = futures::stream::iter(section)
            .map(|(ordinal, claim)| context.prepare(ordinal, claim, &sizing))
            .buffered(claims)
            .try_collect()
            .await?;
        for item in prepared {
            match item {
                Prepared::Complete(result, final_decision) => run.complete(*result, final_decision),
                Prepared::Compared(claim) => {
                    run.compared.insert(claim.ordinal, *claim);
                }
            }
        }
        crate::features::learning::lesson_progress::stage(format!(
            "Compared current evidence for {} of {total} claims · {} saved checks reused so far",
            run.results.len() + run.compared.len(),
            run.reused
        ));
    }
    run.report_decided();
    if let Sizing::Unmeasured = sizing {
        // The job's first request measures what an answer costs.
        let first = run.measuring_request(&context, &labels).await?;
        if !first.is_empty() {
            crate::features::learning::lesson_progress::stage(format!(
                "Checking {} claims against their citations",
                first.len()
            ));
            let shared = std::sync::Mutex::new(std::mem::take(&mut run));
            let tell = |pass: Pass, result: &CheckedClaim| {
                shared
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .answer(pass, result);
            };
            let (_, answered) = group(llm, first, Check::First(&tell), &labels).await?;
            answers.add(answered);
            run = shared
                .into_inner()
                .unwrap_or_else(|error| error.into_inner());
        }
        let sizing = Sizing::Sized(PageBudget::of(llm, answers));
        let waiting: Vec<usize> = run
            .compared
            .values()
            .filter(|claim| claim.corroboration.is_none())
            .map(|claim| claim.ordinal)
            .collect();
        for ordinal in waiting {
            if let Some(claim) = run.compared.get_mut(&ordinal) {
                if let Some(finding) = context.corroborate(claim, &sizing).await? {
                    let key = claim.key.clone();
                    run.compared.remove(&ordinal);
                    run.complete((ordinal, key, finding, true), true);
                }
            }
        }
        run.report_decided();
    }
    let pending: Vec<PendingClaim> = run.compared.values_mut().flat_map(Compared::send).collect();
    run.asked.extend(pending.iter().map(|item| item.ordinal));
    let model_checks = run.asked.len();
    crate::features::learning::lesson_progress::plan_model_checks(model_checks);
    tracing::info!(
        total,
        reused = run.reused,
        model_checks,
        page_budget = %PageBudget::of(llm, answers).text(),
        "Lesson claim check plan ready"
    );
    let attributing = pending
        .iter()
        .filter(|item| item.pass == Pass::Attribution)
        .count();
    crate::features::learning::lesson_progress::stage(format!(
        "{} saved checks reused · Checking {attributing} claims against their citations · Corroborating {} claims across the course, page 1",
        run.reused,
        pending.len() - attributing
    ));
    // A request already carries several complete evidence sets. Run them one
    // at a time rather than compete prompt prefills on the same model.
    let shared = std::sync::Mutex::new(std::mem::take(&mut run));
    {
        let tell = |pass: Pass, result: &CheckedClaim| {
            shared
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .answer(pass, result);
        };
        let mut groups = futures::stream::iter(pack(llm, pending, &labels))
            .map(|request| group(llm, request, Check::First(&tell), &labels))
            .buffer_unordered(1);
        while let Some(batch) = groups.next().await {
            let (_, answered) = batch?;
            answers.add(answered);
            let run = shared.lock().unwrap_or_else(|error| error.into_inner());
            crate::features::learning::lesson_progress::stage(format!(
                "Completed {} of {model_checks} model checks · {} saved checks reused · {total} lesson claims",
                run.done, run.reused
            ));
        }
    }
    let mut run = shared
        .into_inner()
        .unwrap_or_else(|error| error.into_inner());
    let mut results = run.finish(&context, &labels, &mut answers).await?;
    *retained
        .answers
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = answers;
    results.sort_by_key(|(ordinal, _, _, _)| *ordinal);
    // The gate call (§18.6): every supported verdict's support is weighed
    // before the claim ledger sees it; no further model check is made.
    provenance_gate::weigh(llm, &mut results, &run.decided).await?;
    crate::features::learning::lesson_progress::sent_back(
        results
            .iter()
            .filter(|(_, _, finding, _)| finding.scrutinised().is_some())
            .count(),
    );
    Ok(results)
}
