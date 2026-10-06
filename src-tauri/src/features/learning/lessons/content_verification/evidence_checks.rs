//! Retrieve and restore individual comparisons, then batch only unfinished ones.
//! Every decision retains its own evidence binding and durable receipt.
use super::*;
use crate::application::services::claim_verification::LocatedClaim;

const BATCH_SIZE: usize = 8;

struct PendingClaim {
    ordinal: usize,
    key: String,
    receipt_key: String,
    unit: usize,
    claim: Claim,
    evidence: Vec<EvidencePassage>,
}
enum PreparedClaim {
    Complete(CheckedClaim),
    Pending(PendingClaim),
}

async fn prepare(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    executions: &[execution::Observation],
    retained: &ClaimChecks,
    ordinal: usize,
    unit: usize,
    claim: Claim,
) -> Result<PreparedClaim> {
    let evidence = evidence_for(&claim.statement, unit, references, executions).await?;
    let key = ClaimChecks::key(unit, &claim, &evidence);
    if let Some(finding) = retained.get(&key, &evidence) {
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

async fn group(llm: &dyn LLMPort, prepared: Vec<PreparedClaim>) -> Result<Vec<CheckedClaim>> {
    let mut results = Vec::new();
    let mut pending = Vec::new();
    for item in prepared {
        match item {
            PreparedClaim::Complete(result) => completed(&mut results, result),
            PreparedClaim::Pending(item) => pending.push(item),
        }
    }
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
        if !llm.supports_typed_completions() || pending.len() == 1 {
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
        let _model_call = crate::features::learning::lesson_progress::model_call();
        let judgments = match judge_batch_for_publication(llm, &targets).await {
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
    crate::features::learning::lesson_progress::phase(
        crate::features::learning::lesson_progress::Phase::Evidence,
        format!("Checking evidence and unresolved questions for {total} claims"),
    );
    crate::features::learning::lesson_progress::begin_checks(total);
    let size = if llm.supports_typed_completions() {
        BATCH_SIZE
    } else {
        1
    };
    // A group already carries several complete evidence sets. Avoid competing
    // large prompt prefills on the same model; individual requests stay parallel.
    let concurrency = if size > 1 { 1 } else { 3 };
    // Non-structured providers retain the existing individual-call path.
    let mut groups = futures::stream::iter(entries.into_iter().enumerate())
        .chunks(size)
        .map(|entries| async move {
            // Finish this group's reads before checking and saving it. Prefetch
            // outside the group can leave unpolled reads holding pool slots
            // while the current group waits for a connection to save receipts.
            let prepared = futures::stream::iter(entries)
                .map(|(ordinal, (unit, claim))| {
                    prepare(llm, references, executions, retained, ordinal, unit, claim)
                })
                .buffered(size)
                .try_collect()
                .await?;
            group(llm, prepared).await
        })
        .buffer_unordered(concurrency);
    let mut results = Vec::new();
    let mut reused = 0;
    while let Some(batch) = groups.next().await {
        for result in batch? {
            reused += usize::from(result.3);
            results.push(result);
        }
        let reuse = if reused == 0 {
            String::new()
        } else {
            format!(" · {reused} unchanged checks reused")
        };
        let batching = if size > 1 {
            " · Checking remaining claims in groups"
        } else {
            ""
        };
        crate::features::learning::lesson_progress::stage(format!(
            "Checked {} of {total} claims against saved references{reuse}{batching}",
            results.len()
        ));
    }
    results.sort_by_key(|(ordinal, _, _, _)| *ordinal);
    Ok(results)
}
