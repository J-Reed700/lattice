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
//! for one. Without an LLM handle, or when it times out or answers with
//! nonsense, escalated claims remain unsupported rather than receiving a
//! positive verdict from vocabulary overlap alone. With no judge configured,
//! reports remain explicitly lexical.

use std::sync::Arc;

use tracing::{debug, info};

use crate::application::ports::LLMPort;
use crate::features::qa::dto::SourceDto;

mod judge;
mod lexical;

use self::judge::ClaimJudge;
use self::lexical::LexicalClaim;

/// How a claim stands against the passages it cites.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ClaimVerdict {
    Supported,
    Contradicted,
    Unsupported,
}

impl ClaimVerdict {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::Contradicted => "contradicted",
            Self::Unsupported => "unsupported",
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
        }
    }
}

/// Emitting more than this many per-claim verdicts would overflow what the UI
/// will accept and cost the whole summary, so the tail is dropped instead.
const MAX_EMITTED_VERDICTS: usize = 60;

#[derive(Debug, Clone, Default)]
pub(super) struct GroundingReport {
    pub(super) claims_evaluated: usize,
    pub(super) supported_claims: usize,
    pub(super) supported_claim_notes: Vec<String>,
    /// Everything not supported — contradicted claims included, because the
    /// existing badge counts this list and a contradiction is the worst case.
    pub(super) unsupported_claims: Vec<String>,
    pub(super) contradicted_claims: Vec<String>,
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

