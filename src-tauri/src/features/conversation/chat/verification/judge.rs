//! LLM claim judge.
//!
//! The lexical pre-filter can only see shared vocabulary. This asks a model
//! whether the cited passages actually entail the claim, which is the only way
//! to catch a sentence that borrows a source's wording and inverts its meaning.
//!
//! Failed, timed-out, or unparseable judgments return no outcome. The verifier
//! leaves those escalated claims unresolved rather than certifying them from
//! vocabulary overlap alone.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use tokio::time::Instant;
use tracing::{debug, warn};

use crate::application::ports::llm_port::{CompletionInput, CompletionRequest};
use crate::application::ports::LLMPort;
use crate::features::qa::dto::SourceDto;
use crate::shared::error::Result;
use crate::shared::text_utils::normalize_whitespace;

use super::super::prompting::{
    claim_judge_schema, render_claim_judge_prompt, ClaimJudgePassage, ClaimJudgeRequest,
    CLAIM_JUDGE_SYSTEM,
};
use super::lexical::LexicalClaim;
use super::ClaimVerdict;

/// Claims per request. Larger batches cut round trips; past roughly this many
/// the model starts dropping ids and the whole batch has to fall back.
pub(super) const MAX_CLAIMS_PER_CALL: usize = 12;

/// Wall-clock ceiling for all judging in one turn.
pub(super) const DEFAULT_TIME_BUDGET: Duration = Duration::from_secs(8);

/// Do not start a request that cannot plausibly finish inside what is left.
const MIN_CALL_SLICE: Duration = Duration::from_millis(250);

const MAX_PASSAGE_CHARS: usize = 1200;
const MAX_PASSAGES_PER_CLAIM: usize = 3;
/// Ceiling on the shared passage table for one request, so a batch whose
/// claims cite a dozen different sources cannot blow the prompt window.
const MAX_PASSAGES_PER_BATCH: usize = 12;
const MAX_QUOTE_CHARS: usize = 400;

/// What the judge concluded for one claim.
#[derive(Debug, Clone)]
pub(super) struct JudgeOutcome {
    pub(super) verdict: ClaimVerdict,
    pub(super) quote: Option<String>,
}

pub(super) struct ClaimJudge {
    llm: Arc<dyn LLMPort>,
    batch_size: usize,
    time_budget: Duration,
}

