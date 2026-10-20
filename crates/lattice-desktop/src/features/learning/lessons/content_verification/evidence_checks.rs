//! Retrieve and restore individual comparisons, then batch only unfinished ones.
//! Every decision retains its own evidence binding and durable receipt.
//!
//! Evidence is paged. A claim is first checked against its top-ranked
//! passages, an equal share of its batch call's room. A claim that page
//! leaves unsupported is checked again against its next page, passages it has
//! not been shown, batched with the other claims still looking; the walk ends
//! at the first supported or contradicted verdict, or when the next page has
//! no passage mentioning the claim's subject (vector neighbours alone do not
//! continue it). Deepening rounds continue only while the last round settled
//! at least one claim as supported or contradicted. Each page's check has its
//! own receipt.
//!
//! Every check reports what its answer cost in output tokens; the job's
//! mean, with the backend's measured pace, gives a page budget (see
//! `page_budget`), which each deepening round logs. Pages are not sized by
//! it yet.
//!
//! Progress counts each claim once. A deeper page's check adds no check; it
//! moves its claim out of the unresolved count when the page supports it.
use super::*;
use lattice_core::application::services::claim_verification::{LocatedClaim, ANSWER_ROOM_TOKENS};
use std::collections::BTreeMap;

mod page_budget;
use page_budget::{page_budget, AnswerSize};

const BATCH_SIZE: usize = 8;

/// Each passage's label, by source version (`provenance_gate::passage_labels`).
type Labels = HashMap<String, String>;