    pub(super) fn grounded_ratio(&self) -> f32 {
        if self.claims_evaluated == 0 {
            return 1.0;
        }
        self.supported_claims as f32 / self.claims_evaluated as f32
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
                serde_json::json!({
                    "sentence": claim.sentence,
                    "citationIds": claim.citation_ids,
                    "verdict": claim.verdict.as_str(),
                    "evidenceQuote": claim.evidence_quote,
                    "method": claim.method.as_str(),
                })
            })
            .collect();

        serde_json::json!({
            "enabled": true,
            "claimsEvaluated": self.claims_evaluated,
            "supportedClaims": self.supported_claims,
            "supportedClaimNotes": self.supported_claim_notes,
            "unsupportedClaims": self.unsupported_claims,
            "groundedRatio": self.grounded_ratio(),
            "contradictedClaims": self.contradicted_claims,
            "verdictCounts": {
                "supported": self.supported_claims,
                "contradicted": self.contradicted_claims.len(),
                "unsupported": self.claims_evaluated
                    .saturating_sub(self.supported_claims)
                    .saturating_sub(self.contradicted_claims.len()),
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

        let pending: Vec<usize> = claims
            .iter()
            .enumerate()
            .filter(|(_, claim)| lexical::needs_judge(claim))
            .map(|(index, _)| index)
            .collect();
        if pending.is_empty() {
            return GroundingReport::from_assessments(assessments, false);
        }

        // Escalated claims are unresolved until the semantic judge returns.
        // A timeout or malformed reply must not certify a numeric contradiction
        // merely because it shares vocabulary with the source.
        for index in &pending {
            if let Some(assessment) = assessments.get_mut(*index) {
                assessment.verdict = ClaimVerdict::Unsupported;
            }
        }
        let outcomes = judge.judge_claims(&claims, &pending, sources).await;
        for (index, outcome) in outcomes {
            if let Some(assessment) = assessments.get_mut(index) {
                assessment.verdict = outcome.verdict;
                assessment.evidence_quote = outcome.quote;
                assessment.method = VerificationMethod::Judge;
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
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use async_trait::async_trait;
    use futures::stream::Stream;

    use super::test_support::source;
    use super::*;
    use crate::application::ports::llm_port::{CompletionRequest, CompletionResponse};
    use crate::shared::error::Result;

    /// Replies with a scripted body, after an optional delay, counting calls.
    struct ScriptedLlm {
        replies: Vec<String>,
        delay: Duration,
        calls: AtomicUsize,
    }

    impl ScriptedLlm {
        fn new(replies: Vec<&str>) -> Self {
            Self {
                replies: replies.into_iter().map(str::to_string).collect(),
                delay: Duration::ZERO,
                calls: AtomicUsize::new(0),
            }
        }

        fn with_delay(mut self, delay: Duration) -> Self {
            self.delay = delay;
            self
        }

        fn call_count(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl LLMPort for ScriptedLlm {
        async fn generate(
            &self,
            _prompt: &str,
            _context: &[String],
            _images: Option<Vec<String>>,
        ) -> Result<String> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if !self.delay.is_zero() {
                tokio::time::sleep(self.delay).await;
            }
            Ok(self
                .replies
                .get(call)
                .or_else(|| self.replies.last())
                .cloned()
                .unwrap_or_default())
        }

        async fn generate_streaming(
            &self,
            _prompt: &str,
            _context: &[String],
            _images: Option<Vec<String>>,
        ) -> Result<Box<dyn Stream<Item = Result<String>> + Send + Unpin + '_>> {
            unimplemented!("streaming is not used by the claim judge")
        }

        async fn complete(&self, _request: &CompletionRequest) -> Result<CompletionResponse> {
            unimplemented!("typed completions are not enabled for this mock")
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

        let llm = Arc::new(ScriptedLlm::new(vec![
            r#"{"verdicts":[{"id":1,"verdict":"contradicted","quote":"reduced measured cold tolerance in blueberry plants by 12 percent"}]}"#,
        ]));
        let report = GroundingVerifier::new(Some(llm))
            .verify(CONTRADICTING_RESPONSE, &sources)
            .await;

        assert_eq!(report.claims_evaluated, 1);
        assert_eq!(report.supported_claims, 0);
        assert_eq!(report.contradicted_count(), 1);
        assert_eq!(report.unsupported_count(), 1, "contradictions stay flagged");
        assert_eq!(report.claims[0].method, VerificationMethod::Judge);
        assert_eq!(
            report.claims[0].evidence_quote.as_deref(),
            Some("reduced measured cold tolerance in blueberry plants by 12 percent")
        );
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
    async fn fabricated_support_quote_cannot_certify_a_claim() {
        let llm = Arc::new(ScriptedLlm::new(vec![
            r#"{"verdicts":[{"id":1,"verdict":"supported","quote":"fabricated evidence that is absent"}]}"#,
        ]));
        let report = GroundingVerifier::new(Some(llm))
            .verify(CONTRADICTING_RESPONSE, &[source(CONTRADICTION_SOURCE)])
            .await;
        assert_eq!(report.supported_claims, 0);
    }

    #[tokio::test]
    async fn invalid_citation_is_not_rescued_by_the_judge() {
        let llm = Arc::new(ScriptedLlm::new(vec![
            r#"{"verdicts":[{"id":1,"verdict":"supported","quote":"reduced measured cold tolerance"}]}"#,
        ]));
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
        assert_eq!(report.claims[0].method, VerificationMethod::Lexical);
        assert!(!report.judge_used);
    }

    #[tokio::test]
    async fn a_plain_strongly_supported_claim_never_reaches_the_judge() {
        // No digits, no negation, and near-total vocabulary overlap.
        let response = "Salicylic acid treatment improved cold tolerance in blueberry plants during low temperature stress [1].";
        let sources = vec![source(
            "Salicylic acid treatment improved cold tolerance in blueberry plants during low temperature stress.",
        )];
        let llm = Arc::new(ScriptedLlm::new(vec![
            r#"{"verdicts":[{"id":1,"verdict":"unsupported","quote":""}]}"#,
        ]));

        let report = GroundingVerifier::new(Some(Arc::clone(&llm) as Arc<dyn LLMPort>))
            .verify(response, &sources)
            .await;

        assert_eq!(llm.call_count(), 0, "the judge must not be called");
        assert_eq!(report.supported_claims, 1);
        assert_eq!(report.claims[0].method, VerificationMethod::Lexical);
    }

    /// Real sleeps rather than a paused clock: `tokio`'s `test-util` feature is
    /// not enabled for this crate. One 200ms call against a 400ms budget leaves
    /// too little for a second, with wide margins on either side of the check.
    #[tokio::test]
    async fn time_budget_leaves_later_claims_on_their_lexical_verdict() {
        let response = "Yields rose by 42 percent in treated plots [1].\n\
                        Frost damage fell by 18 percent in treated plots [1].\n\
                        Harvest weight increased by 7 percent in treated plots [1].";
        let sources = vec![source(
            "Treated plots recorded changes in yields, frost damage, and harvest weight across the trial.",
        )];

        let llm = Arc::new(
            ScriptedLlm::new(vec![
                r#"{"verdicts":[{"id":1,"verdict":"contradicted","quote":""}]}"#,
            ])
            .with_delay(Duration::from_millis(200)),
        );
        let verifier = GroundingVerifier {
            judge: Some(
                ClaimJudge::new(Arc::clone(&llm) as Arc<dyn LLMPort>)
                    .with_batch_size(1)
                    .with_time_budget(Duration::from_millis(400)),
            ),
        };

        let report = verifier.verify(response, &sources).await;

        assert_eq!(report.claims_evaluated, 3);
        assert_eq!(
            llm.call_count(),
            1,
            "the budget must stop further judge calls"
        );
        assert_eq!(report.claims[0].method, VerificationMethod::Judge);
        assert_eq!(report.claims[1].method, VerificationMethod::Lexical);
        assert_eq!(report.claims[2].method, VerificationMethod::Lexical);
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
        let llm = Arc::new(ScriptedLlm::new(vec![
            r#"{"verdicts":[{"id":1,"verdict":"contradicted","quote":""}]}"#,
        ]));

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
        let llm = Arc::new(ScriptedLlm::new(vec![
            r#"{"verdicts":[{"id":1,"verdict":"contradicted","quote":"reduced measured cold tolerance"}]}"#,
        ]));

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
        assert_eq!(metadata["judgeUsed"], true);

        let verdicts = metadata["claimVerdicts"].as_array().unwrap();
        assert_eq!(verdicts.len(), 1);
        assert_eq!(verdicts[0]["verdict"], "contradicted");
        assert_eq!(verdicts[0]["method"], "judge");
        assert_eq!(verdicts[0]["citationIds"][0], 1);
        assert_eq!(
            verdicts[0]["evidenceQuote"],
            "reduced measured cold tolerance"
        );
    }
}