impl ClaimJudge {
    pub(super) fn new(llm: Arc<dyn LLMPort>) -> Self {
        Self {
            llm,
            batch_size: MAX_CLAIMS_PER_CALL,
            time_budget: DEFAULT_TIME_BUDGET,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn with_time_budget(mut self, budget: Duration) -> Self {
        self.time_budget = budget;
        self
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size.max(1);
        self
    }

    pub(super) fn model_name(&self) -> &str {
        self.llm.model_name()
    }

    /// Judge the claims at `pending` indices, keyed back by those indices.
    ///
    /// Missing claims remain unresolved: the budget ran out, the call failed,
    /// or the model did not return a usable judgment.
    pub(super) async fn judge_claims(
        &self,
        claims: &[LexicalClaim],
        pending: &[usize],
        sources: &[SourceDto],
    ) -> HashMap<usize, JudgeOutcome> {
        let mut outcomes = HashMap::new();
        if pending.is_empty() {
            return outcomes;
        }

        let deadline = Instant::now() + self.time_budget;
        for batch in pending.chunks(self.batch_size) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining < MIN_CALL_SLICE {
                warn!(
                    judged = outcomes.len(),
                    remaining_claims = pending.len().saturating_sub(outcomes.len()),
                    budget_ms = self.time_budget.as_millis(),
                    "Claim judge time budget exhausted — remaining claims stay unresolved"
                );
                break;
            }

            let Some(batch_input) = BatchInput::build(claims, batch, sources) else {
                continue;
            };
            let requests: Vec<ClaimJudgeRequest<'_>> = batch_input
                .claims
                .iter()
                .enumerate()
                .map(|(slot, (_, claim, citations))| ClaimJudgeRequest {
                    index: slot + 1,
                    claim: claim.claim_text.as_str(),
                    citations: citations.clone(),
                })
                .collect();
            let passages: Vec<ClaimJudgePassage<'_>> = batch_input
                .table
                .iter()
                .map(|(citation_id, text)| ClaimJudgePassage {
                    citation_id: *citation_id,
                    text: text.as_str(),
                })
                .collect();

            let prompt = render_claim_judge_prompt(&passages, &requests);
            let response = match tokio::time::timeout(remaining, self.request(&prompt)).await {
                Ok(Ok(text)) => text,
                Ok(Err(e)) => {
                    warn!(error = %e, batch = batch.len(), "Claim judge call failed — leaving this batch unresolved");
                    continue;
                }
                Err(_) => {
                    warn!(
                        batch = batch.len(),
                        "Claim judge call exceeded the remaining time budget — leaving claims unresolved"
                    );
                    break;
                }
            };

            let parsed = parse_judge_response(&response);
            if parsed.is_empty() {
                warn!(
                    batch = batch.len(),
                    response_chars = response.len(),
                    "Claim judge returned no parseable verdicts — leaving this batch unresolved"
                );
                continue;
            }

            for raw in parsed {
                let Some(slot) = raw.id.and_then(|id| usize::try_from(id).ok()) else {
                    continue;
                };
                let Some(slot) = slot.checked_sub(1) else {
                    continue;
                };
                let Some((claim_index, _, citations)) = batch_input.claims.get(slot) else {
                    continue;
                };
                let Some(verdict) = raw.verdict.as_deref().and_then(parse_verdict) else {
                    continue;
                };
                let quote = raw
                    .quote
                    .as_deref()
                    .and_then(|quote| batch_input.verified_quote(quote, citations));
                // A positive verdict without a real supporting span is not evidence.
                // Do not fall back to lexical support: the judge may be certifying
                // exactly the numeric/negated claim the lexical pass cannot settle.
                let verdict = if verdict == ClaimVerdict::Supported && quote.is_none() {
                    ClaimVerdict::Unsupported
                } else {
                    verdict
                };
                outcomes.insert(*claim_index, JudgeOutcome { verdict, quote });
            }
        }

        debug!(
            judged = outcomes.len(),
            requested = pending.len(),
            model = self.llm.model_name(),
            "Claim judge finished"
        );
        outcomes
    }

    async fn request(&self, prompt: &str) -> Result<String> {
        if self.llm.supports_typed_completions() {
            self.llm
                .complete(&CompletionRequest {
                    input: vec![
                        CompletionInput::Message {
                            role: "system".into(),
                            content: CLAIM_JUDGE_SYSTEM.into(),
                        },
                        CompletionInput::Message {
                            role: "user".into(),
                            content: prompt.to_string(),
                        },
                    ],
                    reasoning_effort: Some("none".into()),
                    json_schema: Some(claim_judge_schema()),
                    ..Default::default()
                })
                .await
                .map(|response| response.text)
        } else {
            self.llm
                .generate(prompt, &[format!("System: {CLAIM_JUDGE_SYSTEM}")], None)
                .await
        }
    }
}

/// One request's worth of claims and the passage table they share.
struct BatchInput<'a> {
    /// `(index into the full claim list, the claim, the citations it cites)`.
    claims: Vec<(usize, &'a LexicalClaim, Vec<u32>)>,
    /// Distinct passages, keyed by the citation number the answering model saw.
    table: Vec<(u32, String)>,
}

impl<'a> BatchInput<'a> {
    /// Collect a batch, dropping claims with no passage to judge them against.
    ///
    /// Returns `None` when nothing in the batch is judgeable, so the caller
    /// spends no call on a request that could only answer "unsupported".
    fn build(claims: &'a [LexicalClaim], batch: &[usize], sources: &[SourceDto]) -> Option<Self> {
        let mut input = Self {
            claims: Vec::new(),
            table: Vec::new(),
        };

        for index in batch {
            let Some(claim) = claims.get(*index) else {
                continue;
            };
            let mut citations = Vec::new();
            for (citation_id, text) in passages_for(claim, sources) {
                if !input.table.iter().any(|(id, _)| *id == citation_id) {
                    if input.table.len() >= MAX_PASSAGES_PER_BATCH {
                        continue;
                    }
                    input.table.push((citation_id, text));
                }
                if !citations.contains(&citation_id) {
                    citations.push(citation_id);
                }
            }
            if citations.is_empty() {
                continue;
            }
            input.claims.push((*index, claim, citations));
        }

        (!input.claims.is_empty()).then_some(input)
    }

