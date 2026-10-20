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
//! every claim, packed into the same request as the claim's pass A
//! comparison, so a contradiction anywhere in the course is caught and a
//! weak citation repaired; it is restored across runs while the course's
//! sources are unchanged (`evidence_selection`). A claim neither page
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

mod outcome;
mod page_budget;
mod requests;
mod walk;

use outcome::{combine, decision_key};
#[cfg(test)]
pub(in crate::features::learning) use outcome::{ContraryPassage, SupportedElsewhere};
pub(in crate::features::learning) use outcome::{Pass, TwoPass};
pub(in crate::features::learning) use page_budget::{page_budget, AnswerSize};
#[cfg(test)]
pub(super) use requests::{combined, packed};
use requests::{finish, group, pack};
#[cfg(test)]
pub(super) use walk::first_page;
use walk::{budget, deepen, log_page, walk_budget, Walk};
pub(super) use walk::{page_of, page_one, within_page, PageNote};
#[cfg(test)]
pub(in crate::features::learning) use walk::{walk_pages, WalkedPage};
pub(in crate::features::learning) use walk::{Depth, Paging, Reach};
#[cfg(test)]
pub(in crate::features::learning) use walk::{ORDERS, REACHES};

/// The most output a claim check may write, which the checker is built
/// with: the largest an answer can be.
pub(in crate::features::learning) const CHECKER_OUTPUT_TOKENS: u32 = 512;

/// The page budget pass B pages by after what the job's checks have cost so
/// far, as the verification report says it: its tokens, or that no measured
/// pace gives one.
pub(super) fn page_budget_note(llm: &dyn LLMPort, retained: &ClaimChecks) -> String {
    walk::budget_text(budget(
        llm,
        *retained
            .answers
            .lock()
            .unwrap_or_else(|error| error.into_inner()),
    ))
}

/// Each passage's label, by source version (`provenance_gate::passage_labels`).
type Labels = HashMap<String, String>;

