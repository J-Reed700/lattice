//! LLM claim judge.
//!
//! The lexical pre-filter can only see shared vocabulary. This asks a model
//! whether the cited passages actually entail the claim, which is the only way
//! to catch a sentence that borrows a source's wording and inverts its meaning.
//!
//! One claim per request. The evidence goes first and the claim last, so
//! llama-server's prompt cache reuses a page's prefix across the claims that
//! cite it. The answer starts with a single label whose probability is read
//! from the first token's log-probabilities, followed by a bounded explanation
//! and an exact source quote. Failed, timed-out, or unreadable judgments return
//! no verdict; the verifier decides what an unreached claim reads as.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures::future::BoxFuture;
use futures::{FutureExt, StreamExt};
use tokio::time::Instant;
use tracing::{debug, warn};

use crate::application::contracts::settings::LLMVerificationSettingsDto;
use crate::application::ports::llm_port::SamplingOverride;
#[cfg(test)]
use crate::application::ports::llm_port::{CompletionInput, CompletionRequest};
use crate::application::ports::LLMPort;
#[cfg(test)]
use crate::application::services::claim_verification::{
    parse_judge_explanation, parse_verdict_word, verdict_from_logprobs,
};
use crate::application::services::claim_verification::{CheckPolicy, ClaimChecker};
pub(super) use crate::application::services::claim_verification::{
    ClaimEvidence, ClaimJudgment, MIN_VERDICT_CONFIDENCE,
};
use crate::features::qa::dto::SourceDto;
#[cfg(test)]
use crate::shared::error::Result;

use super::lexical::{best_sentence, best_windows, LexicalClaim, ScoredWindow};
#[cfg(test)]
use super::ClaimVerdict;

/// Wall-clock ceiling for all judging in one turn.
///
/// Verification runs after the answer is on screen, so this bounds background
/// work rather than the reader's wait. Ninety seconds of one-token calls, three
/// in flight, covers a long research answer on the local utility model.
pub(super) const DEFAULT_TIME_BUDGET: Duration = Duration::from_secs(90);

/// Claims judged side by side.
///
/// The local sidecar serves several slots over one KV cache (llama.cpp's
/// auto `n_parallel`, four on the pinned build), and decoding is memory-bound,
/// so three requests in flight finish well ahead of three in a row. A
/// single-slot remote server simply queues them, which costs nothing over the
/// sequential path.
pub(super) const MAX_CONCURRENT_CALLS: usize = 3;

/// Output room for the model's reasoning followed by the public verdict,
/// factual explanation, and exact source quote. Reasoning tokens count toward
/// the provider's output budget even though they are not shown in the verdict.
const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 4096;

/// Cited sources read per claim.
const MAX_SOURCES_PER_CLAIM: usize = 3;
/// Windows offered per cited source.
const WINDOWS_PER_SOURCE: usize = 3;
/// Ceiling on one claim's evidence. Three full windows fit; a fourth does not,
/// so a claim citing three pages is shown the best stretch of each rather than
/// three stretches of the first.
pub(super) const MAX_EVIDENCE_CHARS: usize = 4_000;
const MAX_QUOTE_CHARS: usize = 400;

/// Between two windows of one page: the text in between was left out.
const WINDOW_SEPARATOR: &str = "\n[…]\n";

/// Whether a claim has anything to be judged against.
#[derive(Debug, Clone)]
pub(super) enum Evidence {
    Found(ClaimEvidence),
    /// Every citation resolves, but no cited source has any text: the page was
    /// never archived or came back empty. Nothing was checked.
    NoText,
    /// No citation, or one that points at nothing. A claim with no evidence is
    /// not an unchecked claim: it has nothing behind it.
    NoCitation,
}

pub(super) struct ClaimJudge {
    llm: Arc<dyn LLMPort>,
    time_budget: Duration,
    concurrency: usize,
    /// How the verdict is decoded. Greedy unless the user says otherwise.
    sampling: SamplingOverride,
    max_output_tokens: u32,
}