    /// Accept a quote only when it really is in one of the claim's passages.
    ///
    /// A judge that invents its supporting span has not read the passage, and
    /// a fabricated quote shown beside a "supported" badge is worse than none.
    fn verified_quote(&self, quote: &str, citations: &[u32]) -> Option<String> {
        let trimmed = normalize_whitespace(quote).trim().to_string();
        if trimmed.is_empty() {
            return None;
        }
        let needle = trimmed.to_lowercase();
        let found = self
            .table
            .iter()
            .filter(|(citation_id, _)| citations.contains(citation_id))
            .any(|(_, text)| normalize_whitespace(text).to_lowercase().contains(&needle));
        found.then(|| truncate_chars(&trimmed, MAX_QUOTE_CHARS))
    }
}

/// Evidence offered for one claim: the passages it cites, or the top sources
/// when it cites none — the same passages the lexical pass scored against.
fn passages_for(claim: &LexicalClaim, sources: &[SourceDto]) -> Vec<(u32, String)> {
    if claim.citation_ids.len() != claim.cited_source_indices.len() {
        return Vec::new();
    }
    let indices: Vec<usize> = if claim.citation_ids.is_empty() {
        (0..sources.len().min(MAX_PASSAGES_PER_CLAIM)).collect()
    } else {
        claim
            .cited_source_indices
            .iter()
            .copied()
            .take(MAX_PASSAGES_PER_CLAIM)
            .collect()
    };

    indices
        .into_iter()
        .filter_map(|idx| {
            let source = sources.get(idx)?;
            let text = passage_text(source);
            if text.trim().is_empty() {
                return None;
            }
            let citation_id = source
                .citation_id
                .unwrap_or_else(|| u32::try_from(idx + 1).unwrap_or(u32::MAX));
            Some((citation_id, text))
        })
        .collect()
}

fn passage_text(source: &SourceDto) -> String {
    let body = if source.content.trim().is_empty() {
        source.excerpt.as_deref().unwrap_or_default()
    } else {
        source.content.as_str()
    };
    truncate_chars(&normalize_whitespace(body), MAX_PASSAGE_CHARS)
}

fn truncate_chars(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut out: String = text.chars().take(limit).collect();
    out.push('…');
    out
}

#[derive(Debug, Deserialize)]
pub(super) struct RawVerdict {
    #[serde(default, alias = "claim_id", alias = "claimId", alias = "index")]
    pub(super) id: Option<i64>,
    #[serde(default, alias = "label", alias = "status", alias = "result")]
    pub(super) verdict: Option<String>,
    #[serde(
        default,
        alias = "evidence",
        alias = "evidence_quote",
        alias = "evidenceQuote",
        alias = "span"
    )]
    pub(super) quote: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VerdictEnvelope {
    #[serde(
        default,
        alias = "claims",
        alias = "results",
        alias = "items",
        alias = "judgements",
        alias = "judgments"
    )]
    verdicts: Vec<RawVerdict>,
}

