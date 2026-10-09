//! Retrieve and restore individual comparisons, then batch only unfinished ones.
//! Every decision retains its own evidence binding and durable receipt.
use super::*;
use crate::application::services::claim_verification::LocatedClaim;

const BATCH_SIZE: usize = 8;
const PROVISIONAL_BATCH_POLICY: &str =
    crate::application::services::claim_verification::STRICT_INTERPRETATION_POLICY;

struct PendingClaim {
    ordinal: usize,
    key: String,
    receipt_key: String,
    unit: usize,
    claim: Claim,
    evidence: Vec<EvidencePassage>,
}

/// First-stage entailment is useful saved work, but never publication approval.
/// A supported result must still pass the independent evidence challenge.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProvisionalBatchReceipt {
    policy: String,
    model: String,
    model_context: usize,
    decisions: Vec<ProvisionalDecision>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProvisionalDecision {
    verdict: ClaimVerdict,
    reason: String,
    supporting_quote: Option<String>,
}

fn provisional_key(pending: &[PendingClaim]) -> String {
    let comparisons = pending
        .iter()
        .map(|item| item.receipt_key.as_str())
        .collect::<Vec<_>>();
    format!(
        "claim-batch-provisional-v1:{}",
        digest(&json!(comparisons).to_string())
    )
}

impl ProvisionalBatchReceipt {
    fn from_judgments(llm: &dyn LLMPort, judgments: &[ClaimJudgment]) -> Option<Self> {
        let decisions = judgments
            .iter()
            .map(|judgment| match judgment {
                ClaimJudgment::Judged(outcome)
                    if outcome.verdict != ClaimVerdict::Unverified
                        && outcome
                            .reason
                            .as_ref()
                            .is_some_and(|reason| !reason.trim().is_empty()) =>
                {
                    Some(ProvisionalDecision {
                        verdict: outcome.verdict,
                        reason: outcome.reason.clone()?,
                        supporting_quote: outcome.quote.clone(),
                    })
                }
                _ => None,
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            policy: PROVISIONAL_BATCH_POLICY.into(),
            model: llm.model_name().into(),
            model_context: llm.max_context_tokens(),
            decisions,
        })
    }

    fn judgments(self, llm: &dyn LLMPort, pending: &[PendingClaim]) -> Option<Vec<ClaimJudgment>> {
        if self.policy != PROVISIONAL_BATCH_POLICY
            || self.model != llm.model_name()
            || self.model_context != llm.max_context_tokens()
            || self.decisions.len() != pending.len()
        {
            return None;
        }
        self.decisions
            .into_iter()
            .zip(pending)
            .map(|(decision, target)| {
                let reason = decision.reason.trim();
                if reason.is_empty() || reason.chars().count() > 600 {
                    return None;
                }
                let quote_is_valid = match decision.verdict {
                    ClaimVerdict::Supported | ClaimVerdict::Contradicted => {
                        decision.supporting_quote.as_ref().is_some_and(|quote| {
                            !quote.trim().is_empty()
                                && target
                                    .evidence
                                    .iter()
                                    .any(|passage| passage.text.contains(quote))
                        })
                    }
                    ClaimVerdict::Unsupported => decision.supporting_quote.is_none(),
                    ClaimVerdict::Unverified => false,
                };
                quote_is_valid.then_some(ClaimJudgment::Judged(JudgeOutcome {
                    verdict: decision.verdict,
                    reason: Some(decision.reason),
                    quote: decision.supporting_quote,
                    confidence: None,
                }))
            })
            .collect()
    }
}

