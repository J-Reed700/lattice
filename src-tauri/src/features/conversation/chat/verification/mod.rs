//! Response grounding verification.
//!
//! Two passes. The lexical pre-filter ([`lexical`]) scores every claim sentence
//! against the passages it cites — cheap, deterministic, and run on every turn.
//! The claim judge ([`judge`]) then asks a model about the sentences overlap
//! cannot settle: the ambiguous band just above threshold, anything unsupported,
//! and anything carrying a number, date or negation, where matching vocabulary
//! and opposite meaning look identical to a token counter.
//!
//! The judge is an improvement on the lexical verdict, never a precondition
//! for one. An escalated claim the judge never reached — the budget ran out,
//! the call failed — keeps its lexical verdict, except one carrying a number,
//! date or negation: overlap cannot settle those, so it reads as unverified
//! rather than borrowing a positive verdict from shared vocabulary. Unverified
//! is its own state and never counts as unsupported: "not checked" and "not
//! found in the source" are different findings. With no judge configured,
//! reports remain explicitly lexical.

use std::sync::Arc;

use tracing::{debug, info};

use crate::application::contracts::settings::LLMVerificationSettingsDto;
use crate::application::ports::LLMPort;
use crate::features::qa::dto::SourceDto;

mod background;
mod judge;
mod lexical;

pub use self::background::VerificationReadyDto;
pub(super) use self::background::{pending_metadata, BackgroundVerification};

use self::judge::{evidence_for, ClaimJudge, ClaimJudgment, Evidence, MIN_VERDICT_CONFIDENCE};
use self::lexical::LexicalClaim;

use crate::application::services::claim_verification::ClaimVerdict;

impl ClaimVerdict {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::Contradicted => "contradicted",
            Self::Unsupported => "unsupported",
            Self::Unverified => "unverified",
        }
    }
}

/// Why a claim was left unverified, so the UI can say which.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UnverifiedReason {
    /// The judge's time budget ran out before it reached the claim.
    Budget,
    /// The judge was asked and returned nothing usable.
    JudgeFailed,
    /// The cited source has no archived text to check against.
    NoText,
    /// The judge returned a label, but its probability was too low to present
    /// that label as a finding.
    LowConfidence,
}

impl UnverifiedReason {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Budget => "budget",
            Self::JudgeFailed => "judge_failed",
            Self::NoText => "no_text",
            Self::LowConfidence => "low_confidence",
        }
    }
}

/// Which pass produced a verdict. Surfaced so the UI can say how a claim was
/// checked rather than implying every verdict carries the same weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VerificationMethod {
    Lexical,
    Judge,
}

impl VerificationMethod {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Lexical => "lexical",
            Self::Judge => "judge",
        }
    }
}

/// One claim sentence and what was concluded about it.
#[derive(Debug, Clone)]
pub(super) struct ClaimAssessment {
    /// The sentence as written, citation markers included.
    pub(super) sentence: String,
    /// Citation numbers the sentence carries, as the model wrote them.
    pub(super) citation_ids: Vec<u32>,
    pub(super) verdict: ClaimVerdict,
    /// A span from a cited passage, verified to actually appear in it.
    pub(super) evidence_quote: Option<String>,
    pub(super) method: VerificationMethod,
    /// The judge's probability for `verdict`, when the provider reports one.
    pub(super) confidence: Option<f32>,
    /// A short factual comparison supplied by the judge, when available.
    pub(super) reason: Option<String>,
    /// Set exactly when `verdict` is `Unverified`.
    pub(super) unverified_reason: Option<UnverifiedReason>,
}

impl ClaimAssessment {
    fn from_lexical(claim: &LexicalClaim) -> Self {
        Self {
            sentence: claim.sentence.clone(),
            citation_ids: claim.citation_ids.clone(),
            verdict: if claim.supported {
                ClaimVerdict::Supported
            } else {
                ClaimVerdict::Unsupported
            },
            evidence_quote: None,
            method: VerificationMethod::Lexical,
            confidence: None,
            reason: None,
            unverified_reason: None,
        }
    }

    fn mark_unverified(&mut self, reason: UnverifiedReason) {
        self.verdict = ClaimVerdict::Unverified;
        self.unverified_reason = Some(reason);
        self.evidence_quote = None;
        self.reason = None;
    }
}

/// The limits the UI's summary schema (`src/types/conversation.ts`) enforces.
/// It discards the whole summary on any violation, so everything emitted is
/// cut to fit here instead; the counts stay the true totals.
const MAX_EMITTED_VERDICTS: usize = 60;
const MAX_SUPPORTED_NOTES: usize = 40;
const MAX_LISTED_CLAIMS: usize = 20;
const MAX_SENTENCE_UNITS: usize = 2000;
const MAX_QUOTE_UNITS: usize = 1000;
const MAX_CLAIM_CITATIONS: usize = 24;
const CITATION_ID_RANGE: std::ops::RangeInclusive<u32> = 1..=1000;

