//! Retrieve and restore individual comparisons, then batch only unfinished ones.
//! Every decision retains its own evidence binding and durable receipt.
//!
//! Evidence is paged by relevance, not by room (claim evidence pages design).
//! A claim's first page holds the passages its own section cites and, from
//! each source the lesson cites, the best passage on the claim's subject; a
//! ring with nothing on the subject gives way to the next. A retrieved
//! passage joins a page only above its ranking's cut, where that ranking's
//! scores drop most clearly, and never when it does not mention the claim's
//! subject. A claim that page leaves unsupported walks outward (see
//! `deepening`): the lesson's remaining sources, the course's other sources
//! one passage each with a counter-evidence page beside them, then every
//! source's next-best passage. The walk ends at the first supported or
//! contradicted verdict, or when nothing on the subject is left. Rings 1 and 2
//! always run to the end; the deeper pages continue only while the last round
//! settled a claim. Each page's check has its own receipt.
//!
//! The window bounds a request and never sizes a page. Claims of one section
//! are packed into a request while the batch estimate holds them all; a page
//! too long for its claim checked alone loses its least needed passages, and
//! a request the backend counts past the window is checked in halves. The
//! judge reads a page with its most needed passages at the edges; the page's
//! canonical order still keys the comparison.
//!
//! Every check reports what its answer cost in output tokens; the job's
//! mean, with the backend's measured pace, gives the deeper pages' budget
//! (see `page_budget`), which each deepening round logs.
//!
//! Progress counts each claim once. A deeper page's check adds no check; it
//! moves its claim out of the unresolved count when the page supports it.
use super::*;
use lattice_core::application::services::claim_verification::LocatedClaim;

mod deepening;
mod page_budget;
#[cfg(test)]
pub(in crate::features::learning) use deepening::walk_pages;
pub(in crate::features::learning) use page_budget::{page_budget, AnswerSize};

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
        llm, &claim, unit, references, sources, executions, retained, citations,
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
    // A page too long for the window is still checked: its least needed
    // passages give way (`individual`).
    let local = evidence.is_empty().then(|| {
        ClaimJudgment::Judged(JudgeOutcome {
            verdict: ClaimVerdict::Unsupported,
            reason: Some("No relevant saved passages were found for this claim.".into()),
            quote: None,
            confidence: None,
        })
    });
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

/// Each passage of the claim's page under its source's label (§18.6 of the
/// provenance design), in page order: the label names the source so the
/// passage is never read as another one; the quotation must still come from
/// the passage's own text.
fn labelled(pending: &PendingClaim, labels: &Labels) -> Vec<String> {
    pending
        .evidence
        .iter()
        .map(|p| match labels.get(&p.source_id) {
            Some(label) => format!("{label}\n{}", p.text),
            None => p.text.clone(),
        })
        .collect()
}

/// The claim as the checker reads it: its labelled passages in reading
/// order (`reading_order`), the most needed at the edges. A batch's passage
/// bank keeps them in this order, so a passage two claims share sits at the
/// first one's edge.
fn target(pending: &PendingClaim, labels: &Labels) -> LocatedClaim {
    let passages = labelled(pending, labels);
    LocatedClaim {
        claim: pending.checked.clone(),
        passages: reading_order(&pending.evidence)
            .into_iter()
            .filter_map(|position| passages.get(position).cloned())
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
/// unshown and a deeper page can carry it.
async fn individual(
    llm: &dyn LLMPort,
    mut pending: PendingClaim,
    labels: &Labels,
) -> Result<(CheckedClaim, AnswerSize)> {
    let judged = judge_for_publication(
        llm,
        &pending.checked,
        &labelled(&pending, labels),
        &ranking(&pending.evidence),
        &reading_order(&pending.evidence),
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
            "Comparing {} claims of section {} with their evidence",
            pending.len(),
            pending.first().map_or(0, |item| item.unit) + 1
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

/// The model requests for `pending` (claim evidence pages §5): claims of
/// one section at a time, in section and claim order, a request taking
/// claims while the batch estimate says the window holds them all with their
/// answers. A section's claims share its cited passages, which the bank
/// holds once. The first claim that does not fit starts the next request;
/// one whose page does not fit a request alone is checked alone, where its
/// least needed passages give way. No count caps a request: the section
/// bounds it. The same pages always pack the same way, so a restart
/// rebuilds the same requests and finds their split checkpoints.
fn pack(
    llm: &dyn LLMPort,
    mut pending: Vec<PendingClaim>,
    labels: &Labels,
) -> Vec<Vec<PendingClaim>> {
    let checker = ClaimChecker::new(
        llm,
        SamplingOverride::deterministic(),
        512,
        CheckPolicy::Strict,
    );
    pending.sort_by_key(|item| (item.unit, item.ordinal));
    let mut requests: Vec<Vec<PendingClaim>> = Vec::new();
    let mut targets: Vec<LocatedClaim> = Vec::new();
    for item in pending {
        let next = target(&item, labels);
        let joins = requests
            .last()
            .and_then(|request| request.first())
            .is_some_and(|first| first.unit == item.unit)
            && checker.batch_fits(&[targets.as_slice(), std::slice::from_ref(&next)].concat());
        if !joins {
            requests.push(Vec::new());
            targets.clear();
        }
        targets.push(next);
        if let Some(request) = requests.last_mut() {
            request.push(item);
        }
    }
    requests
}

/// The ordinals [`pack`] puts in each request for claims given as `(unit,
/// statement, page)`, by ordinal.
#[cfg(test)]
pub(super) fn packed(
    llm: &dyn LLMPort,
    claims: Vec<(usize, String, Vec<EvidencePassage>)>,
) -> Vec<Vec<usize>> {
    let pending = claims
        .into_iter()
        .enumerate()
        .map(|(ordinal, (unit, statement, evidence))| PendingClaim {
            ordinal,
            key: String::new(),
            receipt_key: String::new(),
            unit,
            claim: Claim {
                quote: statement.clone(),
                statement: statement.clone(),
            },
            checked: statement,
            evidence,
        })
        .collect();
    pack(llm, pending, &Labels::new())
        .into_iter()
        .map(|request| request.iter().map(|item| item.ordinal).collect())
        .collect()
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
                    citations,
                )
            })
            .buffered(claims)
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
    // A request already carries several complete evidence sets. Run them one
    // at a time rather than compete prompt prefills on the same model.
    let mut answers = AnswerSize::default();
    let mut groups = futures::stream::iter(pack(llm, pending, &labels))
        .map(|request| group(llm, request, Check::First, &labels))
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
    deepening::deepen(
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
