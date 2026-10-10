use super::*;
use crate::application::ports::llm_port::CompletionResponse;
use crate::application::services::claim_verification::ClaimVerdict;
use async_trait::async_trait;
use std::sync::Mutex;

/// Answers with a fixed text and records every request it was sent.
struct Scripted {
    window: usize,
    answer: String,
    sent: Mutex<Vec<CompletionRequest>>,
}

impl Scripted {
    fn new(window: usize, answer: &str) -> Self {
        Self {
            window,
            answer: answer.to_string(),
            sent: Mutex::new(Vec::new()),
        }
    }

    fn first_request(&self) -> CompletionRequest {
        self.sent.lock().unwrap().first().cloned().unwrap()
    }
}

#[async_trait]
impl LLMPort for Scripted {
    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> crate::shared::error::Result<CompletionResponse> {
        self.sent.lock().unwrap().push(request.clone());
        Ok(CompletionResponse::from_text(self.answer.clone()))
    }
    fn model_name(&self) -> &str {
        "scripted"
    }
    fn max_context_tokens(&self) -> usize {
        self.window
    }
    async fn is_ready(&self) -> crate::shared::error::Result<bool> {
        Ok(true)
    }
}

fn passage(id: &str, label: &str, text: &str, rank: f32) -> EvidencePassage {
    EvidencePassage {
        id: id.into(),
        label: label.into(),
        text: text.into(),
        rank,
    }
}

fn user_text(request: &CompletionRequest) -> String {
    request.user_text().to_string()
}

#[tokio::test]
async fn evidence_is_rendered_under_its_heading_between_the_task_and_the_closing() {
    let llm = Scripted::new(8192, "answer");
    let mut request = GroundedRequest::new("Be exact.", "Fill the row.");
    request.evidence_heading = "Passages:".into();
    request.evidence = vec![
        passage("c1", "[1]", "First passage.", 1.0),
        passage("c2", "[2]", "Second passage.", 0.5),
    ];
    request.closing = "Answer in JSON.".into();
    request.call = CallOptions {
        priority: InferencePriority::Interactive,
        cache_key: Some("doc-1".into()),
        ..Default::default()
    };

    let output = generate(&llm, request).await.unwrap();

    assert_eq!(output.text, "answer");
    assert_eq!(output.used_evidence_ids, vec!["c1", "c2"]);
    let sent = llm.first_request();
    assert_eq!(
        user_text(&sent),
        "Fill the row.\n\nPassages:\n[1] First passage.\n\n[2] Second passage.\n\nAnswer in JSON."
    );
    assert!(matches!(
        sent.input.first(),
        Some(CompletionInput::Message { role, content }) if role == "system" && content == "Be exact."
    ));
    assert_eq!(sent.priority, InferencePriority::Interactive);
    assert_eq!(sent.cache_key.as_deref(), Some("doc-1"));
    // The reservation the plan made is the one the request enforces.
    assert_eq!(
        sent.max_output_tokens,
        u32::try_from(output.accounting.output_reserved).ok()
    );
    assert!(output.accounting.fits());
}

#[tokio::test]
async fn a_task_larger_than_the_window_is_refused_with_a_typed_overflow_and_never_sent() {
    let llm = Scripted::new(4096, "unused");
    let request = GroundedRequest::new("Be exact.", "word ".repeat(20_000));

    let error = generate(&llm, request).await.unwrap_err();

    let overflow = error.overflow().expect("an overflow");
    assert!(overflow.required > overflow.available, "{overflow:?}");
    assert!(llm.sent.lock().unwrap().is_empty());
    assert!(matches!(AppError::from(error), AppError::InvalidInput(_)));
}

#[tokio::test]
async fn required_evidence_that_does_not_fit_is_refused_rather_than_cut() {
    let llm = Scripted::new(4096, "unused");
    let mut request = GroundedRequest::new("", "Summarize.");
    request.evidence = vec![passage("m1", "USER:", &"long ".repeat(10_000), 1.0)];

    let error = generate(&llm, request).await.unwrap_err();

    assert!(error.overflow().is_some(), "{error:?}");
    assert!(llm.sent.lock().unwrap().is_empty());
}