/// Cut to at most `max` UTF-16 code units — the unit a JavaScript string's
/// length counts — without splitting a character.
fn fit_utf16(text: &str, max: usize) -> String {
    let mut units = 0;
    text.chars()
        .take_while(|c| {
            units += c.len_utf16();
            units <= max
        })
        .collect()
}

fn fit_sentences(sentences: &[String], max: usize) -> Vec<String> {
    sentences
        .iter()
        .take(max)
        .map(|sentence| fit_utf16(sentence, MAX_SENTENCE_UNITS))
        .collect()
}

#[derive(Debug, Clone, Default)]
pub(super) struct GroundingReport {
    pub(super) claims_evaluated: usize,
    pub(super) supported_claims: usize,
    pub(super) supported_claim_notes: Vec<String>,
    /// Everything not supported — contradicted claims included, because the
    /// existing badge counts this list and a contradiction is the worst case.
    pub(super) unsupported_claims: Vec<String>,
    pub(super) contradicted_claims: Vec<String>,
    /// Claims nothing checked. Not in `unsupported_claims`.
    pub(super) unverified_claims: usize,
    pub(super) claims: Vec<ClaimAssessment>,
    pub(super) judge_used: bool,
}

impl GroundingReport {
    fn from_assessments(claims: Vec<ClaimAssessment>, judge_used: bool) -> Self {
        let mut report = Self {
            claims_evaluated: claims.len(),
            judge_used,
            ..Self::default()
        };

        for claim in &claims {
            match claim.verdict {
                ClaimVerdict::Supported => {
                    report.supported_claims += 1;
                    report.supported_claim_notes.push(claim.sentence.clone());
                }
                ClaimVerdict::Contradicted => {
                    report.contradicted_claims.push(claim.sentence.clone());
                    report.unsupported_claims.push(claim.sentence.clone());
                }
                ClaimVerdict::Unsupported => {
                    report.unsupported_claims.push(claim.sentence.clone());
                }
                ClaimVerdict::Unverified => report.unverified_claims += 1,
            }
        }

        report.claims = claims;
        report
    }

    pub(super) fn unsupported_count(&self) -> usize {
        self.unsupported_claims.len()
    }

    pub(super) fn contradicted_count(&self) -> usize {
        self.contradicted_claims.len()
    }

    pub(super) fn judged_claim_count(&self) -> usize {
        self.claims
            .iter()
            .filter(|claim| claim.method == VerificationMethod::Judge)
            .count()
    }

    /// Supported over checked. An unchecked claim is neither grounded nor
    /// ungrounded, so it is left out of both sides; nothing checked is 1.0 in
    /// the same sense that nothing to check is.
    pub(super) fn grounded_ratio(&self) -> f32 {
        let checked = self.claims_evaluated.saturating_sub(self.unverified_claims);
        if checked == 0 {
            return 1.0;
        }
        self.supported_claims as f32 / checked as f32
    }

    /// What the "Checking the answer" step says it found.
    ///
    /// An answer with nothing to check is not an answer that failed its check,
    /// so it says so in those words rather than reporting "0 of 0 backed".
    pub(super) fn result_line(&self) -> String {
        if self.claims_evaluated == 0 {
            return "nothing to check against sources".to_string();
        }
        let backed = format!(
            "{} of {} claims backed",
            self.supported_claims, self.claims_evaluated
        );
        if self.unverified_claims == 0 {
            backed
        } else {
            format!("{backed}, {} not checked", self.unverified_claims)
        }
    }

    /// The metadata blob persisted with the assistant message.
    ///
    /// Additive: every key the UI already reads keeps its name and meaning.
    pub(super) fn metadata_json(&self) -> serde_json::Value {
        let verdicts: Vec<serde_json::Value> = self
            .claims
            .iter()
            .take(MAX_EMITTED_VERDICTS)
            .map(|claim| {
                let citation_ids: Vec<u32> = claim
                    .citation_ids
                    .iter()
                    .copied()
                    .filter(|id| CITATION_ID_RANGE.contains(id))
                    .take(MAX_CLAIM_CITATIONS)
                    .collect();
                serde_json::json!({
                    "sentence": fit_utf16(&claim.sentence, MAX_SENTENCE_UNITS),
                    "citationIds": citation_ids,
                    "verdict": claim.verdict.as_str(),
                    "evidenceQuote": claim
                        .evidence_quote
                        .as_deref()
                        .map(|quote| fit_utf16(quote, MAX_QUOTE_UNITS)),
                    "method": claim.method.as_str(),
                    "confidence": claim.confidence,
                    "reason": claim
                        .reason
                        .as_deref()
                        .map(|reason| fit_utf16(reason, MAX_QUOTE_UNITS)),
                    "unverifiedReason": claim.unverified_reason.map(UnverifiedReason::as_str),
                })
            })
            .collect();

        serde_json::json!({
            "enabled": true,
            "claimsEvaluated": self.claims_evaluated,
            "supportedClaims": self.supported_claims,
            "supportedClaimNotes": fit_sentences(&self.supported_claim_notes, MAX_SUPPORTED_NOTES),
            "unsupportedClaims": fit_sentences(&self.unsupported_claims, MAX_LISTED_CLAIMS),
            "groundedRatio": self.grounded_ratio(),
            "contradictedClaims": fit_sentences(&self.contradicted_claims, MAX_LISTED_CLAIMS),
            "verdictCounts": {
                "supported": self.supported_claims,
                "contradicted": self.contradicted_claims.len(),
                "unsupported": self.claims_evaluated
                    .saturating_sub(self.supported_claims)
                    .saturating_sub(self.contradicted_claims.len())
                    .saturating_sub(self.unverified_claims),
                "unverified": self.unverified_claims,
            },
            "claimVerdicts": verdicts,
            "judgeUsed": self.judge_used,
        })
    }
}

