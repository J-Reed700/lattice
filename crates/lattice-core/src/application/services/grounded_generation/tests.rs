use super::*;
use crate::application::ports::llm_port::CompletionResponse;
use crate::application::ports::llm_port::InferencePriority;
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
async fn structured_call_options_reach_the_model_unchanged() {
    let llm = Scripted::new(8192, "{}");
    let mut request = GroundedRequest::new("Return JSON.", "Draft a card.");
    request.output_tokens = 1000;
    request.call = CallOptions {
        priority: InferencePriority::Verification,
        cache_key: Some("course-1".into()),
        no_time_limit: true,
        json_schema: Some(serde_json::json!({"type": "object"})),
        reasoning_effort: Some("low".into()),
        ..Default::default()
    };

    let output = generate(&llm, request).await.unwrap();

    let sent = llm.first_request();
    assert_eq!(sent.priority, InferencePriority::Verification);
    assert_eq!(sent.cache_key.as_deref(), Some("course-1"));
    assert!(sent.no_time_limit);
    assert_eq!(
        sent.json_schema,
        Some(serde_json::json!({"type": "object"}))
    );
    assert_eq!(sent.reasoning_effort.as_deref(), Some("low"));
    assert_eq!(sent.max_output_tokens, Some(1000));
    assert_eq!(output.finish_reason, "stop");
}

#[tokio::test]
async fn an_answer_that_fills_the_window_gets_the_room_the_input_left() {
    let llm = Scripted::new(8192, "{}");
    let mut request = GroundedRequest::new("Return JSON.", "Draft an outline.");
    request.output_tokens = 1000;
    request.output_fills_window = true;

    let output = generate(&llm, request).await.unwrap();

    let max_output = llm.first_request().max_output_tokens.unwrap() as usize;
    let margin = output.accounting.safety_margin;
    assert_eq!(max_output + output.accounting.total_input + margin, 8192);
    assert!(max_output > 1000);
}

#[tokio::test]
async fn streaming_delivers_text_and_retries_through_the_callers_callbacks() {
    let llm = Scripted::new(8192, "partial answer");
    let received = Mutex::new(Vec::new());
    let on_text = |text: String| {
        received.lock().unwrap().push(text);
        Ok(())
    };
    let on_retry = |_: usize| Ok(());

    let output = generate_streaming(
        &llm,
        GroundedRequest::new("", "Write."),
        &Streaming {
            on_text: &on_text,
            on_retry: Some(&on_retry),
        },
    )
    .await
    .unwrap();

    assert_eq!(output.text, "partial answer");
    assert_eq!(
        *received.lock().unwrap(),
        vec!["partial answer".to_string()]
    );
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