impl ClaimJudge {
    pub(super) fn new(llm: Arc<dyn LLMPort>) -> Self {
        Self {
            llm,
            time_budget: DEFAULT_TIME_BUDGET,
            concurrency: MAX_CONCURRENT_CALLS,
            // Deterministic by default, so a judge built anywhere in the code
            // cannot quietly inherit a creative model's sampling: measured on
            // the bundled 9B at the chat default of 0.7, one claim against one
            // passage came back supported, unsupported and contradicted across
            // twenty-one runs of the same request.
            sampling: SamplingOverride::deterministic(),
            max_output_tokens: DEFAULT_MAX_OUTPUT_TOKENS,
        }
    }

    /// Apply the user's sampling settings while retaining enough output room
    /// for reasoning, a verdict, and its supporting explanation.
    pub(super) fn with_tuning(mut self, tuning: &LLMVerificationSettingsDto) -> Self {
        self.sampling = SamplingOverride {
            temperature: Some(tuning.temperature),
            top_p: Some(tuning.top_p),
            top_k: Some(tuning.top_k),
        };
        self
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn with_time_budget(mut self, budget: Duration) -> Self {
        self.time_budget = budget;
        self
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = concurrency.max(1);
        self
    }

    pub(super) fn model_name(&self) -> &str {
        self.llm.model_name()
    }

    /// Judge each `(claim index, claim text, evidence)`, keyed back by index.
    ///
    /// Every request gets an entry. Claims run side by side against one
    /// deadline; one that finds no time left when its turn comes is skipped
    /// rather than started, so a slow model never overruns by a whole call.
    pub(super) async fn judge_claims(
        &self,
        requests: &[(usize, &str, ClaimEvidence)],
    ) -> HashMap<usize, ClaimJudgment> {
        let mut judgments = HashMap::new();
        if requests.is_empty() {
            return judgments;
        }

        let deadline = Instant::now() + self.time_budget;
        // Boxed: the borrowed `async fn` futures otherwise trip the compiler's
        // higher-ranked `Send` check once the whole turn is spawned.
        let calls: Vec<BoxFuture<'_, (usize, ClaimJudgment)>> = requests
            .iter()
            .map(|(index, claim, evidence)| {
                async move { (*index, self.judge_one(claim, evidence, deadline).await) }.boxed()
            })
            .collect();
        let mut calls = futures::stream::iter(calls).buffer_unordered(self.concurrency);
        while let Some((index, judgment)) = calls.next().await {
            judgments.insert(index, judgment);
        }

        let judged = judgments
            .values()
            .filter(|judgment| matches!(judgment, ClaimJudgment::Judged(_)))
            .count();
        let out_of_time = judgments
            .values()
            .filter(|judgment| matches!(judgment, ClaimJudgment::OutOfTime))
            .count();
        if out_of_time > 0 {
            warn!(
                judged,
                out_of_time,
                budget_ms = self.time_budget.as_millis(),
                "Claim judge time budget exhausted — remaining claims were not checked"
            );
        }
        debug!(
            judged,
            requested = requests.len(),
            model = self.llm.model_name(),
            "Claim judge finished"
        );
        judgments
    }

    async fn judge_one(
        &self,
        claim: &str,
        evidence: &ClaimEvidence,
        deadline: Instant,
    ) -> ClaimJudgment {
        ClaimChecker::new(
            self.llm.as_ref(),
            self.sampling,
            self.max_output_tokens,
            CheckPolicy::Chat,
        )
        .check(claim, evidence, deadline)
        .await
    }
}

/// The evidence one claim is judged against: up to [`WINDOWS_PER_SOURCE`]
/// windows of each cited source, the best of every source taken before the
/// second-best of any, inside [`MAX_EVIDENCE_CHARS`].
///
/// Only the passages the claim cites. A sentence that cites nothing has no
/// evidence; offering it whichever sources ranked first would make the judge
/// rule on text the sentence never made a statement about.
pub(super) fn evidence_for(claim: &LexicalClaim, sources: &[SourceDto]) -> Evidence {
    if claim.citation_ids.is_empty() || claim.citation_ids.len() != claim.cited_source_indices.len()
    {
        return Evidence::NoCitation;
    }

    // (citation number, windows best first) per cited source with any text.
    let mut per_source: Vec<(u32, Vec<ScoredWindow>)> = Vec::new();
    for &idx in claim
        .cited_source_indices
        .iter()
        .take(MAX_SOURCES_PER_CLAIM)
    {
        let Some(source) = sources.get(idx) else {
            return Evidence::NoCitation;
        };
        let body = if source.content.trim().is_empty() {
            source.excerpt.as_deref().unwrap_or_default()
        } else {
            source.content.as_str()
        };
        if body.trim().is_empty() {
            continue;
        }
        let windows = best_windows(&claim.claim_text, body, WINDOWS_PER_SOURCE);
        if windows.is_empty() {
            continue;
        }
        let citation_id = source
            .citation_id
            .unwrap_or_else(|| u32::try_from(idx + 1).unwrap_or(u32::MAX));
        per_source.push((citation_id, windows));
    }
    if per_source.is_empty() {
        return Evidence::NoText;
    }

    let mut chosen: Vec<Vec<&ScoredWindow>> = vec![Vec::new(); per_source.len()];
    let mut used = 0usize;
    'ranks: for rank in 0..WINDOWS_PER_SOURCE {
        for (slot, (_, windows)) in per_source.iter().enumerate() {
            let Some(window) = windows.get(rank) else {
                continue;
            };
            let cost = window.text.chars().count();
            // The first window always goes in: a claim is never judged
            // against nothing because one window was long.
            if used > 0 && used + cost > MAX_EVIDENCE_CHARS {
                break 'ranks;
            }
            used += cost;
            if let Some(list) = chosen.get_mut(slot) {
                list.push(window);
            }
        }
    }

