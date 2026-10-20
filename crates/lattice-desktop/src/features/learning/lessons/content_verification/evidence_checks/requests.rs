//! The model requests a pass sends: comparisons packed by section, a
//! claim's two together, judged in one batch while the window holds them,
//! in halves when the backend counts a batch past it, and alone when one
//! comparison is all a request holds; each judgment finished into a finding
//! and its receipt.
use super::*;

/// Finish `pending` with the judge's `judgment`. `shown` holds the positions
/// in the page of the passages the judge was shown when it was not shown them
/// all; the pending claim's evidence is already cut to those, and its receipt
/// records them, so a restored decision names the same passages.
pub(super) async fn finish(
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
        two_pass: TwoPass::default(),
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
pub(super) fn labelled(pending: &PendingClaim, labels: &Labels) -> Vec<String> {
    pending
        .evidence
        .iter()
        .map(|p| match labels.get(&p.source_id) {
            Some(label) => format!("{label}\n{}", p.text),
            None => p.text.clone(),
        })
        .collect()
}

/// The order the judge reads `pending`'s page in: a pass B page by need,
/// the most relevant at the edges (`reading_order`); pass A's short page in
/// its canonical order.
pub(super) fn reading(pending: &PendingClaim) -> Vec<usize> {
    match pending.pass {
        Pass::Attribution => (0..pending.evidence.len()).collect(),
        Pass::Corroboration => reading_order(&pending.evidence),
    }
}

/// The claim as the checker reads it, its labelled passages in reading
/// order. A batch's passage bank keeps them in this order, so a passage two
/// claims share sits where the first one reads it.
pub(super) fn target(pending: &PendingClaim, labels: &Labels) -> LocatedClaim {
    let passages = labelled(pending, labels);
    LocatedClaim {
        claim: pending.checked.clone(),
        passages: reading(pending)
            .into_iter()
            .filter_map(|position| passages.get(position).cloned())
            .collect(),
    }
}

/// The checker attaches the text of the passage it cites, label included;
/// the finding binds to the passage's own text.
pub(super) fn unlabelled(
    judgment: ClaimJudgment,
    pending: &PendingClaim,
    labels: &Labels,
) -> ClaimJudgment {
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

/// One comparison checked alone. A page too long for the judge's window
/// loses its least needed passages (see [`ranking`]); the finding and its
/// receipt name only the passages the judge was shown, so one cut for size
/// is still unshown and a deeper page can carry it. The check comes with
/// what its answer cost.
pub(super) async fn individual(
    llm: &dyn LLMPort,
    mut pending: PendingClaim,
    labels: &Labels,
) -> Result<((Pass, CheckedClaim), AnswerSize)> {
    let judged = judge_in_order(
        llm,
        &pending.checked,
        &labelled(&pending, labels),
        &ranking(&pending.evidence),
        &reading(&pending),
    )
    .await;
    let shown = judged.shown;
    let cut = shown.len() < pending.evidence.len();
    pending.evidence = shown_of(&pending.evidence, &shown);
    let judgment = unlabelled(judged.judgment, &pending, labels);
    let answer = AnswerSize::of(judged.output_tokens, 1);
    let pass = pending.pass;
    Ok((
        (pass, finish(pending, judgment, cut.then_some(shown)).await?),
        answer,
    ))
}

pub(super) fn completed(
    results: &mut Vec<(Pass, CheckedClaim)>,
    done: (Pass, CheckedClaim),
    check: Check,
) {
    // A deeper page reports each finished comparison at once, even while its
    // siblings are pending; a first check reports its claim when all of the
    // claim's comparisons are done.
    if let Check::Deeper = check {
        crate::features::learning::lesson_progress::rechecked(
            done.1 .2.verdict == ClaimVerdict::Supported,
        );
    }
    results.push(done);
}

/// Check `pending`'s comparisons, with what the judge's answers to them
/// cost.
pub(super) async fn group(
    llm: &dyn LLMPort,
    pending: Vec<PendingClaim>,
    check: Check,
    labels: &Labels,
) -> Result<(Vec<(Pass, CheckedClaim)>, AnswerSize)> {
    let mut results = Vec::new();
    let mut answers = AnswerSize::default();
    let checker = ClaimChecker::new(
        llm,
        SamplingOverride::deterministic(),
        CHECKER_OUTPUT_TOKENS,
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
        let mut claims: Vec<usize> = pending.iter().map(|item| item.ordinal).collect();
        claims.dedup();
        crate::features::learning::lesson_progress::stage(format!(
            "Comparing {} claims of section {} with their evidence",
            claims.len(),
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
                let pass = item.pass;
                completed(
                    &mut results,
                    (pass, finish(item, judgment, None).await?),
                    check,
                );
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

/// The model requests for `pending` (§5.6): claims of one section at a time,
/// in section and claim order, a claim's comparisons (pass A, then pass B's
/// page) together, a request taking claims while the batch estimate says
/// the window holds them all with their answers. A section's claims share
/// its cited passages, which the bank holds once. The first claim that does
/// not fit starts the next request; comparisons that do not fit a request
/// alone are checked in halves, down to one alone, where its least needed
/// passages give way. No count caps a request: the section bounds it. The
/// same pages always pack the same way, so a restart rebuilds the same
/// requests and finds their split checkpoints.
pub(super) fn pack(
    llm: &dyn LLMPort,
    mut pending: Vec<PendingClaim>,
    labels: &Labels,
) -> Vec<Vec<PendingClaim>> {
    let checker = ClaimChecker::new(
        llm,
        SamplingOverride::deterministic(),
        CHECKER_OUTPUT_TOKENS,
        CheckPolicy::Strict,
    );
    pending.sort_by_key(|item| (item.unit, item.ordinal, item.pass));
    let mut claims: Vec<Vec<PendingClaim>> = Vec::new();
    for item in pending {
        match claims.last_mut() {
            Some(claim)
                if claim
                    .first()
                    .is_some_and(|first| first.ordinal == item.ordinal) =>
            {
                claim.push(item);
            }
            _ => claims.push(vec![item]),
        }
    }
    let mut requests: Vec<Vec<PendingClaim>> = Vec::new();
    let mut targets: Vec<LocatedClaim> = Vec::new();
    for claim in claims {
        let next: Vec<LocatedClaim> = claim.iter().map(|item| target(item, labels)).collect();
        let unit = claim.first().map(|item| item.unit);
        let joins = requests
            .last()
            .and_then(|request| request.first())
            .is_some_and(|first| Some(first.unit) == unit)
            && checker.batch_fits(&[targets.as_slice(), next.as_slice()].concat());
        if !joins {
            requests.push(Vec::new());
            targets.clear();
        }
        targets.extend(next);
        if let Some(request) = requests.last_mut() {
            request.extend(claim);
        }
    }
    requests
}

/// [`outcome::combine`], for tests of what two passes decide together.
#[cfg(test)]
pub(in crate::features::learning::lessons::content_verification) fn combined(
    attribution: Option<Finding>,
    corroboration: Finding,
    at: (Option<Reach>, usize),
    cited: &[String],
) -> Finding {
    combine(attribution, corroboration, at, cited)
}

/// The comparisons [`pack`] puts in each request, as `(ordinal, pass)`, for
/// comparisons given as `(ordinal, unit, statement, pass, page)`.
#[cfg(test)]
pub(in crate::features::learning::lessons::content_verification) fn packed(
    llm: &dyn LLMPort,
    comparisons: Vec<(usize, usize, String, Pass, Vec<EvidencePassage>)>,
) -> Vec<Vec<(usize, Pass)>> {
    let pending = comparisons
        .into_iter()
        .map(|(ordinal, unit, statement, pass, evidence)| PendingClaim {
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
            pass,
        })
        .collect();
    pack(llm, pending, &Labels::new())
        .into_iter()
        .map(|request| {
            request
                .iter()
                .map(|item| (item.ordinal, item.pass))
                .collect()
        })
        .collect()
}
