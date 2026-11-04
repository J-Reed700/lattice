//! Claim checks in two passes (course evidence index §5), each decision
//! keeping its own evidence binding and durable receipt. Pass A (attribution)
//! checks a claim against the passages its own section cites; pass B
//! (corroboration) against the course's first page for it by relevance (see
//! `walk`), and a claim neither supports goes to web research. Requests hold
//! one section's claims while the window holds them with their answers, and
//! pass B pages are sized by what the job's answers cost (`page_budget`).
//! What the two passes decide together is `outcome`'s. Pass A reads a claim
//! part by part (`bridge`). Pass B reads a page for a claim whose section
//! cites nothing, and for one whose research round has ended; what a section
//! cites decides the rest alone (claim classes design §3.2).
use super::*;
use lattice_core::application::services::claim_verification::LocatedClaim;
#[cfg(test)]
use std::collections::BTreeMap;

mod bridge;
mod claims;
mod outcome;
mod page_budget;
mod requests;
#[cfg(test)]
mod run;
mod walk;

pub(super) use bridge::{applies, bridge_checker, part_wise};
pub(super) use claims::{attribution_page, compare, Comparison, NO_PASSAGES};
#[cfg(test)]
use claims::{Compared, Context, Prepared};
#[cfg(test)]
pub(in crate::features::learning) use outcome::ContraryPassage;
pub(super) use outcome::{alone, combine};
#[cfg(test)]
use outcome::{alone_key, decision_key};
pub(in crate::features::learning) use outcome::{
    same_passage, CheckedPart, Pass, SupportedElsewhere, SupportingPassage, TwoPass,
};
pub(in crate::features::learning) use page_budget::{page_budget, AnswerSize};
use requests::finish;
#[cfg(test)]
pub(super) use requests::{combined, packed};
pub(super) use requests::{group, labelled_passages, pack};
#[cfg(test)]
use run::Run;
#[cfg(test)]
pub(super) use walk::first_page;
#[cfg(test)]
use walk::log_page;
pub(in crate::features::learning) use walk::Reach;
pub(super) use walk::{page_of, page_one, within_page, PageBudget, PageNote, Sizing};

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
pub(super) type Labels = HashMap<String, String>;

/// Tells a pass's bookkeeping that one comparison finished, so a claim is
/// counted the moment its last comparison is done, even while its request's
/// other comparisons are pending.
type Answered<'a> = dyn Fn(Pass, &CheckedClaim) + Sync + 'a;

pub(super) struct PendingClaim {
    pub(super) ordinal: usize,
    pub(super) key: String,
    pub(super) receipt_key: String,
    pub(super) unit: usize,
    pub(super) claim: Claim,
    /// The claim's statement as the checker reads it (see [`as_checked`]).
    pub(super) checked: String,
    pub(super) evidence: Vec<EvidencePassage>,
    pub(super) pass: Pass,
}

/// `claim` as the checker reads it: each "Source N" of the lesson's text
/// written as that source's title, so the index cannot be read as passage N.
/// Its key names this text, so a check of the bare index is not restored.
pub(super) fn as_checked(
    claim: &Claim,
    citations: &Citations,
    references: &ReferenceCollection<'_>,
) -> Claim {
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
/// own section cites (`citations`, by unit), and against the course's first
/// page for it. Pages are sized by the job's measured answers (§5.3), so the
/// first request carries pass A only and every later one packs pass A and
/// pass B together. For the tests of the two passes; step 6b-3 retires it with
/// them.
#[cfg(test)]
pub(super) async fn check_with_citations(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    inventory: &Inventory,
    executions: &[execution::Observation],
    retained: &ClaimChecks,
    citations: &Citations,
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
            let (_, answered) = group(llm, first, &tell, &labels).await?;
            answers.add(answered);
            run = shared
                .into_inner()
                .unwrap_or_else(|error| error.into_inner());
        }
        let sizing = Sizing::Sized(PageBudget::of(llm, answers));
        let waiting: Vec<usize> = run
            .compared
            .values()
            .filter(|claim| claim.corroborates && claim.corroboration.is_none())
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
            .map(|request| group(llm, request, &tell, &labels))
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
    let mut results = run.finish()?;
    results.sort_by_key(|(ordinal, _, _, _)| *ordinal);
    // The gate call (§18.6): every supported verdict's support is weighed
    // before the claim ledger sees it.
    provenance_gate::weigh(llm, &mut results, &run.decided).await?;
    *retained
        .answers
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = answers;
    Ok(results)
}