    let mut blocks = Vec::new();
    for ((citation_id, _), mut windows) in per_source.iter().zip(chosen) {
        if windows.is_empty() {
            continue;
        }
        // Reading order, so a page's argument runs the way it was written.
        windows.sort_by_key(|window| window.index);
        let joined: Vec<&str> = windows.iter().map(|window| window.text.as_str()).collect();
        blocks.push(format!(
            "[{citation_id}]\n{}",
            joined.join(WINDOW_SEPARATOR)
        ));
    }

    let quote = per_source
        .iter()
        .filter_map(|(_, windows)| windows.first())
        .max_by_key(|window| window.score)
        .and_then(|window| best_sentence(&claim.claim_text, &window.text))
        .map(|sentence| truncate_chars(&sentence, MAX_QUOTE_CHARS));

    Evidence::Found(ClaimEvidence {
        text: blocks.join("\n\n"),
        quote,
    })
}

fn truncate_chars(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut out: String = text.chars().take(limit).collect();
    out.push('…');
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
mod tests {
    use super::super::lexical::lexical_pass;
    use super::super::test_support::source;
    use super::*;
    use crate::application::ports::llm_port::CompletionResponse;

    fn found(evidence: Evidence) -> ClaimEvidence {
        match evidence {
            Evidence::Found(evidence) => evidence,
            other => panic!("expected evidence, got {other:?}"),
        }
    }

    #[test]
    fn logprobs_name_the_verdict_and_its_probability() {
        let (verdict, p) = verdict_from_logprobs(&[
            ("supported".into(), (0.7f32).ln()),
            (" uns".into(), (0.2f32).ln()),
            ("contr".into(), (0.05f32).ln()),
            ("The".into(), (0.05f32).ln()),
        ])
        .unwrap();
        assert_eq!(verdict, ClaimVerdict::Supported);
        // Normalised over the three labels: 0.7 / 0.95.
        assert!((p - 0.7 / 0.95).abs() < 1e-4, "{p}");

        // Split spellings of one label add up.
        let (verdict, _) = verdict_from_logprobs(&[
            ("un".into(), (0.3f32).ln()),
            ("uns".into(), (0.3f32).ln()),
            ("supported".into(), (0.4f32).ln()),
        ])
        .unwrap();
        assert_eq!(verdict, ClaimVerdict::Unsupported);
    }

    #[test]
    fn a_reply_that_does_not_open_with_a_label_is_read_as_text() {
        assert!(verdict_from_logprobs(&[
            ("The".into(), (0.9f32).ln()),
            ("supported".into(), (0.1f32).ln()),
        ])
        .is_none());
        assert!(verdict_from_logprobs(&[("so".into(), -0.1)]).is_none());
        assert!(verdict_from_logprobs(&[]).is_none());
    }

    #[test]
    fn reply_words_parse_to_verdicts() {
        assert_eq!(
            parse_verdict_word("Supported."),
            Some(ClaimVerdict::Supported)
        );
        assert_eq!(
            parse_verdict_word("**contradicted**"),
            Some(ClaimVerdict::Contradicted)
        );
        assert_eq!(
            parse_verdict_word(" unsupported"),
            Some(ClaimVerdict::Unsupported)
        );
        assert_eq!(
            parse_verdict_word("Not supported"),
            Some(ClaimVerdict::Unsupported)
        );
        assert_eq!(parse_verdict_word("I cannot tell."), None);
        assert_eq!(parse_verdict_word(""), None);
    }

    #[test]
    fn explanation_keeps_the_reason_and_only_accepts_an_exact_source_quote() {
        let evidence = ClaimEvidence {
            text:
                "[1]\nThe source says the treatment reduced measured cold tolerance by 12 percent."
                    .into(),
            quote: Some(
                "The source says the treatment reduced measured cold tolerance by 12 percent."
                    .into(),
            ),
        };
        let parsed = parse_judge_explanation(
            "contradicted\nReason: The claim says improved; the source says reduced.\nSource quote: \"The source says the treatment reduced measured cold tolerance by 12 percent.\"",
            &evidence,
            ClaimVerdict::Contradicted,
        );

        assert_eq!(
            parsed.reason.as_deref(),
            Some("The claim says improved; the source says reduced.")
        );
        assert_eq!(parsed.quote, evidence.quote);

        let rejected = parse_judge_explanation(
            "contradicted\nReason: The claim says improved; the source says reduced.\nSource quote: The source says the treatment improved cold tolerance.",
            &evidence,
            ClaimVerdict::Contradicted,
        );
        assert_eq!(rejected.quote, None);
    }

    /// The case that kept failing: one page says "6-8 hours" around char
    /// 10,000 and "south-facing" around char 11,900. A sentence joining the two
    /// must be judged against both, not against whichever window scored best.
    #[test]
    fn a_claim_joining_two_far_apart_facts_is_shown_both() {
        let filler = |topic: &str, chars: usize| {
            let sentence = format!("Unrelated notes on {topic} and seasonal chores. ");
            sentence.repeat(chars / sentence.len() + 1)
        };
        let page = format!(
            "{}Tomatoes need 6-8 hours of direct sunlight each day. {}A south-facing bed gives tomatoes the most light. {}",
            filler("compost", 10_144),
            filler("trellis", 1_700),
            filler("mulch", 3_000),
        );
        assert!(page.find("6-8 hours").unwrap() > 10_000);
        assert!(page.find("south-facing").unwrap() > 11_800);

        let sources = [source(&page)];
        let claims = lexical_pass(
            "Tomatoes need 6-8 hours of direct sunlight, so plant them in a south-facing bed [1].",
            &sources,
        );
        let evidence = found(evidence_for(&claims[0], &sources));

        assert!(
            evidence.text.contains("6-8 hours of direct sunlight"),
            "{}",
            evidence.text
        );
        assert!(
            evidence.text.contains("south-facing bed"),
            "{}",
            evidence.text
        );
        assert!(evidence.text.chars().count() <= MAX_EVIDENCE_CHARS + 16);
        assert!(evidence.text.starts_with("[1]\n"));
        assert!(evidence.quote.is_some());
    }

    #[test]
    fn several_cited_pages_each_get_their_best_window_inside_the_cap() {
        let page = |fact: &str| {
            format!(
                "{}{fact} {}",
                "Background prose about gardens in general. ".repeat(60),
                "Closing prose about harvest festivals. ".repeat(60),
            )
        };
        let mut a = source(&page("Basil wilts below ten degrees."));
        a.citation_id = Some(1);
        let mut b = source(&page("Basil prefers well drained soil."));
        b.citation_id = Some(2);
        let mut c = source(&page("Basil flowers in late summer."));
        c.citation_id = Some(3);
        let sources = [a, b, c];
        let claims = lexical_pass(
            "Basil wilts below ten degrees, prefers well drained soil and flowers in late summer [1][2][3].",
            &sources,
        );
        let evidence = found(evidence_for(&claims[0], &sources));
        for fact in [
            "wilts below ten",
            "well drained soil",
            "flowers in late summer",
        ] {
            assert!(evidence.text.contains(fact), "missing {fact}");
        }
        assert!(evidence.text.chars().count() <= MAX_EVIDENCE_CHARS + 32);
    }

    #[test]
    fn a_claim_that_cites_nothing_is_offered_no_evidence() {
        let sources = [
            source("Sweet potato vines can be kept alive as a perennial indoors."),
            source("The ideal initial planting depth for potatoes is 4 to 6 inches."),
        ];
        let claims = lexical_pass(
            "So: fully buried at first, then buried deeper as the plant grows.",
            &sources,
        );
        let claim = claims.first().expect("the uncited sentence is a claim");
        assert!(claim.citation_ids.is_empty());
        assert!(matches!(
            evidence_for(claim, &sources),
            Evidence::NoCitation
        ));
    }

    #[test]
    fn a_cited_page_with_no_text_is_not_evidence_of_anything() {
        let sources = [source("")];
        let claims = lexical_pass(
            "The ideal initial planting depth is 4 to 6 inches of soil above the seed potato [1].",
            &sources,
        );
        assert!(matches!(
            evidence_for(&claims[0], &sources),
            Evidence::NoText
        ));
    }

    /// Records the request the judge sends and answers with fixed logprobs.
    struct RecordingLlm {
        seen: std::sync::Mutex<Vec<CompletionRequest>>,
        logprobs: Option<Vec<(String, f32)>>,
        reply: String,
    }

    impl RecordingLlm {
        fn new(logprobs: Option<Vec<(String, f32)>>) -> Arc<Self> {
            Arc::new(Self {
                seen: std::sync::Mutex::new(Vec::new()),
                logprobs,
                reply: "supported".to_string(),
            })
        }

        fn with_reply(mut self: Arc<Self>, reply: &str) -> Arc<Self> {
            Arc::get_mut(&mut self)
                .expect("recording mock is not shared before configuration")
                .reply = reply.to_string();
            self
        }
    }

    #[async_trait::async_trait]
    impl LLMPort for RecordingLlm {
        async fn generate(
            &self,
            _prompt: &str,
            _context: &[String],
            _images: Option<Vec<String>>,
        ) -> Result<String> {
            unreachable!("this mock supports typed completions")
        }

        async fn generate_streaming(
            &self,
            _prompt: &str,
            _context: &[String],
            _images: Option<Vec<String>>,
        ) -> Result<Box<dyn futures::Stream<Item = Result<String>> + Send + Unpin + '_>> {
            unimplemented!("streaming is not used by the claim judge")
        }

        fn supports_typed_completions(&self) -> bool {
            true
        }

        async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
            self.seen.lock().unwrap().push(request.clone());
            Ok(CompletionResponse {
                text: self.reply.clone(),
                first_token_logprobs: self.logprobs.clone(),
                ..Default::default()
            })
        }

        fn model_name(&self) -> &str {
            "recording-judge"
        }

        fn max_context_tokens(&self) -> usize {
            8192
        }

        fn count_tokens(&self, text: &str) -> usize {
            text.len() / 4
        }

        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }

    fn one_request() -> (Vec<LexicalClaim>, [SourceDto; 1]) {
        let sources = [source(
            "The ideal initial planting depth for potatoes is 4 to 6 inches of soil cover.",
        )];
        let claims = lexical_pass(
            "The ideal initial planting depth is 4 to 6 inches of soil above the seed potato [1].",
            &sources,
        );
        (claims, sources)
    }

    async fn judge_first(judge: &ClaimJudge) -> HashMap<usize, ClaimJudgment> {
        let (claims, sources) = one_request();
        let evidence = found(evidence_for(&claims[0], &sources));
        judge
            .judge_claims(&[(0, claims[0].claim_text.as_str(), evidence)])
            .await
    }

    #[tokio::test]
    async fn the_judge_starts_with_a_label_and_puts_document_first() {
        let llm = RecordingLlm::new(None);
        judge_first(&ClaimJudge::new(Arc::clone(&llm) as Arc<dyn LLMPort>)).await;

        let sent = llm.seen.lock().unwrap().clone();
        assert_eq!(sent.len(), 1);
        let request = &sent[0];
        assert!(request.want_logprobs);
        assert!(request.json_schema.is_none());
        assert!(request.reasoning_effort.is_none());
        assert!(!request.no_time_limit);
        // A verdict is a classification. Sampling one from the chat model's
        // distribution made the same claim against the same passage come back
        // supported, unsupported and contradicted across repeats of one request;
        // the explanation follows the first label without changing that signal.
        let sampling = request.sampling.expect("the judge sets its own sampling");
        assert_eq!(sampling.temperature, Some(0.0));
        assert_eq!(sampling.top_k, Some(1));
        assert_eq!(request.max_output_tokens, Some(DEFAULT_MAX_OUTPUT_TOKENS));

        let CompletionInput::Message { content, .. } = &request.input[1] else {
            panic!("user message expected");
        };
        let document = content.find("4 to 6 inches of soil cover").unwrap();
        let claim = content.find("Claim:").unwrap();
        assert!(document < claim, "document first, claim last");
    }

    #[tokio::test]
    async fn verification_settings_reach_the_request() {
        let llm = RecordingLlm::new(None);
        let judge = ClaimJudge::new(Arc::clone(&llm) as Arc<dyn LLMPort>).with_tuning(
            &LLMVerificationSettingsDto {
                enabled: true,
                temperature: 0.4,
                top_p: 0.8,
                top_k: 20,
            },
        );
        judge_first(&judge).await;

        let sent = llm.seen.lock().unwrap().clone();
        let sampling = sent[0].sampling.expect("the judge sets its own sampling");
        assert_eq!(sampling.temperature, Some(0.4));
        assert_eq!(sampling.top_p, Some(0.8));
        assert_eq!(sampling.top_k, Some(20));
        assert_eq!(sent[0].max_output_tokens, Some(DEFAULT_MAX_OUTPUT_TOKENS));
    }

    #[tokio::test]
    async fn the_verdict_carries_its_probability_and_a_quote_from_the_page() {
        let llm = RecordingLlm::new(Some(vec![
            ("contr".into(), (0.8f32).ln()),
            ("supported".into(), (0.2f32).ln()),
        ]))
        .with_reply("contradicted\nReason: The claim says one thing; the source says another.");
        let judgments = judge_first(&ClaimJudge::new(llm as Arc<dyn LLMPort>)).await;

        let Some(ClaimJudgment::Judged(outcome)) = judgments.get(&0) else {
            panic!("judged: {judgments:?}");
        };
        // The logprobs outrank the reply text, which said "supported".
        assert_eq!(outcome.verdict, ClaimVerdict::Contradicted);
        assert!((outcome.confidence.unwrap() - 0.8).abs() < 1e-4);
        assert!(outcome
            .quote
            .as_deref()
            .unwrap()
            .contains("4 to 6 inches of soil cover"));
    }
}