#[tokio::test]
async fn best_first_keeps_the_strongest_passages_that_fit_and_reports_the_rest() {
    let llm = Scripted::new(4096, "answer");
    let mut request = GroundedRequest::new("", "Answer.");
    request.selection = EvidenceSelection::BestFirst;
    request.evidence = vec![
        passage("weak", "[1]", &"weak ".repeat(100), 0.1),
        passage("huge", "[2]", &"huge ".repeat(20_000), 0.9),
        passage("strong", "[3]", "strong", 0.8),
    ];

    let output = generate(&llm, request).await.unwrap();

    // Rendered in the caller's order, the oversized passage left out.
    assert_eq!(output.used_evidence_ids, vec!["weak", "strong"]);
    assert_eq!(output.accounting.evicted, vec!["evidence:huge"]);
    let text = user_text(&llm.first_request());
    assert!(text.find("[1]").unwrap() < text.find("[3]").unwrap());
    assert!(!text.contains("[2]"));
}

#[tokio::test]
async fn a_prefix_stops_at_the_first_passage_that_does_not_fit() {
    let llm = Scripted::new(8192, "answer");
    let mut request = GroundedRequest::new("", "Summarize the opening.");
    request.selection = EvidenceSelection::Prefix;
    request.evidence_limit = Some(100);
    request.evidence = vec![
        passage("p1", "", &"a ".repeat(100), 1.0),
        passage("p2", "", &"b ".repeat(200), 0.9),
        passage("p3", "", "c", 0.8),
    ];

    let output = generate(&llm, request).await.unwrap();

    // `p3` would fit, but carrying it after a gap would not be an opening.
    assert_eq!(output.used_evidence_ids, vec!["p1"]);
    assert!(output.accounting.document_evidence <= 100);
}

#[tokio::test]
async fn evidence_room_shrinks_by_what_the_task_and_history_take() {
    let llm = Scripted::new(8192, "unused");
    let bare = GroundedRequest::new("", "Task.");
    let mut with_history = GroundedRequest::new("", "Task.");
    with_history.history = vec![HistoryItem {
        role: "user".into(),
        content: "earlier ".repeat(400),
    }];

    let bare_room = evidence_room(&llm, &bare).unwrap();
    let history_room = evidence_room(&llm, &with_history).unwrap();

    assert!(history_room < bare_room);
    assert!(bare_room < 8192);
}

#[tokio::test]
async fn a_requested_claim_check_reports_a_verdict_against_the_carried_evidence() {
    let llm = Scripted::new(8192, "Water boils at 100 C.");
    let judge: Arc<dyn LLMPort> = Arc::new(Scripted::new(
        8192,
        "supported\nReason: the passage states it.\nSource passage: passage-0",
    ));
    let mut request = GroundedRequest::new("", "When does water boil?");
    request.evidence = vec![passage(
        "c1",
        "[1]",
        "Water boils at 100 C at sea level.",
        1.0,
    )];
    request.verification = Some(ClaimCheck {
        policy: CheckPolicy::Chat,
        claims: Vec::new(),
        judge: Some(judge),
    });

    let output = generate(&llm, request).await.unwrap();

    let verdicts = output.verdicts.expect("verdicts");
    assert_eq!(verdicts.len(), 1);
    assert_eq!(verdicts[0].claim, "Water boils at 100 C.");
    assert!(matches!(
        &verdicts[0].judgment,
        ClaimJudgment::Judged(outcome) if outcome.verdict == ClaimVerdict::Supported
    ));
}

#[test]
fn a_long_text_becomes_ordered_passages_without_losing_a_word() {
    let text = format!("Opening paragraph.\n\n{}\n\nClosing.", "word ".repeat(300));
    let passages = passages_from_text("doc", &text, 200);

    assert_eq!(passages[0].text, "Opening paragraph.");
    assert_eq!(passages.last().unwrap().text, "Closing.");
    assert!(passages.iter().all(|p| p.text.chars().count() <= 200));
    assert!(passages.windows(2).all(|pair| pair[0].rank > pair[1].rank));
    let words: usize = passages
        .iter()
        .map(|p| p.text.split_whitespace().count())
        .sum();
    assert_eq!(words, 2 + 300 + 1);
    assert_eq!(passages[1].id, "doc:1");
}
