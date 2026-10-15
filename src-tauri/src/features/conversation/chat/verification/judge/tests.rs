//! Tests for the claim judge: verdict parsing, evidence assembly and the
//! sequential judging of evidence split over calls.

use super::super::lexical::lexical_pass;
use super::super::test_support::source;
use super::*;
use lattice_core::application::ports::llm_port::CompletionResponse;

/// Room enough for any test page in one call.
const WIDE: usize = 1_000_000;

fn found(evidence: Evidence) -> ClaimEvidence {
    match evidence {
        Evidence::Found(mut parts) if parts.len() == 1 => parts.remove(0),
        other => panic!("expected evidence in one part, got {other:?}"),
    }
}

fn found_parts(evidence: Evidence) -> Vec<ClaimEvidence> {
    match evidence {
        Evidence::Found(parts) => parts,
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
        text: "[1]\nThe source says the treatment reduced measured cold tolerance by 12 percent."
            .into(),
        quote: Some(
            "The source says the treatment reduced measured cold tolerance by 12 percent.".into(),
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
    let evidence = found(evidence_for(&claims[0], &sources, WIDE));

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
    assert!(evidence.text.starts_with("[1]\n"));
    assert!(evidence.quote.is_some());

    // A judge with room for less still sees both facts, in its first call:
    // they are the windows that share the most with the claim.
    let parts = found_parts(evidence_for(&claims[0], &sources, 4_000));
    let first = &parts[0].text;
    assert!(first.contains("6-8 hours of direct sunlight"), "{first}");
    assert!(first.contains("south-facing bed"), "{first}");
    assert!(parts.iter().all(|part| part.text.chars().count() <= 4_000));
}

/// A page longer than one judge call is split across calls, not cut: the
/// one passage that supports the claim, deep in the page, is in some part.
/// The old evidence held three windows and 4,000 characters, so support
/// outside the three best-scoring windows was never seen.
#[test]
fn support_deep_in_a_long_page_reaches_the_judge_in_a_later_part() {
    let page = format!(
        "{}{}The harvest begins in late September across the valley. {}",
        "The valley harvest festival draws visitors every year. ".repeat(40),
        "Harvest notes about the valley and its harvest crews. ".repeat(60),
        "Closing prose. ".repeat(50),
    );
    let sources = [source(&page)];
    let claims = lexical_pass("The valley harvest begins in late September [1].", &sources);
    let parts = found_parts(evidence_for(&claims[0], &sources, 2_500));
    assert!(parts.len() > 1, "the page needs more than one call");
    assert!(parts.iter().all(|part| part.text.chars().count() <= 2_500));
    assert!(
        parts
            .iter()
            .any(|part| part.text.contains("begins in late September")),
        "the supporting sentence is judged"
    );
}

/// Every cited source is read: the old judge read the first three.
#[test]
fn a_fourth_cited_source_is_judged_too() {
    let mut sources = Vec::new();
    for (index, fact) in [
        "Basil wilts below ten degrees.",
        "Basil prefers well drained soil.",
        "Basil flowers in late summer.",
        "Basil leaves darken when frozen.",
    ]
    .iter()
    .enumerate()
    {
        let mut cited = source(fact);
        cited.citation_id = Some(index as u32 + 1);
        sources.push(cited);
    }
    let claims = lexical_pass(
        "Basil wilts below ten degrees, prefers drained soil, flowers late and its leaves darken when frozen [1][2][3][4].",
        &sources,
    );
    let evidence = found(evidence_for(&claims[0], &sources, WIDE));
    assert!(
        evidence.text.contains("[4]\nBasil leaves darken"),
        "{}",
        evidence.text
    );
}

#[test]
fn several_cited_pages_each_get_their_best_window_in_the_first_call() {
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
    // A judge call with room for about one page: every page's best window
    // goes into the first call before any page's second-best.
    let parts = found_parts(evidence_for(&claims[0], &sources, 4_000));
    for fact in [
        "wilts below ten",
        "well drained soil",
        "flowers in late summer",
    ] {
        assert!(parts[0].text.contains(fact), "missing {fact}");
    }
    assert!(parts.iter().all(|part| part.text.chars().count() <= 4_000));
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
        evidence_for(claim, &sources, WIDE),
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
        evidence_for(&claims[0], &sources, WIDE),
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
    let parts = found_parts(evidence_for(&claims[0], &sources, WIDE));
    judge
        .judge_claims(&[(0, claims[0].claim_text.as_str(), parts)])
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

/// Says "supported" only to a call whose evidence holds `marker`.
struct PartAwareLlm {
    marker: &'static str,
    calls: std::sync::Mutex<usize>,
}

#[async_trait::async_trait]
impl LLMPort for PartAwareLlm {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        *self.calls.lock().unwrap() += 1;
        let holds_marker = request.input.iter().any(|input| {
            matches!(input, CompletionInput::Message { content, .. } if content.contains(self.marker))
        });
        Ok(CompletionResponse {
            text: if holds_marker {
                "supported\nReason: stated.\nSource quote: none".to_string()
            } else {
                "unsupported\nReason: not stated.\nSource quote: none".to_string()
            },
            ..Default::default()
        })
    }

    fn model_name(&self) -> &str {
        "part-aware-judge"
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

fn part(text: &str) -> ClaimEvidence {
    ClaimEvidence {
        text: text.to_string(),
        quote: None,
    }
}

async fn judge_parts(llm: &Arc<PartAwareLlm>, parts: Vec<ClaimEvidence>) -> ClaimJudgment {
    let judge = ClaimJudge::new(Arc::clone(llm) as Arc<dyn LLMPort>);
    judge
        .judge_claims(&[(0, "The harvest begins in September.", parts)])
        .await
        .remove(&0)
        .unwrap()
}

/// Evidence split over calls is judged in order and the first part that
/// supports the claim settles it; the parts after it are not asked.
#[tokio::test]
async fn the_first_part_that_supports_a_claim_settles_it() {
    let llm = Arc::new(PartAwareLlm {
        marker: "late September",
        calls: std::sync::Mutex::new(0),
    });
    let verdict = judge_parts(
        &llm,
        vec![
            part("[1]\nThe festival draws visitors."),
            part("[1]\nThe harvest begins in late September."),
            part("[1]\nClosing prose."),
        ],
    )
    .await;
    assert!(matches!(
        verdict,
        ClaimJudgment::Judged(ref outcome) if outcome.verdict == ClaimVerdict::Supported
    ));
    assert_eq!(*llm.calls.lock().unwrap(), 2);
}

#[tokio::test]
async fn a_claim_no_part_supports_is_unsupported_after_every_part_is_read() {
    let llm = Arc::new(PartAwareLlm {
        marker: "never present",
        calls: std::sync::Mutex::new(0),
    });
    let verdict = judge_parts(&llm, vec![part("[1]\nOne."), part("[1]\nTwo.")]).await;
    assert!(matches!(
        verdict,
        ClaimJudgment::Judged(ref outcome) if outcome.verdict == ClaimVerdict::Unsupported
    ));
    assert_eq!(*llm.calls.lock().unwrap(), 2);
}

/// The room a claim's evidence gets comes from the judge's own window: a
/// larger window reads more per call.
#[test]
fn evidence_room_follows_the_judge_window() {
    let small = ClaimJudge::new(Arc::new(PartAwareLlm {
        marker: "",
        calls: std::sync::Mutex::new(0),
    }) as Arc<dyn LLMPort>);
    let room = small.evidence_room_chars("A claim.");
    // 8,192 tokens less the answer's 4,096 and the prompts, at the
    // estimate's three-quarter under-fill.
    assert!(room > 8_000 && room < 12_288, "got {room}");
}