enum PreparedClaim {
    Complete(CheckedClaim),
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
) -> Result<PreparedClaim> {
    let (unit, claim) = located_claim;
    let evidence =
        evidence_selection::select(llm, &claim, unit, references, sources, executions, retained)
            .await?;
    let key = ClaimChecks::key(unit, &claim, &evidence);
    if let Some(finding) = retained.get(&key, unit, &claim, &evidence) {
        return Ok(PreparedClaim::Complete((ordinal, key, finding, true)));
    }
    let receipt_key = claim_receipt_key(llm, &key);
    if let Some(finding) = crate::features::learning::lesson_drafts::checkpoint(&receipt_key)
        .await?
        .and_then(|value| serde_json::from_value::<ClaimReceipt>(value).ok())
        .and_then(|receipt| receipt.finding(unit, &claim, &evidence))
    {
        return Ok(PreparedClaim::Complete((ordinal, key, finding, true)));
    }
    if let Some(receipt) =
        claim_reuse::legacy_receipt(llm, retained, unit, &claim, &evidence).await?
    {
        let saved = serde_json::to_value(&receipt)?;
        if let Some(finding) = receipt.finding(unit, &claim, &evidence) {
            crate::features::learning::lesson_drafts::record_checkpoint(&receipt_key, saved)
                .await?;
            return Ok(PreparedClaim::Complete((ordinal, key, finding, true)));
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
    } else if llm.count_tokens(&text) + llm.count_tokens(&claim.statement) + 1200
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
        evidence,
    };
    if let Some(judgment) = local {
        return Ok(PreparedClaim::Complete(finish(pending, judgment).await?));
    }
    Ok(PreparedClaim::Pending(pending))
}