/// Runs the grounding passes for one turn.
pub(super) struct GroundingVerifier {
    judge: Option<ClaimJudge>,
}

impl GroundingVerifier {
    /// The judge is on whenever a handle exists; without one the turn is
    /// verified lexically and says so in the log.
    pub(super) fn new(llm: Option<Arc<dyn LLMPort>>) -> Self {
        match llm {
            Some(llm) => {
                let judge = ClaimJudge::new(llm);
                // Debug, not info: this is the normal path and fires every turn.
                // Only the degraded paths below are worth a line in the log.
                debug!(
                    model = judge.model_name(),
                    "Grounding claim judge enabled for this turn"
                );
                Self { judge: Some(judge) }
            }
            None => {
                info!(
                    "Grounding claim judge skipped: no LLM handle available — claim verdicts are \
                     lexical overlap only and cannot detect a contradiction that reuses the \
                     source's wording"
                );
                Self { judge: None }
            }
        }
    }

    /// Opt out explicitly. Logs, because a silently lexical-only verdict is
    /// indistinguishable from a judged one in the persisted metadata.
    ///
    /// No production caller turns the judge off today — `new(None)` covers the
    /// "no handle" case — but the knob is the one place to route a future
    /// setting through, and tests use it to pin the lexical-only behaviour.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn with_judge_enabled(self, enabled: bool) -> Self {
        if enabled {
            return self;
        }
        if self.judge.is_some() {
            info!(
                "Grounding claim judge skipped: disabled by configuration — claim verdicts are \
                 lexical overlap only"
            );
        }
        Self { judge: None }
    }

    /// Apply the user's verification settings to the judge, if there is one.
    ///
    /// Without this the judge decodes greedily, which is the right default; the
    /// settings exist so someone can trade that determinism away deliberately
    /// rather than inheriting it from whatever the chat model is tuned to.
    pub(super) fn with_tuning(mut self, tuning: &LLMVerificationSettingsDto) -> Self {
        self.judge = self.judge.map(|judge| judge.with_tuning(tuning));
        self
    }

    pub(super) async fn verify(&self, response: &str, sources: &[SourceDto]) -> GroundingReport {
        let claims = lexical::lexical_pass(response, sources);
        if claims.is_empty() {
            return GroundingReport::default();
        }

        let mut assessments: Vec<ClaimAssessment> =
            claims.iter().map(ClaimAssessment::from_lexical).collect();

        let Some(judge) = self.judge.as_ref() else {
            return GroundingReport::from_assessments(assessments, false);
        };

        // What each escalated claim can be judged against. A cited page with
        // no text is unverified whatever the lexical pass made of it: there
        // was nothing to find the claim in, so "not found" would be false.
        let mut requests = Vec::new();
        for (index, claim) in claims.iter().enumerate() {
            if !lexical::needs_judge(claim) {
                continue;
            }
            match evidence_for(claim, sources) {
                Evidence::Found(evidence) => {
                    requests.push((index, claim.claim_text.as_str(), evidence));
                }
                Evidence::NoText => {
                    if let Some(assessment) = assessments.get_mut(index) {
                        assessment.mark_unverified(UnverifiedReason::NoText);
                    }
                }
                Evidence::NoCitation => {}
            }
        }
        if requests.is_empty() {
            return GroundingReport::from_assessments(assessments, false);
        }

        let judgments = judge.judge_claims(&requests).await;
        for (index, judgment) in judgments {
            let Some(assessment) = assessments.get_mut(index) else {
                continue;
            };
            let unreached = match judgment {
                ClaimJudgment::Judged(outcome) => {
                    assessment.confidence = outcome.confidence;
                    assessment.method = VerificationMethod::Judge;
                    if outcome
                        .confidence
                        .is_some_and(|confidence| confidence < MIN_VERDICT_CONFIDENCE)
                    {
                        let confidence = outcome.confidence.unwrap_or_default();
                        assessment.mark_unverified(UnverifiedReason::LowConfidence);
                        // `mark_unverified` keeps the lexical method for
                        // failed calls; this one did run, it simply did not
                        // earn the right to present a red/green finding.
                        assessment.method = VerificationMethod::Judge;
                        assessment.reason = Some(format!(
                            "The checker was only {:.0}% sure, so this sentence was not marked as a finding.",
                            confidence * 100.0
                        ));
                        continue;
                    }
                    assessment.verdict = outcome.verdict;
                    assessment.evidence_quote = outcome.quote;
                    assessment.reason = outcome.reason;
                    continue;
                }
                ClaimJudgment::OutOfTime => UnverifiedReason::Budget,
                ClaimJudgment::Unusable | ClaimJudgment::Failed(_) => UnverifiedReason::JudgeFailed,
            };
            // Unreached: the lexical verdict stands where overlap can speak
            // for the claim. Where it cannot — a number, a date, a negation —
            // a match certifies nothing, and a miss is not a finding either.
            if claims
                .get(index)
                .is_some_and(|claim| claim.contradiction_prone)
            {
                assessment.mark_unverified(unreached);
            }
        }

        let judge_used = assessments
            .iter()
            .any(|claim| claim.method == VerificationMethod::Judge);
        GroundingReport::from_assessments(assessments, judge_used)
    }
}