pub(super) fn parse_verdict(raw: &str) -> Option<ClaimVerdict> {
    match raw.trim().to_lowercase().replace(['_', '-'], " ").as_str() {
        "supported" | "support" | "supports" | "entailed" | "yes" | "true" => {
            Some(ClaimVerdict::Supported)
        }
        "contradicted" | "contradict" | "contradiction" | "contradicts" | "refuted"
        | "contradictory" => Some(ClaimVerdict::Contradicted),
        "unsupported" | "not supported" | "unsupport" | "no" | "false" | "neutral" | "unknown" => {
            Some(ClaimVerdict::Unsupported)
        }
        _ => None,
    }
}

/// Parse a judge response as leniently as is safe.
///
/// Models wrap JSON in fences, prepend commentary, and truncate mid-array.
/// Each fallback recovers more of a damaged response; returning an empty vec
/// means the caller keeps every lexical verdict in the batch.
pub(super) fn parse_judge_response(text: &str) -> Vec<RawVerdict> {
    let cleaned = strip_code_fences(text);
    if cleaned.is_empty() {
        return Vec::new();
    }

    for candidate in [cleaned, json_span(cleaned).unwrap_or(cleaned)] {
        if let Ok(envelope) = serde_json::from_str::<VerdictEnvelope>(candidate) {
            let usable = retain_usable(envelope.verdicts);
            if !usable.is_empty() {
                return usable;
            }
        }
        if let Ok(list) = serde_json::from_str::<Vec<RawVerdict>>(candidate) {
            let usable = retain_usable(list);
            if !usable.is_empty() {
                return usable;
            }
        }
    }

    // Truncated or otherwise damaged output: salvage whatever complete objects
    // survived. A half-written array still carries earlier, intact verdicts.
    retain_usable(
        balanced_objects(cleaned)
            .into_iter()
            .filter_map(|object| serde_json::from_str::<RawVerdict>(object).ok())
            .collect(),
    )
}

fn retain_usable(verdicts: Vec<RawVerdict>) -> Vec<RawVerdict> {
    verdicts
        .into_iter()
        .filter(|raw| raw.id.is_some() && raw.verdict.as_deref().and_then(parse_verdict).is_some())
        .collect()
}

fn strip_code_fences(text: &str) -> &str {
    text.trim()
        .trim_start_matches("```json")
        .trim_start_matches("```JSON")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
}

/// The widest `{...}` or `[...]` span, for responses padded with prose.
fn json_span(text: &str) -> Option<&str> {
    let open_object = text.find('{');
    let open_array = text.find('[');
    let (start, close) = match (open_object, open_array) {
        (Some(o), Some(a)) if a < o => (a, ']'),
        (Some(o), _) => (o, '}'),
        (None, Some(a)) => (a, ']'),
        (None, None) => return None,
    };
    let end = text.rfind(close)?;
    if end <= start {
        return None;
    }
    text.get(start..=end)
}