async fn finish(pending: PendingClaim, judgment: ClaimJudgment) -> Result<CheckedClaim> {
    let (verdict, reason, supporting_quote) = match judgment {
        ClaimJudgment::Failed(error) => return Err(error),
        ClaimJudgment::Judged(outcome) => (
            outcome.verdict,
            outcome.reason.unwrap_or_default(),
            outcome.quote,
        ),
        ClaimJudgment::OutOfTime => (
            ClaimVerdict::Unverified,
            "The claim checking deadline expired.".into(),
            None,
        ),
        ClaimJudgment::Unusable => (
            ClaimVerdict::Unverified,
            "Missing evidence or a failed, incomplete, or uncertain checker response.".into(),
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
    };
    if finding.verdict != ClaimVerdict::Unverified {
        crate::features::learning::lesson_drafts::record_checkpoint(
            &pending.receipt_key,
            serde_json::to_value(ClaimReceipt {
                verdict: finding.verdict,
                reason: finding.reason.clone(),
                supporting_quote: finding.supporting_quote.clone(),
                interpretation_policy: Some(
                    crate::application::services::claim_verification::STRICT_INTERPRETATION_POLICY
                        .into(),
                ),
            })?,
        )
        .await?;
    }
    Ok((pending.ordinal, pending.key, finding, false))
}

fn target(pending: &PendingClaim) -> LocatedClaim {
    LocatedClaim {
        claim: pending.claim.statement.clone(),
        passages: pending.evidence.iter().map(|p| p.text.clone()).collect(),
    }
}

async fn individual(llm: &dyn LLMPort, pending: PendingClaim) -> Result<CheckedClaim> {
    let input = target(&pending);
    let judgment = judge_for_publication(llm, &input.claim, &input.passages).await;
    finish(pending, judgment).await
}

fn completed(results: &mut Vec<CheckedClaim>, result: CheckedClaim) {
    // Individual fallbacks and smaller groups can take minutes. Report each
    // finished comparison immediately, even while its siblings are pending.
    crate::features::learning::lesson_progress::checked(
        result.3,
        result.2.verdict != ClaimVerdict::Supported,
    );
    results.push(result);
}

async fn group(llm: &dyn LLMPort, pending: Vec<PendingClaim>) -> Result<Vec<CheckedClaim>> {
    let mut results = Vec::new();
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
        let targets: Vec<_> = pending.iter().map(target).collect();
        if pending.len() == 1 {
            for item in pending {
                completed(&mut results, individual(llm, item).await?);
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
        let provisional_key = provisional_key(&pending);
        let restored = crate::features::learning::lesson_drafts::checkpoint(&provisional_key)
            .await?
            .and_then(|value| serde_json::from_value::<ProvisionalBatchReceipt>(value).ok())
            .and_then(|receipt| receipt.judgments(llm, &pending));
        let provisional = if let Some(restored) = restored {
            crate::features::learning::lesson_progress::stage(format!(
                "Restored first-stage judgments for {} claims; continuing with their independent evidence challenge",
                pending.len()
            ));
            restored
        } else {
            crate::features::learning::lesson_progress::stage(format!(
                "Comparing a group of {} remaining claims with their assigned evidence",
                pending.len()
            ));
            let _model_call = crate::features::learning::lesson_progress::model_call();
            let judgments = match judge_batch_provisionally(llm, &targets).await {
                Ok(judgments) => judgments,
                Err(AppError::ServiceNotAvailable(_)) => {
                    crate::features::learning::lesson_drafts::record_checkpoint(
                        &split_key,
                        json!(true),
                    )
                    .await?;
                    crate::features::learning::lesson_progress::stage(format!(
                        "The model could not finish a group of {} checks; retrying in smaller groups with the same evidence",
                        pending.len()
                    ));
                    let rest = pending.split_off(pending.len() / 2);
                    groups.push(rest);
                    groups.push(pending);
                    continue;
                }
                Err(error) => return Err(error),
            };
            if let Some(receipt) = ProvisionalBatchReceipt::from_judgments(llm, &judgments) {
                crate::features::learning::lesson_drafts::record_checkpoint(
                    &provisional_key,
                    serde_json::to_value(receipt)?,
                )
                .await?;
            }
            judgments
        };
        let approvals = provisional
            .iter()
            .filter(|judgment| {
                matches!(judgment, ClaimJudgment::Judged(outcome) if outcome.verdict == ClaimVerdict::Supported)
            })
            .count();
        let judgments = if approvals == 0 {
            crate::features::learning::lesson_progress::stage(format!(
                "The first-stage comparison found no provisional approvals; saving {} completed checks",
                pending.len()
            ));
            provisional
        } else {
            crate::features::learning::lesson_progress::stage(format!(
                "Challenging {approvals} provisional approvals before saving this group of {} checks",
                pending.len()
            ));
            let _model_call = crate::features::learning::lesson_progress::model_call();
            match challenge_batch_for_publication(llm, &targets, provisional).await {
                Ok(judgments) => judgments,
                // The first-stage judgments are already durable. Let the job
                // retry this challenge instead of splitting and redoing them.
                Err(error @ AppError::ServiceNotAvailable(_)) => return Err(error),
                Err(error) => return Err(error),
            }
        };
        if judgments.len() != pending.len() {
            return Err(invalid("Incomplete claim batch; no lesson was published."));
        }
        let mut unresolved = Vec::new();
        for (item, judgment) in pending.into_iter().zip(judgments) {
            if matches!(judgment, ClaimJudgment::Unusable | ClaimJudgment::OutOfTime) {
                unresolved.push(item);
            } else {
                // Save completed siblings before a malformed response needs a
                // slower individual recheck. An outage must not discard them.
                completed(&mut results, finish(item, judgment).await?);
            }
        }
        for item in unresolved {
            completed(&mut results, individual(llm, item).await?);
        }
    }
    Ok(results)
}

pub(super) async fn check(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    inventory: &Inventory,
    executions: &[execution::Observation],
    retained: &ClaimChecks,
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
                )
            })
            .buffered(BATCH_SIZE)
            .try_collect()
            .await?;
        for item in prepared {
            match item {
                PreparedClaim::Complete(result) => {
                    reused += usize::from(result.3);
                    completed(&mut results, result);
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
    let mut groups = futures::stream::iter(pending)
        .chunks(BATCH_SIZE)
        .map(|pending| group(llm, pending))
        .buffer_unordered(1);
    while let Some(batch) = groups.next().await {
        results.extend(batch?);
        crate::features::learning::lesson_progress::stage(format!(
            "Completed {} of {model_checks} model checks · {reused} saved checks reused · {total} lesson claims",
            results.len() - resolved
        ));
    }
    results.sort_by_key(|(ordinal, _, _, _)| *ordinal);
    Ok(results)
}
