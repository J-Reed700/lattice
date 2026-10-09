//! Challenge a batch of provisional approvals without combining their evidence.
use super::*;
use crate::application::services::claim_verification::{render_batch_evidence, LocatedClaim};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BatchQuestions {
    checks: Vec<CheckQuestions>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckQuestions {
    id: String,
    queries: Vec<Question>,
}

pub(in crate::features::learning::lessons::content_verification) async fn guard_batch(
    llm: &dyn LLMPort,
    claims: &[LocatedClaim],
    provisional: Vec<ClaimJudgment>,
) -> Result<Vec<ClaimJudgment>> {
    if claims.len() != provisional.len() {
        return Err(invalid("Claim batch and decisions do not match."));
    }
    let selected: Vec<_> = provisional.iter().enumerate().filter_map(|(index, result)| {
        matches!(result, ClaimJudgment::Judged(finding) if finding.verdict == ClaimVerdict::Supported).then_some(index)
    }).collect();
    if selected.is_empty() {
        return Ok(provisional);
    }
    let targets = selected
        .iter()
        .filter_map(|index| claims.get(*index).cloned())
        .collect::<Vec<_>>();
    let (prompt, _) = render_batch_evidence(&targets);
    let system = format!("{SYSTEM}\n\nThis request contains several independent claims and a bank of shared source passages. For EACH claim, use only the passages in that claim's evidenceIds. Other claims and evidence allocated only to them are not evidence for this claim. Review each claim separately; do not infer that approval or a concern applies to the whole batch. Return JSON with checks: exactly one object for every supplied claim ID, containing id and queries. Keep queries empty only when that claim has no concrete unresolved concern. Do not omit any claim or invent an ID.");
    let remaining = llm
        .max_context_tokens()
        .saturating_sub(llm.count_tokens(&system) + llm.count_tokens(&prompt));
    if remaining < 1200 * targets.len() {
        return Err(invalid(
            "The evidence challenge batch needs a smaller context.",
        ));
    }
    let ids: Vec<_> = (0..targets.len())
        .map(|index| format!("claim-{index}"))
        .collect();
    let response = llm.complete_with_retry_progress(&CompletionRequest {
        input:vec![CompletionInput::Message {role:"system".into(),content:system},CompletionInput::Message {role:"user".into(),content:prompt}],
        json_schema:Some(json!({"type":"object","additionalProperties":false,"required":["checks"],"properties":{"checks":{"type":"array","minItems":targets.len(),"maxItems":targets.len(),"items":{"type":"object","additionalProperties":false,"required":["id","queries"],"properties":{"id":{"type":"string","enum":ids},"queries":{"type":"array","maxItems":3,"items":{"type":"object","additionalProperties":false,"required":["query","rationale"],"properties":{"query":{"type":"string","minLength":3,"maxLength":240},"rationale":{"type":"string","minLength":1,"maxLength":800}}}}}}}}})),
        sampling:Some(SamplingOverride::deterministic()),reasoning_effort:Some("low".into()),max_output_tokens:Some(remaining.min(u32::MAX as usize) as u32),no_time_limit:true,
        priority: crate::application::ports::llm_port::InferencePriority::Verification,
        cache_key: crate::features::learning::lesson_progress::cache_key(),
        ..Default::default()
    }, &|text| {
        crate::features::learning::lesson_progress::received(&text);
        Ok(())
    }, &|attempt| {
        crate::features::learning::lesson_progress::model_retry(attempt);
        Ok(())
    }).await?;
    let checks = if matches!(
        response.finish_reason.as_str(),
        "stop" | "end_turn" | "completed"
    ) {
        serde_json::from_str::<BatchQuestions>(&response.text)
            .ok()
            .and_then(|reply| {
                if reply.checks.len() != targets.len() {
                    return None;
                }
                let mut seen = HashSet::new();
                let mut results = Vec::new();
                for check in reply.checks {
                    let index: usize = check.id.strip_prefix("claim-")?.parse().ok()?;
                    if index >= targets.len()
                        || check.id != format!("claim-{index}")
                        || !seen.insert(index)
                    {
                        return None;
                    }
                    let questions = Questions {
                        queries: check.queries,
                    };
                    validate_questions(&questions).ok()?;
                    results.push((index, questions));
                }
                Some(results)
            })
    } else {
        None
    };
    let mut result = provisional;
    if let Some(checks) = checks {
        for (index, questions) in checks {
            if let Some(finding) = selected
                .get(index)
                .and_then(|original| result.get_mut(*original))
            {
                *finding = apply_questions(finding.clone(), questions);
            }
        }
    } else {
        for index in selected {
            if let Some(finding) = result.get_mut(index) {
                *finding = ClaimJudgment::Unusable;
            }
        }
    }
    Ok(result)
}