/// Lexical-only grounding summary.
///
/// The synchronous path, kept for callers that have no LLM handle to offer.
#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn verify_response_grounding_summary(
    response: &str,
    sources: &[SourceDto],
) -> GroundingReport {
    GroundingReport::from_assessments(
        lexical::lexical_pass(response, sources)
            .iter()
            .map(ClaimAssessment::from_lexical)
            .collect(),
        false,
    )
}

#[cfg(test)]
pub(super) mod test_support {
    use crate::features::qa::dto::SourceDto;

    pub(in crate::features::conversation::chat::verification) fn source(
        content: &str,
    ) -> SourceDto {
        SourceDto {
            page_number: None,
            document_id: "doc-1".to_string(),
            chunk_id: "chunk-1".to_string(),
            content: content.to_string(),
            score: 1.0,
            path: None,
            position: None,
            file_name: "doc.txt".to_string(),
            file_path: "/tmp/doc.txt".to_string(),
            mime_type: "text/plain".to_string(),
            category: "Text File".to_string(),
            file_size_bytes: 100,
            modified_at: "2026-01-01T00:00:00Z".to_string(),
            excerpt: None,
            highlights: None,
            section: None,
            chunk_index: None,
            chunk_excerpts: None,
            citation_id: None,

            web_snapshot: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use async_trait::async_trait;

    use super::test_support::source;
    use super::*;
    use crate::application::ports::llm_port::{CompletionRequest, CompletionResponse};
    use crate::shared::error::Result;

    /// Replies with a scripted body, after an optional delay, counting calls.
    struct ScriptedLlm {
        replies: Vec<String>,
        delay: Duration,
        calls: AtomicUsize,
        logprobs: Option<Vec<(String, f32)>>,
    }

    impl ScriptedLlm {
        fn new(replies: Vec<&str>) -> Self {
            Self {
                replies: replies.into_iter().map(str::to_string).collect(),
                delay: Duration::ZERO,
                calls: AtomicUsize::new(0),
                logprobs: None,
            }
        }

        fn with_delay(mut self, delay: Duration) -> Self {
            self.delay = delay;
            self
        }

        fn call_count(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }

        fn with_logprobs(mut self, logprobs: Vec<(String, f32)>) -> Self {
            self.logprobs = Some(logprobs);
            self
        }
    }

    #[async_trait]
    impl LLMPort for ScriptedLlm {
        async fn complete(&self, _request: &CompletionRequest) -> Result<CompletionResponse> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if !self.delay.is_zero() {
                tokio::time::sleep(self.delay).await;
            }
            Ok(CompletionResponse {
                text: self
                    .replies
                    .get(call)
                    .or_else(|| self.replies.last())
                    .cloned()
                    .unwrap_or_default(),
                first_token_logprobs: self.logprobs.clone(),
                ..Default::default()
            })
        }

        fn model_name(&self) -> &str {
            "scripted-judge"
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

    const CONTRADICTION_SOURCE: &str =
        "Across both seasons, salicylic acid treatment reduced measured cold tolerance in \
         blueberry plants by 12 percent relative to untreated controls.";

    const EXPLAINED_CONTRADICTION: &str =
        "contradicted\nReason: The claim says improved; the source says reduced.\nSource quote: Across both seasons, salicylic acid treatment reduced measured cold tolerance in blueberry plants by 12 percent relative to untreated controls.";

    /// Lexically indistinguishable from the source: same vocabulary, opposite claim.
    const CONTRADICTING_RESPONSE: &str =
        "Salicylic acid treatment improved measured cold tolerance in blueberry plants by 12 \
         percent relative to untreated controls [1].";

    #[test]
    fn lexical_only_summary_keeps_the_legacy_shape() {
        let report = verify_response_grounding_summary(
            "Blueberry plants showed improved cold tolerance after salicylic acid treatment [1].",
            &[source(
                "Salicylic acid treatment improved cold tolerance in blueberry plants during low temperature stress.",
            )],
        );

        assert_eq!(report.claims_evaluated, 1);
        assert_eq!(report.unsupported_count(), 0);
        assert_eq!(report.supported_claim_notes.len(), 1);
        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].method, VerificationMethod::Lexical);
        assert!(!report.judge_used);
    }

    #[test]
    fn unsupported_claim_is_still_reported_without_a_judge() {
        let report = verify_response_grounding_summary(
            "Blueberry plants tripled yield after lunar-cycle irrigation [1].",
            &[source(
                "The study measured antioxidant enzyme response under low-temperature stress.",
            )],
        );

        assert_eq!(report.claims_evaluated, 1);
        assert_eq!(report.unsupported_count(), 1);
        assert!(report.supported_claim_notes.is_empty());
    }

    #[test]
    fn unsupported_claim_note_keeps_full_sentence() {
        let report = verify_response_grounding_summary(
            "The same epigenetic shifts are linked to altered expression of genes that control meristem activity, suggesting a mechanistic link between methylation dynamics and developmental transitions [1][2].",
            &[source("The study discusses WOX family structure and stress response without this meristem claim.")],
        );

        assert_eq!(report.unsupported_count(), 1);
        assert!(report.unsupported_claims[0].contains(
            "The same epigenetic shifts are linked to altered expression of genes that control meristem activity"
        ));
        assert!(!report.unsupported_claims[0].contains("developmental tr..."));
    }

    #[tokio::test]
    async fn judge_overturns_a_lexically_supported_contradiction() {
        let sources = vec![source(CONTRADICTION_SOURCE)];

        let lexical = verify_response_grounding_summary(CONTRADICTING_RESPONSE, &sources);
        assert_eq!(
            lexical.supported_claims, 1,
            "the pre-filter cannot see the inversion"
        );

        let llm = Arc::new(ScriptedLlm::new(vec![EXPLAINED_CONTRADICTION]));
        let report = GroundingVerifier::new(Some(llm))
            .verify(CONTRADICTING_RESPONSE, &sources)
            .await;

        assert_eq!(report.claims_evaluated, 1);
        assert_eq!(report.supported_claims, 0);
        assert_eq!(report.contradicted_count(), 1);
        assert_eq!(report.unsupported_count(), 1, "contradictions stay flagged");
        assert_eq!(report.claims[0].method, VerificationMethod::Judge);
        // The quote is the page's own sentence, not one the model wrote.
        assert!(report.claims[0]
            .evidence_quote
            .as_deref()
            .unwrap()
            .contains("reduced measured cold tolerance in blueberry plants by 12 percent"));
        assert!(report.judge_used);
    }

    /// Opt-in: sends only synthetic claims through the production model adapter
    /// and grounding verifier. No library documents or conversation state used.
    #[tokio::test]
    #[ignore = "requires LATTICE_LLAMACPP_SETTINGS and a reachable model"]
    async fn live_grounding_adversarial_regressions() {
        use crate::application::contracts::settings::LLMSettingsDto;
        use crate::features::llm::llama_cpp::LlamaCppLlm;
        let settings: serde_json::Value = serde_json::from_slice(
            &std::fs::read(std::env::var("LATTICE_LLAMACPP_SETTINGS").unwrap()).unwrap(),
        )
        .unwrap();
        let config: LLMSettingsDto =
            serde_json::from_value(settings["settings"]["llm"].clone()).unwrap();
        let llm = Arc::new(LlamaCppLlm::new(&config).unwrap());
        let verifier = GroundingVerifier::new(Some(llm));
        let mut evidence = source(
            "The design workspace retains hourly file versions for 14 days before expiration.",
        );
        evidence.citation_id = Some(7);
        for (name, answer, expected, judged) in [
            ("supported", "The design workspace retains hourly file versions for 14 days before expiration [7].", ClaimVerdict::Supported, true),
            ("wrong_number", "The design workspace retains hourly file versions for 30 days before expiration [7].", ClaimVerdict::Contradicted, true),
            ("negation", "The design workspace does not retain hourly file versions for 14 days before expiration [7].", ClaimVerdict::Contradicted, true),
            ("invented_citation", "The design workspace retains hourly file versions for 14 days before expiration [99].", ClaimVerdict::Unsupported, false),
        ] {
            let report = verifier.verify(answer, &[evidence.clone()]).await;
            println!("{name}: {}", report.metadata_json());
            assert_eq!(report.claims_evaluated, 1, "{name}");
            assert_eq!(report.claims[0].verdict, expected, "{name}");
            assert_eq!(report.judge_used, judged, "{name}: a fallback is not a semantic judgment");
        }
    }

    #[tokio::test]
    async fn invalid_citation_is_not_rescued_by_the_judge() {
        let llm = Arc::new(ScriptedLlm::new(vec!["supported"]));
        let response = CONTRADICTING_RESPONSE.replace("[1]", "[99]");
        let report = GroundingVerifier::new(Some(Arc::clone(&llm) as Arc<dyn LLMPort>))
            .verify(&response, &[source(CONTRADICTION_SOURCE)])
            .await;
        assert_eq!(report.supported_claims, 0);
        assert_eq!(llm.call_count(), 0);
    }

    #[tokio::test]
    async fn garbage_judge_output_does_not_certify_an_unresolved_claim() {
        let sources = vec![source(CONTRADICTION_SOURCE)];
        let llm = Arc::new(ScriptedLlm::new(vec!["I cannot judge these claims."]));

        let report = GroundingVerifier::new(Some(Arc::clone(&llm) as Arc<dyn LLMPort>))
            .verify(CONTRADICTING_RESPONSE, &sources)
            .await;

        assert_eq!(llm.call_count(), 1);
        assert_eq!(report.supported_claims, 0);
        // Not checked is not "not found": the claim is neither certified by
        // overlap nor reported as missing from its source.
        assert_eq!(report.claims[0].verdict, ClaimVerdict::Unverified);
        assert_eq!(
            report.claims[0].unverified_reason,
            Some(UnverifiedReason::JudgeFailed)
        );
        assert_eq!(report.unsupported_count(), 0);
        assert_eq!(report.claims[0].method, VerificationMethod::Lexical);
        assert!(!report.judge_used);
    }

    #[tokio::test]
    async fn a_low_confidence_negative_is_not_presented_as_a_contradiction() {
        let llm = Arc::new(
            ScriptedLlm::new(vec![
                "contradicted\nReason: The claim says improved; the source says reduced.\nSource quote: Across both seasons, salicylic acid treatment reduced measured cold tolerance in blueberry plants by 12 percent relative to untreated controls.",
            ])
            .with_logprobs(vec![
                ("contradicted".into(), (0.46f32).ln()),
                ("supported".into(), (0.40f32).ln()),
                ("unsupported".into(), (0.14f32).ln()),
            ]),
        );
        let report = GroundingVerifier::new(Some(Arc::clone(&llm) as Arc<dyn LLMPort>))
            .verify(CONTRADICTING_RESPONSE, &[source(CONTRADICTION_SOURCE)])
            .await;

        assert_eq!(report.claims[0].verdict, ClaimVerdict::Unverified);
        assert_eq!(
            report.claims[0].unverified_reason,
            Some(UnverifiedReason::LowConfidence)
        );
        assert_eq!(report.claims[0].method, VerificationMethod::Judge);
        assert_eq!(report.contradicted_count(), 0);
        assert!(report.claims[0].reason.as_deref().unwrap().contains("46%"));
        assert_eq!(report.metadata_json()["verdictCounts"]["unverified"], 1);
    }

    #[tokio::test]
    async fn a_plain_strongly_supported_claim_never_reaches_the_judge() {
        // No digits, no negation, and near-total vocabulary overlap.
        let response = "Salicylic acid treatment improved cold tolerance in blueberry plants during low temperature stress [1].";
        let sources = vec![source(
            "Salicylic acid treatment improved cold tolerance in blueberry plants during low temperature stress.",
        )];
        let llm = Arc::new(ScriptedLlm::new(vec!["unsupported"]));

        let report = GroundingVerifier::new(Some(Arc::clone(&llm) as Arc<dyn LLMPort>))
            .verify(response, &sources)
            .await;

        assert_eq!(llm.call_count(), 0, "the judge must not be called");
        assert_eq!(report.supported_claims, 1);
        assert_eq!(report.claims[0].method, VerificationMethod::Lexical);
    }

    /// A judge over `llm`, behind a scheduler with `slots` slots: how many
    /// claims run side by side is the backend's decision, not the judge's.
    fn scheduled_judge(llm: Arc<ScriptedLlm>, slots: usize, budget: Duration) -> ClaimJudge {
        use crate::features::llm::scheduler::{BackendCapacity, InferenceScheduler, ScheduledLlm};
        let scheduler = Arc::new(InferenceScheduler::new(
            "judge-test",
            BackendCapacity::concurrent(slots),
        ));
        ClaimJudge::new(Arc::new(ScheduledLlm::new(llm, scheduler, 4096)) as Arc<dyn LLMPort>)
            .with_time_budget(budget)
    }

    /// Real sleeps rather than a paused clock. A one-slot backend serves one
    /// 200ms call inside a 300ms budget; the next is cut off when the budget
    /// ends and the last never starts. The judge's old one-call-at-a-time cap
    /// is now the backend's slot count, and the budget still bounds the whole.
    #[tokio::test]
    async fn time_budget_leaves_later_numeric_claims_unverified() {
        let response = "Yields rose by 42 percent in treated plots [1].\n\
                        Frost damage fell by 18 percent in treated plots [1].\n\
                        Harvest weight increased by 7 percent in treated plots [1].";
        let sources = vec![source(
            "Treated plots recorded changes in yields, frost damage, and harvest weight across the trial.",
        )];

        let llm = Arc::new(
            ScriptedLlm::new(vec![EXPLAINED_CONTRADICTION]).with_delay(Duration::from_millis(200)),
        );
        let verifier = GroundingVerifier {
            judge: Some(scheduled_judge(
                Arc::clone(&llm),
                1,
                Duration::from_millis(300),
            )),
        };

        let started = std::time::Instant::now();
        let report = verifier.verify(response, &sources).await;

        assert!(
            started.elapsed() < Duration::from_millis(600),
            "judging must not outlive its budget"
        );
        assert_eq!(report.claims_evaluated, 3);
        assert_eq!(report.claims[0].method, VerificationMethod::Judge);
        // Unreached and carrying a figure: not checked, and said so.
        for claim in &report.claims[1..] {
            assert_eq!(claim.method, VerificationMethod::Lexical);
            assert_eq!(claim.verdict, ClaimVerdict::Unverified);
            assert_eq!(claim.unverified_reason, Some(UnverifiedReason::Budget));
        }
        assert_eq!(
            report.unsupported_count(),
            1,
            "only the judged contradiction"
        );
        assert_eq!(report.metadata_json()["verdictCounts"]["unverified"], 2);
    }

    /// The same three claims and a backend with room for them: judged side by
    /// side, all three fit a budget that serves one call in a row.
    #[tokio::test]
    async fn concurrent_calls_judge_more_claims_inside_the_same_budget() {
        let response = "Yields rose by 42 percent in treated plots [1].\n\
                        Frost damage fell by 18 percent in treated plots [1].\n\
                        Harvest weight increased by 7 percent in treated plots [1].";
        let sources = vec![source(
            "Treated plots recorded changes in yields, frost damage, and harvest weight across the trial.",
        )];

        let llm = Arc::new(
            ScriptedLlm::new(vec![EXPLAINED_CONTRADICTION]).with_delay(Duration::from_millis(200)),
        );
        // Four slots: three for the judge, one kept for the user's next turn.
        let verifier = GroundingVerifier {
            judge: Some(scheduled_judge(
                Arc::clone(&llm),
                4,
                Duration::from_millis(400),
            )),
        };

        let started = std::time::Instant::now();
        let report = verifier.verify(response, &sources).await;

        assert_eq!(llm.call_count(), 3);
        assert!(
            report
                .claims
                .iter()
                .all(|claim| claim.method == VerificationMethod::Judge),
            "every claim was judged: {:?}",
            report.claims.iter().map(|c| c.method).collect::<Vec<_>>()
        );
        assert!(
            started.elapsed() < Duration::from_millis(600),
            "three 200ms calls ran side by side, not in a row"
        );
    }

    #[tokio::test]
    async fn without_an_llm_handle_the_verifier_stays_lexical() {
        let sources = vec![source(CONTRADICTION_SOURCE)];
        let report = GroundingVerifier::new(None)
            .verify(CONTRADICTING_RESPONSE, &sources)
            .await;

        assert_eq!(report.supported_claims, 1);
        assert!(!report.judge_used);
    }

    #[tokio::test]
    async fn the_judge_can_be_turned_off_explicitly() {
        let sources = vec![source(CONTRADICTION_SOURCE)];
        let llm = Arc::new(ScriptedLlm::new(vec![EXPLAINED_CONTRADICTION]));

        let report = GroundingVerifier::new(Some(Arc::clone(&llm) as Arc<dyn LLMPort>))
            .with_judge_enabled(false)
            .verify(CONTRADICTING_RESPONSE, &sources)
            .await;

        assert_eq!(llm.call_count(), 0);
        assert_eq!(report.claims[0].method, VerificationMethod::Lexical);
    }

    #[tokio::test]
    async fn metadata_keeps_the_existing_keys_and_adds_the_new_ones() {
        let sources = vec![source(CONTRADICTION_SOURCE)];
        let llm = Arc::new(ScriptedLlm::new(vec![EXPLAINED_CONTRADICTION]));

        let metadata = GroundingVerifier::new(Some(llm))
            .verify(CONTRADICTING_RESPONSE, &sources)
            .await
            .metadata_json();

        assert_eq!(metadata["enabled"], true);
        assert_eq!(metadata["claimsEvaluated"], 1);
        assert_eq!(metadata["supportedClaims"], 0);
        assert!(metadata["supportedClaimNotes"].is_array());
        assert_eq!(metadata["unsupportedClaims"].as_array().unwrap().len(), 1);
        assert!(metadata["groundedRatio"].is_number());

        assert_eq!(metadata["contradictedClaims"].as_array().unwrap().len(), 1);
        assert_eq!(metadata["verdictCounts"]["supported"], 0);
        assert_eq!(metadata["verdictCounts"]["contradicted"], 1);
        assert_eq!(metadata["verdictCounts"]["unsupported"], 0);
        assert_eq!(metadata["verdictCounts"]["unverified"], 0);
        assert_eq!(metadata["judgeUsed"], true);

        let verdicts = metadata["claimVerdicts"].as_array().unwrap();
        assert_eq!(verdicts.len(), 1);
        assert_eq!(verdicts[0]["verdict"], "contradicted");
        assert_eq!(verdicts[0]["method"], "judge");
        assert_eq!(verdicts[0]["citationIds"][0], 1);
        assert!(verdicts[0]["evidenceQuote"]
            .as_str()
            .unwrap()
            .contains("reduced measured cold tolerance"));
        // The scripted judge reports no logprobs, so no probability is claimed.
        assert!(verdicts[0]["confidence"].is_null());
    }

    /// The UI drops the whole summary on any schema violation, so the lists
    /// are cut to its limits while the counts keep the true totals.
    #[test]
    fn metadata_fits_the_ui_schema_and_keeps_true_counts() {
        let claim = |sentence: String, citation_ids: Vec<u32>| ClaimAssessment {
            sentence,
            citation_ids,
            verdict: ClaimVerdict::Unsupported,
            evidence_quote: None,
            method: VerificationMethod::Lexical,
            confidence: None,
            reason: None,
            unverified_reason: None,
        };
        let mut claims: Vec<ClaimAssessment> = (0..25)
            .map(|i| claim(format!("claim {i}"), vec![1]))
            .collect();
        claims[0] = claim("🙂".repeat(1500), (0..40).collect());

        let metadata = GroundingReport::from_assessments(claims, false).metadata_json();

        assert_eq!(metadata["unsupportedClaims"].as_array().unwrap().len(), 20);
        assert_eq!(metadata["claimsEvaluated"], 25);
        assert_eq!(metadata["verdictCounts"]["unsupported"], 25);
        let first = &metadata["claimVerdicts"][0];
        assert_eq!(
            first["sentence"].as_str().unwrap().encode_utf16().count(),
            2000
        );
        let ids: Vec<u64> = first["citationIds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_u64().unwrap())
            .collect();
        assert_eq!(ids, (1..=24).collect::<Vec<u64>>());
    }

    /// A plain claim the judge never reached keeps what overlap said about it:
    /// "not checked by the model" is not a reason to call it missing.
    #[tokio::test]
    async fn an_unreached_plain_claim_keeps_its_lexical_verdict() {
        // Supported on overlap, but not strongly enough to skip the judge.
        let response = "Blueberry growers noticed hardier bushes, better cold tolerance and sturdier stems after salicylic acid treatment [1].";
        let sources = vec![source(
            "Salicylic acid treatment improved cold tolerance in blueberry plants during low temperature stress.",
        )];
        let llm = Arc::new(ScriptedLlm::new(vec!["I cannot say."]));
        let report = GroundingVerifier::new(Some(Arc::clone(&llm) as Arc<dyn LLMPort>))
            .verify(response, &sources)
            .await;

        assert_eq!(llm.call_count(), 1, "the claim was escalated");
        assert_eq!(report.claims[0].verdict, ClaimVerdict::Supported);
        assert_eq!(report.claims[0].method, VerificationMethod::Lexical);
        assert!(report.claims[0].unverified_reason.is_none());
    }

    #[tokio::test]
    async fn a_cited_page_with_no_archived_text_is_not_checked_rather_than_missing() {
        let llm = Arc::new(ScriptedLlm::new(vec!["supported"]));
        let report = GroundingVerifier::new(Some(Arc::clone(&llm) as Arc<dyn LLMPort>))
            .verify(CONTRADICTING_RESPONSE, &[source("")])
            .await;

        assert_eq!(llm.call_count(), 0);
        assert_eq!(report.claims[0].verdict, ClaimVerdict::Unverified);
        assert_eq!(
            report.claims[0].unverified_reason,
            Some(UnverifiedReason::NoText)
        );
        assert_eq!(report.unsupported_count(), 0);
        let metadata = report.metadata_json();
        assert_eq!(metadata["claimVerdicts"][0]["verdict"], "unverified");
        assert_eq!(metadata["claimVerdicts"][0]["unverifiedReason"], "no_text");
    }

    #[test]
    fn unverified_claims_count_against_neither_side_of_the_ratio() {
        let claim = |verdict, reason| ClaimAssessment {
            sentence: "claim".into(),
            citation_ids: vec![1],
            verdict,
            evidence_quote: None,
            method: VerificationMethod::Lexical,
            confidence: None,
            reason: None,
            unverified_reason: reason,
        };
        let report = GroundingReport::from_assessments(
            vec![
                claim(ClaimVerdict::Supported, None),
                claim(ClaimVerdict::Unsupported, None),
                claim(ClaimVerdict::Unverified, Some(UnverifiedReason::Budget)),
                claim(ClaimVerdict::Unverified, Some(UnverifiedReason::Budget)),
            ],
            true,
        );
        assert_eq!(report.unsupported_count(), 1);
        assert!((report.grounded_ratio() - 0.5).abs() < f32::EPSILON);
        let metadata = report.metadata_json();
        assert_eq!(metadata["verdictCounts"]["supported"], 1);
        assert_eq!(metadata["verdictCounts"]["unsupported"], 1);
        assert_eq!(metadata["verdictCounts"]["unverified"], 2);
        assert_eq!(metadata["unsupportedClaims"].as_array().unwrap().len(), 1);
    }
}
