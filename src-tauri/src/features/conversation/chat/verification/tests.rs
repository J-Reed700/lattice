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
    let mut evidence =
        source("The design workspace retains hourly file versions for 14 days before expiration.");
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
async fn a_plain_near_verbatim_claim_never_reaches_the_judge() {
    // No digits, no negation, and the passage's own sentence word for word.
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

/// `cp-025` from the claim-checker pairs. Every content word is in the
/// passage and the claim has no "not", so the old strong-overlap rule
/// certified it supported without a model ever reading it.
#[tokio::test]
async fn a_claim_that_keeps_the_passage_words_and_drops_its_negation_reaches_the_judge() {
    let pair = super::gate_tests::pair("cp-025");
    let llm = Arc::new(ScriptedLlm::new(vec![
        "contradicted\nReason: The passage says a manager cannot grant one.\nSource quote: a manager cannot grant one",
    ]));

    let report = GroundingVerifier::new(Some(Arc::clone(&llm) as Arc<dyn LLMPort>))
        .verify(
            &super::gate_tests::cited_answer(&pair.claim),
            &[source(&pair.passage)],
        )
        .await;

    assert_eq!(llm.call_count(), 1);
    assert_eq!(report.claims[0].method, VerificationMethod::Judge);
    assert_eq!(report.claims[0].verdict, ClaimVerdict::Contradicted);
    assert_eq!(report.supported_claims, 0);
}

/// A copy of a passage sentence that cut its "Do not" was sent to the
/// judge for that reason alone. If the judge never answers, the shared
/// wording is no evidence either way, so it is not checked rather than
/// supported.
#[tokio::test]
async fn an_unreached_copy_that_cut_a_negation_is_not_checked_rather_than_supported() {
    let pair = super::gate_tests::pair("cp-013");
    let llm = Arc::new(ScriptedLlm::new(vec!["I cannot judge this."]));

    let report = GroundingVerifier::new(Some(Arc::clone(&llm) as Arc<dyn LLMPort>))
        .verify(
            "Widen the pool on Garnetline to work around a stale lease takeover [1].",
            &[source(&pair.passage)],
        )
        .await;

    assert_eq!(llm.call_count(), 1, "the copy was escalated");
    assert_eq!(report.claims[0].verdict, ClaimVerdict::Unverified);
    assert_eq!(
        report.claims[0].unverified_reason,
        Some(UnverifiedReason::JudgeFailed)
    );
    assert_eq!(report.supported_claims, 0);
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