/// Every balanced `{...}` substring, innermost first, skipping braces in strings.
fn balanced_objects(text: &str) -> Vec<&str> {
    let mut stack: Vec<usize> = Vec::new();
    let mut found = Vec::new();
    let mut in_string = false;
    let mut escaped = false;

    for (idx, ch) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '{' if !in_string => stack.push(idx),
            '}' if !in_string => {
                if let Some(start) = stack.pop() {
                    if let Some(slice) = text.get(start..=idx) {
                        found.push(slice);
                    }
                }
            }
            _ => {}
        }
    }

    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_well_formed_batch() {
        let parsed = parse_judge_response(
            r#"{"verdicts":[
                {"id":1,"verdict":"supported","quote":"yielded 42% more fruit"},
                {"id":2,"verdict":"contradicted","quote":""},
                {"id":3,"verdict":"unsupported","quote":""}
            ]}"#,
        );

        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].id, Some(1));
        assert_eq!(
            parse_verdict(parsed[0].verdict.as_deref().unwrap()),
            Some(ClaimVerdict::Supported)
        );
        assert_eq!(
            parse_verdict(parsed[1].verdict.as_deref().unwrap()),
            Some(ClaimVerdict::Contradicted)
        );
        assert_eq!(
            parse_verdict(parsed[2].verdict.as_deref().unwrap()),
            Some(ClaimVerdict::Unsupported)
        );
    }

    #[test]
    fn parses_fenced_json_with_surrounding_prose() {
        let parsed = parse_judge_response(
            "Here are my verdicts:\n```json\n{\"verdicts\":[{\"id\":2,\"verdict\":\"SUPPORTED\",\"quote\":\"x\"}]}\n```\nLet me know if you need more.",
        );

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].id, Some(2));
    }

    #[test]
    fn accepts_a_bare_array_and_common_key_aliases() {
        let parsed =
            parse_judge_response(r#"[{"claim_id":4,"label":"not_supported","evidence":"none"}]"#);

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].id, Some(4));
        assert_eq!(
            parse_verdict(parsed[0].verdict.as_deref().unwrap()),
            Some(ClaimVerdict::Unsupported)
        );
    }

    #[test]
    fn salvages_complete_objects_from_a_truncated_response() {
        let parsed = parse_judge_response(
            r#"{"verdicts":[{"id":1,"verdict":"supported","quote":"a"},{"id":2,"verdict":"contradicted","quote":"b"},{"id":3,"verd"#,
        );

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].id, Some(1));
        assert_eq!(parsed[1].id, Some(2));
    }

    #[test]
    fn garbage_yields_nothing_so_lexical_verdicts_stand() {
        assert!(parse_judge_response("I'm not able to judge these claims.").is_empty());
        assert!(parse_judge_response("").is_empty());
        assert!(parse_judge_response("{{{{").is_empty());
        assert!(parse_judge_response(r#"{"verdicts":[{"id":1,"verdict":"maybe"}]}"#).is_empty());
        assert!(parse_judge_response(r#"{"verdicts":[{"verdict":"supported"}]}"#).is_empty());
    }

    #[test]
    fn braces_inside_quoted_text_do_not_confuse_the_salvage_scan() {
        let parsed = parse_judge_response(
            r#"[{"id":1,"verdict":"supported","quote":"the set {a, b} was used"},{"id":2,"#,
        );
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].quote.as_deref(), Some("the set {a, b} was used"));
    }

    #[test]
    fn quotes_are_kept_only_when_they_appear_in_a_cited_passage() {
        let batch = BatchInput {
            claims: Vec::new(),
            table: vec![
                (1u32, "Treated plots yielded 42% more fruit.".to_string()),
                (2u32, "Control plots were left unirrigated.".to_string()),
            ],
        };

        assert_eq!(
            batch
                .verified_quote("yielded 42% more fruit", &[1])
                .as_deref(),
            Some("yielded 42% more fruit")
        );
        // Whitespace differences are tolerated; invented spans are not.
        assert_eq!(
            batch
                .verified_quote("yielded   42%   more fruit", &[1])
                .as_deref(),
            Some("yielded 42% more fruit")
        );
        assert!(batch
            .verified_quote("yields tripled in the third season", &[1])
            .is_none());
        // A span from a passage this claim never cited is not its evidence.
        assert!(batch.verified_quote("left unirrigated", &[1]).is_none());
        assert_eq!(
            batch.verified_quote("left unirrigated", &[2]).as_deref(),
            Some("left unirrigated")
        );
        assert!(batch.verified_quote("", &[1]).is_none());
        assert!(batch.verified_quote("anything", &[]).is_none());
    }

    #[test]
    fn passage_text_is_bounded() {
        let long = "word ".repeat(1000);
        let truncated = truncate_chars(&long, MAX_PASSAGE_CHARS);
        assert_eq!(truncated.chars().count(), MAX_PASSAGE_CHARS + 1);
        assert!(truncated.ends_with('…'));
    }
}