/// What a finished comparison reports to lesson progress.
#[derive(Clone, Copy)]
enum Check {
    /// A claim's pass A comparison or its pass B first page.
    First,
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

/// One of a claim's comparisons in this pass.
enum Comparison {
    /// Its finding, and whether a saved check gave it.
    Done(Finding, bool),
    Pending(PendingClaim),
    /// Sent to the model; its finding is on the way.
    Sent,
}

impl Comparison {
    fn done(&self) -> Option<(&Finding, bool)> {
        match self {
            Self::Done(finding, reused) => Some((finding, *reused)),
            _ => None,
        }
    }
}

/// A claim whose two passes are compared in this pass.
struct Compared {
    ordinal: usize,
    unit: usize,
    claim: Claim,
    checked: String,
    /// The key of its whole decision (`outcome::decision_key`).
    key: String,
    attribution: Option<Comparison>,
    corroboration: Comparison,
    /// The reach pass B's first page was read from.
    reach: Option<Reach>,
    /// Source versions research saved for the claim, which its walk reads.
    researched: Vec<String>,
    /// The source versions the claim's section cites.
    cited: Vec<String>,
    /// The receipt key of pass B's first page.
    first_receipt: String,
    /// What the first page's selection said about its list, for the log.
    note: Option<PageNote>,
}

impl Compared {
    /// The claim's finding once every comparison is done, and whether all
    /// of them were restored.
    fn decided(&self) -> Option<(Finding, bool)> {
        let (b, b_reused) = self.corroboration.done()?;
        let a = match &self.attribution {
            Some(side) => Some(side.done()?),
            None => None,
        };
        let reused = b_reused && a.is_none_or(|(_, reused)| reused);
        Some((
            combine(
                a.map(|(finding, _)| finding.clone()),
                b.clone(),
                (self.reach, 1),
                &self.cited,
            ),
            reused,
        ))
    }
}

enum Prepared {
    /// A finished decision: `true` when it is a retained final decision that
    /// already walked its pages, `false` for one decided on the spot.
    Complete(CheckedClaim, bool),
    Compared(Box<Compared>),
}

#[allow(clippy::too_many_arguments)]
async fn prepare(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    sources: &[SourceBinding],
    executions: &[execution::Observation],
    retained: &ClaimChecks,
    ordinal: usize,
    located_claim: (usize, Claim),
    (citations, paging, budget): (&Citations, Paging, Option<usize>),
) -> Result<Prepared> {
    let (unit, claim) = located_claim;
    // A quotation found in the source it names is settled by the match.
    if let Some(finding) = quote_claims::settle(references, citations, unit, &claim)? {
        let key = ClaimChecks::key(unit, &claim, &finding.evidence);
        return Ok(Prepared::Complete((ordinal, key, finding, false), true));
    }
    // Resolved by evidence the user brought: that check is the receipt.
    if let Some(finding) = retained.settled_finding(unit, &claim) {
        let key = ClaimChecks::key(unit, &claim, &finding.evidence);
        return Ok(Prepared::Complete((ordinal, key, finding, true), true));
    }
    let checked = as_checked(&claim, citations, references);
    // Pass A: the section's passed example runs and the passages it cites,
    // while a check of them alone fits the window.
    let mut attribution = example_runs(executions, unit);
    let cited = take_cited(
        llm,
        &checked.statement,
        &mut attribution,
        citations.of(unit),
    );
    canonicalize_evidence(&mut attribution);
    let selected = evidence_selection::select(
        llm,
        (&claim, &checked.statement, unit),
        references,
        sources,
        retained,
        (citations, &cited, paging, budget),
    )
    .await?;
    let a_key = (!attribution.is_empty()).then(|| ClaimChecks::key(unit, &checked, &attribution));
    let b_key = ClaimChecks::key(unit, &checked, &selected.evidence);
    let key = decision_key(a_key.as_deref(), &b_key);
    if let Some(finding) = retained.get(&key, unit, &claim, &[]) {
        return Ok(Prepared::Complete((ordinal, key, finding, true), true));
    }
    let side = |key: String, evidence: Vec<EvidencePassage>, pass: Pass| PendingClaim {
        ordinal,
        receipt_key: claim_receipt_key(llm, &key),
        key,
        unit,
        claim: claim.clone(),
        checked: checked.statement.clone(),
        evidence,
        pass,
    };
    let first_receipt = claim_receipt_key(llm, &b_key);
    let attribution = match a_key {
        Some(a_key) => {
            Some(compare(llm, retained, side(a_key, attribution, Pass::Attribution)).await?)
        }
        None => None,
    };
    let corroboration = compare(
        llm,
        retained,
        side(b_key, selected.evidence, Pass::Corroboration),
    )
    .await?;
    let mut cited: Vec<String> = cited.into_iter().map(|passage| passage.source_id).collect();
    cited.dedup();
    Ok(Prepared::Compared(Box::new(Compared {
        ordinal,
        unit,
        claim,
        checked: checked.statement,
        key,
        attribution,
        corroboration,
        reach: selected.reach,
        researched: selected.researched,
        cited,
        first_receipt,
        note: selected.note,
    })))
}

/// One comparison: restored from its receipt (or a receipt saved under an
/// earlier key for the same claim and evidence), decided on the spot when it
/// has nothing to compare, or left for the model.
async fn compare(
    llm: &dyn LLMPort,
    retained: &ClaimChecks,
    pending: PendingClaim,
) -> Result<Comparison> {
    let (unit, claim, evidence) = (pending.unit, &pending.claim, &pending.evidence);
    if let Some(finding) =
        crate::features::learning::lesson_drafts::checkpoint(&pending.receipt_key)
            .await?
            .and_then(|value| serde_json::from_value::<ClaimReceipt>(value).ok())
            .and_then(|receipt| receipt.finding(unit, claim, evidence))
    {
        return Ok(Comparison::Done(finding, true));
    }
    if let Some(receipt) = claim_reuse::legacy_receipt(llm, retained, unit, claim, evidence).await?
    {
        let saved = serde_json::to_value(&receipt)?;
        if let Some(finding) = receipt.finding(unit, claim, evidence) {
            crate::features::learning::lesson_drafts::record_checkpoint(
                &pending.receipt_key,
                saved,
            )
            .await?;
            return Ok(Comparison::Done(finding, true));
        }
    }
    if evidence.is_empty() {
        let judgment = ClaimJudgment::Judged(JudgeOutcome {
            verdict: ClaimVerdict::Unsupported,
            reason: Some("No relevant saved passages were found for this claim.".into()),
            quote: None,
            confidence: None,
        });
        let (_, _, finding, _) = finish(pending, judgment, None).await?;
        return Ok(Comparison::Done(finding, false));
    }
    Ok(Comparison::Pending(pending))
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
    crate::features::learning::lesson_progress::phase(
        crate::features::learning::lesson_progress::Phase::Evidence,
        format!("Looking for reusable checks across {total} lesson claims"),
    );
    crate::features::learning::lesson_progress::begin_checks(total);
    let mut answers = *retained
        .answers
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let first_budget = budget(llm, answers);
    // Restore ALL reusable results before the first model request. Otherwise a
    // slow early comparison hides saved work later in the inventory, making a
    // resumed or expanded-evidence pass look like a complete restart. Collect
    // only pending comparisons into model requests.
    let mut results = Vec::new();
    let mut compared: BTreeMap<usize, Compared> = BTreeMap::new();
    let mut reused = 0;
    let mut decided = HashSet::new();
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
            .map(|(ordinal, (unit, claim))| {
                prepare(
                    llm,
                    references,
                    &sources,
                    executions,
                    retained,
                    ordinal,
                    (unit, claim),
                    (citations, paging, first_budget),
                )
            })
            .buffered(claims)
            .try_collect()
            .await?;
        for item in prepared {
            match item {
                Prepared::Complete(result, final_decision) => {
                    reused += usize::from(result.3);
                    if final_decision {
                        decided.insert(result.0);
                    }
                    crate::features::learning::lesson_progress::checked(
                        result.3,
                        result.2.verdict != ClaimVerdict::Supported,
                    );
                    results.push(result);
                }
                Prepared::Compared(claim) => {
                    compared.insert(claim.ordinal, *claim);
                }
            }
        }
        crate::features::learning::lesson_progress::stage(format!(
            "Compared current evidence for {} of {total} claims · {reused} saved checks reused so far",
            results.len() + compared.len()
        ));
    }
    // Round 1: every compared claim's pass A page and pass B's first page.
    let mut pending = Vec::new();
    for claim in compared.values_mut() {
        let ordinal = claim.ordinal;
        let note = claim.note.take();
        let sides = claim
            .attribution
            .iter_mut()
            .map(|side| (Pass::Attribution, side))
            .chain(std::iter::once((
                Pass::Corroboration,
                &mut claim.corroboration,
            )));
        for (pass, side) in sides {
            let (page, judged) = match side {
                Comparison::Pending(item) => (item.evidence.clone(), true),
                Comparison::Done(finding, _) => (finding.evidence.clone(), false),
                Comparison::Sent => continue,
            };
            let note = match pass {
                Pass::Attribution => None,
                Pass::Corroboration => note.as_ref(),
            };
            log_page(llm, (ordinal, 1), pass, &page, note, judged).await;
            if judged {
                if let Comparison::Pending(item) = std::mem::replace(side, Comparison::Sent) {
                    pending.push(item);
                }
            }
        }
    }
    let mut checked_now: Vec<usize> = pending.iter().map(|item| item.ordinal).collect();
    checked_now.dedup();
    let model_checks = checked_now.len();
    // A claim whose comparisons were all restored or decided on the spot is
    // finished before any request.
    let finished: Vec<usize> = compared
        .values()
        .filter(|claim| claim.decided().is_some())
        .map(|claim| claim.ordinal)
        .collect();
    for ordinal in &finished {
        if let Some((finding, all_reused)) = compared.get(ordinal).and_then(Compared::decided) {
            reused += usize::from(all_reused);
            crate::features::learning::lesson_progress::checked(
                all_reused,
                finding.verdict != ClaimVerdict::Supported,
            );
        }
    }
    let resolved = results.len() + finished.len();
    crate::features::learning::lesson_progress::plan_model_checks(model_checks);
    tracing::info!(
        total,
        reused,
        model_checks,
        locally_resolved = resolved - reused,
        page_budget = ?first_budget,
        "Lesson claim check plan ready"
    );
    let attributing = pending
        .iter()
        .filter(|item| item.pass == Pass::Attribution)
        .count();
    crate::features::learning::lesson_progress::stage(format!(
        "{reused} saved checks reused · Checking {attributing} claims against their citations · Corroborating {} claims across the course, page 1",
        pending.len() - attributing
    ));
    // A request already carries several complete evidence sets. Run them one
    // at a time rather than compete prompt prefills on the same model.
    let mut groups = futures::stream::iter(pack(llm, pending, &labels))
        .map(|request| group(llm, request, Check::First, &labels))
        .buffer_unordered(1);
    let mut done = 0;
    while let Some(batch) = groups.next().await {
        let (batch, answered) = batch?;
        answers.add(answered);
        for (pass, (ordinal, _, finding, _)) in batch {
            let Some(claim) = compared.get_mut(&ordinal) else {
                continue;
            };
            let side = match pass {
                Pass::Attribution => claim.attribution.as_mut(),
                Pass::Corroboration => Some(&mut claim.corroboration),
            };
            if let Some(side) = side {
                *side = Comparison::Done(finding, false);
            }
            if let Some((finding, _)) = claim.decided() {
                done += 1;
                crate::features::learning::lesson_progress::checked(
                    false,
                    finding.verdict != ClaimVerdict::Supported,
                );
            }
        }
        crate::features::learning::lesson_progress::stage(format!(
            "Completed {done} of {model_checks} model checks · {reused} saved checks reused · {total} lesson claims"
        ));
    }
    // Every compared claim's decision, and the walks of those still
    // unsupported.
    let mut walks = Vec::new();
    let mut attributions: HashMap<usize, (Option<Finding>, Vec<String>)> = HashMap::new();
    for claim in compared.into_values() {
        let Some((finding, all_reused)) = claim.decided() else {
            return Err(invalid(
                "A claim check finished without its finding; no lesson was published.",
            ));
        };
        if paging.depth == Depth::ListEnd && finding.verdict == ClaimVerdict::Unsupported {
            let shown: Vec<EvidencePassage> = claim
                .attribution
                .iter()
                .chain(std::iter::once(&claim.corroboration))
                .filter_map(Comparison::done)
                .flat_map(|(finding, _)| finding.evidence.clone())
                .collect();
            let fresh = budget(llm, answers);
            walks.push(Walk::new(
                (claim.ordinal, claim.unit),
                claim.claim.clone(),
                claim.checked.clone(),
                shown,
                (paging.reaches, claim.reach),
                claim.researched.clone(),
                walk_budget(&claim.first_receipt, fresh).await?,
                1,
            ));
            attributions.insert(
                claim.ordinal,
                (
                    claim
                        .attribution
                        .as_ref()
                        .and_then(Comparison::done)
                        .map(|(finding, _)| finding.clone()),
                    claim.cited.clone(),
                ),
            );
        }
        results.push((claim.ordinal, claim.key, finding, all_reused));
    }
    if !walks.is_empty() {
        let found = deepen(llm, references, walks, citations, &labels, &mut answers).await?;
        for (ordinal, (finding, reach, page)) in found {
            let Some((attribution, cited)) = attributions.remove(&ordinal) else {
                continue;
            };
            if let Some(result) = results.iter_mut().find(|result| result.0 == ordinal) {
                result.2 = combine(attribution, finding, (Some(reach), page), &cited);
            }
        }
    }
    *retained
        .answers
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = answers;
    results.sort_by_key(|(ordinal, _, _, _)| *ordinal);
    // The gate call (§18.6): every supported verdict's support is weighed
    // before the claim ledger sees it; no further model check is made.
    provenance_gate::weigh(llm, &mut results, &decided).await?;
    crate::features::learning::lesson_progress::sent_back(
        results
            .iter()
            .filter(|(_, _, finding, _)| finding.scrutinised().is_some())
            .count(),
    );
    Ok(results)
}