/// What a finished comparison reports to lesson progress.
#[derive(Clone, Copy)]
enum Check {
    /// The claim's first check in this pass.
    First,
    /// An unsupported claim's check against a deeper evidence page.
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

enum PreparedClaim {
    /// A finished decision: `true` when it is a retained final decision that
    /// already walked its pages, `false` for a first-page result.
    Complete(CheckedClaim, bool),
    Pending(PendingClaim),
}

async fn prepare(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    sources: &[SourceBinding],
    executions: &[execution::Observation],
    retained: &ClaimChecks,
    ordinal: usize,
    located_claim: (usize, Claim),
    sharing: usize,
    citations: &Citations,
) -> Result<PreparedClaim> {
    let (unit, claim) = located_claim;
    // A quotation found in the source it names is settled by the match.
    if let Some(finding) = quote_claims::settle(references, citations, unit, &claim)? {
        let key = ClaimChecks::key(unit, &claim, &finding.evidence);
        return Ok(PreparedClaim::Complete(
            (ordinal, key, finding, false),
            true,
        ));
    }
    // Resolved by evidence the user brought: that check is the receipt.
    if let Some(finding) = retained.settled_finding(unit, &claim) {
        let key = ClaimChecks::key(unit, &claim, &finding.evidence);
        return Ok(PreparedClaim::Complete((ordinal, key, finding, true), true));
    }
    let evidence = evidence_selection::select(
        llm, &claim, unit, references, sources, executions, retained, sharing, citations,
    )
    .await?;
    let checked = as_checked(&claim, citations, references).statement;
    let key = ClaimChecks::key(
        unit,
        &Claim {
            statement: checked.clone(),
            quote: claim.quote.clone(),
        },
        &evidence,
    );
    if let Some(finding) = retained.get(&key, unit, &claim, &evidence) {
        return Ok(PreparedClaim::Complete((ordinal, key, finding, true), true));
    }
    let receipt_key = claim_receipt_key(llm, &key);
    if let Some(finding) = crate::features::learning::lesson_drafts::checkpoint(&receipt_key)
        .await?
        .and_then(|value| serde_json::from_value::<ClaimReceipt>(value).ok())
        .and_then(|receipt| receipt.finding(unit, &claim, &evidence))
    {
        return Ok(PreparedClaim::Complete(
            (ordinal, key, finding, true),
            false,
        ));
    }
    if let Some(receipt) =
        claim_reuse::legacy_receipt(llm, retained, unit, &claim, &evidence).await?
    {
        let saved = serde_json::to_value(&receipt)?;
        if let Some(finding) = receipt.finding(unit, &claim, &evidence) {
            crate::features::learning::lesson_drafts::record_checkpoint(&receipt_key, saved)
                .await?;
            return Ok(PreparedClaim::Complete(
                (ordinal, key, finding, true),
                false,
            ));
        }
    }
    let text = evidence
        .iter()
        .map(|p| format!("[{}]\n{}", p.source_id, p.text))
        .collect::<Vec<_>>()
        .join("\n\n");
    let local = if text.is_empty() {
        Some(ClaimJudgment::Judged(JudgeOutcome {
            verdict: ClaimVerdict::Unsupported,
            reason: Some("No relevant saved passages were found for this claim.".into()),
            quote: None,
            confidence: None,
        }))
    } else if llm.count_tokens(&text) + llm.count_tokens(&claim.statement) + ANSWER_ROOM_TOKENS
        > llm.max_context_tokens()
    {
        Some(ClaimJudgment::Unusable)
    } else {
        None
    };
    let pending = PendingClaim {
        ordinal,
        key,
        receipt_key,
        unit,
        claim,
        checked,
        evidence,
    };
    if let Some(judgment) = local {
        return Ok(PreparedClaim::Complete(
            finish(pending, judgment, None).await?,
            false,
        ));
    }
    Ok(PreparedClaim::Pending(pending))
}

/// Finish `pending` with the judge's `judgment`. `shown` holds the positions
/// in the page of the passages the judge was shown when it was not shown them
/// all; the pending claim's evidence is already cut to those, and its receipt
/// records them, so a restored decision names the same passages.
async fn finish(
    pending: PendingClaim,
    judgment: ClaimJudgment,
    shown: Option<Vec<usize>>,
) -> Result<CheckedClaim> {
    let (verdict, reason, supporting_quote) = match judgment {
        ClaimJudgment::Failed(error) => return Err(error),
        ClaimJudgment::Judged(outcome) => (
            outcome.verdict,
            outcome.reason.unwrap_or_default(),
            outcome.quote,
        ),
        // Every unverified finding is deterministic: its reason is the cause.
        ClaimJudgment::Unusable => (
            ClaimVerdict::Unverified,
            "The checker's answer was unusable after its correction and its retry alone.".into(),
            None,
        ),
    };
    let verdict = if verdict == ClaimVerdict::Supported
        && supporting_quote
            .as_ref()
            .is_none_or(|quote| !pending.evidence.iter().any(|p| p.text.contains(quote)))
    {
        ClaimVerdict::Unverified
    } else {
        verdict
    };
    let finding = Finding {
        unit: pending.unit,
        quote: pending.claim.quote,
        statement: pending.claim.statement,
        verdict,
        reason,
        evidence: pending.evidence,
        supporting_quote,
        resolution: None,
        trust: None,
    };
    if finding.verdict != ClaimVerdict::Unverified {
        crate::features::learning::lesson_drafts::record_checkpoint(
            &pending.receipt_key,
            serde_json::to_value(ClaimReceipt {
                verdict: finding.verdict,
                reason: finding.reason.clone(),
                supporting_quote: finding.supporting_quote.clone(),
                interpretation_policy: Some(
                    lattice_core::application::services::claim_verification::STRICT_INTERPRETATION_POLICY
                        .into(),
                ),
                shown,
            })?,
        )
        .await?;
    }
    Ok((pending.ordinal, pending.key, finding, false))
}

/// The claim as the checker reads it, each passage under its source's label
/// (§18.6 of the provenance design): the label names the source so the
/// passage is never read as another one; the quotation must still come from
/// the passage's own text.
fn target(pending: &PendingClaim, labels: &Labels) -> LocatedClaim {
    LocatedClaim {
        claim: pending.checked.clone(),
        passages: pending
            .evidence
            .iter()
            .map(|p| match labels.get(&p.source_id) {
                Some(label) => format!("{label}\n{}", p.text),
                None => p.text.clone(),
            })
            .collect(),
    }
}

/// The checker attaches the text of the passage it cites, label included;
/// the finding binds to the passage's own text.
fn unlabelled(judgment: ClaimJudgment, pending: &PendingClaim, labels: &Labels) -> ClaimJudgment {
    match judgment {
        ClaimJudgment::Judged(mut outcome) => {
            if let Some(passage) = outcome.quote.as_ref().and_then(|quote| {
                pending.evidence.iter().find(|p| {
                    labels
                        .get(&p.source_id)
                        .is_some_and(|label| *quote == format!("{label}\n{}", p.text))
                })
            }) {
                outcome.quote = Some(passage.text.clone());
            }
            ClaimJudgment::Judged(outcome)
        }
        other => other,
    }
}

/// One claim checked alone. A page too long for the judge's window loses
/// its least needed passages (see [`ranking`]); the finding and its receipt
/// name only the passages the judge was shown, so one cut for size is still
/// unshown and a deeper page can carry it. The check comes with what its
/// answer cost.
async fn individual(
    llm: &dyn LLMPort,
    mut pending: PendingClaim,
    labels: &Labels,
) -> Result<(CheckedClaim, AnswerSize)> {
    let input = target(&pending, labels);
    let judged = judge_for_publication(
        llm,
        &input.claim,
        &input.passages,
        &ranking(&pending.evidence),
    )
    .await;
    let shown = judged.shown;
    let cut = shown.len() < pending.evidence.len();
    pending.evidence = shown_of(&pending.evidence, &shown);
    let judgment = unlabelled(judged.judgment, &pending, labels);
    let answer = AnswerSize::of(judged.output_tokens, 1);
    Ok((
        finish(pending, judgment, cut.then_some(shown)).await?,
        answer,
    ))
}

fn completed(results: &mut Vec<CheckedClaim>, result: CheckedClaim, check: Check) {
    // Individual fallbacks and smaller groups can take minutes. Report each
    // finished comparison immediately, even while its siblings are pending.
    let supported = result.2.verdict == ClaimVerdict::Supported;
    match check {
        Check::First => crate::features::learning::lesson_progress::checked(result.3, !supported),
        Check::Deeper => crate::features::learning::lesson_progress::rechecked(supported),
    }
    results.push(result);
}

/// Check `pending`'s claims, with what the judge's answers to them cost.
async fn group(
    llm: &dyn LLMPort,
    pending: Vec<PendingClaim>,
    check: Check,
    labels: &Labels,
) -> Result<(Vec<CheckedClaim>, AnswerSize)> {
    let mut results = Vec::new();
    let mut answers = AnswerSize::default();
    let checker = ClaimChecker::new(
        llm,
        SamplingOverride::deterministic(),
        512,
        CheckPolicy::Strict,
    );
    let mut groups = vec![pending];
    while let Some(mut pending) = groups.pop() {
        if pending.is_empty() {
            continue;
        }
        let targets: Vec<_> = pending.iter().map(|item| target(item, labels)).collect();
        if pending.len() == 1 {
            for item in pending {
                let (result, answer) = individual(llm, item, labels).await?;
                answers.add(answer);
                completed(&mut results, result, check);
            }
            continue;
        }
        if !checker.batch_fits(&targets) {
            let rest = pending.split_off(pending.len() / 2);
            groups.push(rest);
            groups.push(pending);
            continue;
        }
        // This is a transport plan, never an approval. Bind it to the exact
        // comparisons so restarting cannot repeat a group known to fail.
        let split_key = format!(
            "claim-batch-split-v1:{}",
            digest(&serde_json::to_string(
                &pending
                    .iter()
                    .map(|item| &item.receipt_key)
                    .collect::<Vec<_>>(),
            )?)
        );
        if crate::features::learning::lesson_drafts::checkpoint(&split_key).await?
            == Some(json!(true))
        {
            let rest = pending.split_off(pending.len() / 2);
            groups.push(rest);
            groups.push(pending);
            continue;
        }
        crate::features::learning::lesson_progress::stage(format!(
            "Comparing a group of {} remaining claims with their assigned evidence",
            pending.len()
        ));
        let _model_call = crate::features::learning::lesson_progress::model_call();
        let judgments = match judge_batch_for_publication(llm, &targets).await {
            Ok(answered) => {
                answers.add(AnswerSize::of(answered.output_tokens, targets.len()));
                answered.judgments
            }
            // A group the model could not finish, or one the backend counted
            // past the context window, is checked in halves on the same
            // evidence, so every receipt is unchanged.
            Err(
                error @ (AppError::ServiceNotAvailable(_) | AppError::PromptExceedsWindow { .. }),
            ) => {
                crate::features::learning::lesson_drafts::record_checkpoint(
                    &split_key,
                    json!(true),
                )
                .await?;
                crate::features::learning::lesson_progress::stage(
                    if matches!(error, AppError::PromptExceedsWindow { .. }) {
                        format!(
                            "A group of {} checks was too long for the model's context window; checking them in smaller groups with the same evidence",
                            pending.len()
                        )
                    } else {
                        format!(
                            "The model could not finish a group of {} checks; retrying in smaller groups with the same evidence",
                            pending.len()
                        )
                    },
                );
                let rest = pending.split_off(pending.len() / 2);
                groups.push(rest);
                groups.push(pending);
                continue;
            }
            Err(error) => return Err(error),
        };
        if judgments.len() != pending.len() {
            return Err(invalid("Incomplete claim batch; no lesson was published."));
        }
        let mut unresolved = Vec::new();
        for (item, judgment) in pending.into_iter().zip(judgments) {
            if matches!(judgment, ClaimJudgment::Unusable) {
                unresolved.push(item);
            } else {
                // Save completed siblings before a malformed response needs a
                // slower individual recheck. An outage must not discard them.
                let judgment = unlabelled(judgment, &item, labels);
                completed(&mut results, finish(item, judgment, None).await?, check);
            }
        }
        for item in unresolved {
            let (result, answer) = individual(llm, item, labels).await?;
            answers.add(answer);
            completed(&mut results, result, check);
        }
    }
    Ok((results, answers))
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

/// Check every claim of `inventory`, each first against the passages its
/// own section cites (`citations`, by unit) and then retrieved ones.
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
    crate::features::learning::lesson_progress::phase(
        crate::features::learning::lesson_progress::Phase::Evidence,
        format!("Looking for reusable checks across {total} lesson claims"),
    );
    crate::features::learning::lesson_progress::begin_checks(total);
    // Restore ALL reusable results before the first model request. Otherwise a
    // slow early comparison hides saved work later in the inventory, making a
    // resumed or expanded-evidence pass look like a complete restart. Collect
    // only pending claims into model batches, including across inventory gaps.
    let mut results = Vec::new();
    let mut pending = Vec::new();
    let mut reused = 0;
    // Claims share a batch call's room equally; one checked alone has it all.
    let sharing = BATCH_SIZE.min(total);
    let mut decided = HashSet::new();
    let mut entries = futures::stream::iter(entries.into_iter().enumerate()).chunks(BATCH_SIZE);
    while let Some(entries) = entries.next().await {
        // Drain the reads before any subsequent model work/checkpoint writes.
        // Unpolled prefetches can otherwise hold scarce database pool slots.
        let prepared: Vec<_> = futures::stream::iter(entries)
            .map(|(ordinal, (unit, claim))| {
                prepare(
                    llm,
                    references,
                    &sources,
                    executions,
                    retained,
                    ordinal,
                    (unit, claim),
                    sharing,
                    citations,
                )
            })
            .buffered(BATCH_SIZE)
            .try_collect()
            .await?;
        for item in prepared {
            match item {
                PreparedClaim::Complete(result, final_decision) => {
                    reused += usize::from(result.3);
                    if final_decision {
                        decided.insert(result.0);
                    }
                    completed(&mut results, result, Check::First);
                }
                PreparedClaim::Pending(item) => pending.push(item),
            }
        }
        crate::features::learning::lesson_progress::stage(format!(
            "Compared current evidence for {} of {total} claims · {reused} saved checks reused so far",
            results.len() + pending.len()
        ));
    }
    let model_checks = pending.len();
    let resolved = results.len();
    crate::features::learning::lesson_progress::plan_model_checks(model_checks);
    tracing::info!(
        total,
        reused,
        model_checks,
        locally_resolved = resolved - reused,
        "Lesson claim check plan ready"
    );
    crate::features::learning::lesson_progress::stage(format!(
        "{reused} saved checks reused · {model_checks} of {total} claims need model review"
    ));
    // A group already carries several complete evidence sets. Run them one at
    // a time rather than compete large prompt prefills on the same model.
    let mut answers = AnswerSize::default();
    let mut groups = futures::stream::iter(pending)
        .chunks(BATCH_SIZE)
        .map(|pending| group(llm, pending, Check::First, &labels))
        .buffer_unordered(1);
    while let Some(batch) = groups.next().await {
        let (batch, answered) = batch?;
        answers.add(answered);
        results.extend(batch);
        crate::features::learning::lesson_progress::stage(format!(
            "Completed {} of {model_checks} model checks · {reused} saved checks reused · {total} lesson claims",
            results.len() - resolved
        ));
    }
    deepen(
        llm,
        references,
        &mut results,
        &decided,
        citations,
        &labels,
        answers,
    )
    .await?;
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

/// A claim still looking for evidence: what it has been shown so far.
struct Walk {
    unit: usize,
    claim: Claim,
    shown: Vec<EvidencePassage>,
}

/// Check every first-page result that is unsupported against its next pages
/// until a page decides it or no page with a matching content term is left.
/// A deciding page's finding replaces the first page's; the first page's key
/// still names the decision, so a later pass on the same evidence reuses it.
/// `answers` is what the job's checks so far cost per claim; each round's
/// checks add to it, and each round logs the page budget it gives.
async fn deepen(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    results: &mut [CheckedClaim],
    decided: &HashSet<usize>,
    citations: &Citations,
    labels: &Labels,
    mut answers: AnswerSize,
) -> Result<()> {
    let mut walks: BTreeMap<usize, Walk> = results
        .iter()
        .filter(|(ordinal, _, finding, _)| {
            !decided.contains(ordinal) && finding.verdict == ClaimVerdict::Unsupported
        })
        .map(|(ordinal, _, finding, _)| {
            (
                *ordinal,
                Walk {
                    unit: finding.unit,
                    claim: Claim {
                        quote: finding.quote.clone(),
                        statement: finding.statement.clone(),
                    },
                    shown: finding
                        .evidence
                        .iter()
                        .filter(|passage| passage.retrieval_kind != "execution")
                        .cloned()
                        .collect(),
                },
            )
        })
        .collect();
    let mut round = 0;
    while !walks.is_empty() {
        round += 1;
        // Measured, not yet applied: pages are still sized by their share.
        let budget = page_budget(llm.pace(), answers.per_claim());
        let sharing = BATCH_SIZE.min(walks.len());
        let mut pending = Vec::new();
        let mut outcomes = Vec::new();
        for (ordinal, walk) in std::mem::take(&mut walks) {
            let share = claim_share(llm, &walk.claim.statement, sharing);
            // Cited passages of the section that did not fit an earlier page
            // lead this one; retrieved passages not yet shown fill the rest.
            let mut page: Vec<EvidencePassage> = Vec::new();
            for passage in citations.of(walk.unit) {
                if walk
                    .shown
                    .iter()
                    .chain(&page)
                    .any(|seen| overlaps(seen, passage))
                {
                    continue;
                }
                if !within_share(llm, share, page.iter().chain(std::iter::once(passage))) {
                    break;
                }
                page.push(passage.clone());
            }
            let cited_left = !page.is_empty();
            let shown: Vec<_> = walk.shown.iter().chain(&page).cloned().collect();
            let retrieved = lesson_sources_first(
                llm,
                references,
                &walk.claim.statement,
                share,
                &page,
                &shown,
                citations,
            )
            .await?;
            page.extend(retrieved);
            // A page that only shares the subject's common words, or only
            // vector neighbours, ends the walk; the section's own citations
            // never do.
            let subject = references.subject_of(&walk.claim.statement).await?;
            if !cited_left
                && !page
                    .iter()
                    .any(|passage| subject.mentioned_in(&passage.text))
            {
                continue;
            }
            canonicalize_evidence(&mut page);
            let checked = as_checked(&walk.claim, citations, references);
            let key = ClaimChecks::key(walk.unit, &checked, &page);
            let receipt_key = claim_receipt_key(llm, &key);
            if let Some(finding) =
                crate::features::learning::lesson_drafts::checkpoint(&receipt_key)
                    .await?
                    .and_then(|value| serde_json::from_value::<ClaimReceipt>(value).ok())
                    .and_then(|receipt| receipt.finding(walk.unit, &walk.claim, &page))
            {
                outcomes.push((ordinal, finding));
            } else {
                pending.push(PendingClaim {
                    ordinal,
                    key,
                    receipt_key,
                    unit: walk.unit,
                    claim: walk.claim.clone(),
                    checked: checked.statement,
                    evidence: page,
                });
            }
            // The page joins what the claim was shown once its check says
            // what the judge saw: a passage cut for size was not shown.
            walks.insert(ordinal, walk);
        }
        if !pending.is_empty() || !outcomes.is_empty() {
            crate::features::learning::lesson_progress::deeper_page(
                round + 1,
                pending.len() + outcomes.len(),
            );
            // A saved check of this page is finished as soon as it is restored.
            for (_, finding) in &outcomes {
                crate::features::learning::lesson_progress::rechecked(
                    finding.verdict == ClaimVerdict::Supported,
                );
            }
        }
        if !pending.is_empty() {
            crate::features::learning::lesson_progress::stage(format!(
                "Checking {} unsupported claims against their next page of evidence (page {})",
                pending.len(),
                round + 1
            ));
            let mut batches = futures::stream::iter(pending)
                .chunks(BATCH_SIZE)
                .map(|pending| group(llm, pending, Check::Deeper, labels))
                .buffer_unordered(1);
            while let Some(batch) = batches.next().await {
                let (batch, answered) = batch?;
                answers.add(answered);
                outcomes.extend(
                    batch
                        .into_iter()
                        .map(|(ordinal, _, finding, _)| (ordinal, finding)),
                );
            }
        }
        let mut settled = 0;
        for (ordinal, finding) in outcomes {
            if finding.verdict == ClaimVerdict::Unsupported {
                if let Some(walk) = walks.get_mut(&ordinal) {
                    walk.shown.extend(finding.evidence);
                }
                continue;
            }
            settled += usize::from(matches!(
                finding.verdict,
                ClaimVerdict::Supported | ClaimVerdict::Contradicted
            ));
            walks.remove(&ordinal);
            if let Some(result) = results.iter_mut().find(|result| result.0 == ordinal) {
                result.2 = finding;
            }
        }
        tracing::info!(
            round,
            settled,
            looking = walks.len(),
            page_budget = ?budget,
            "Deepened the evidence of unsupported claims"
        );
        // Another round runs only while the last one settled a claim.
        if settled == 0 {
            break;
        }
    }
    crate::features::learning::lesson_progress::deeper_done();
    Ok(())
}
