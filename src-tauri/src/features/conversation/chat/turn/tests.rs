//! Stage tests against a fake runtime: no database, no model, no container.
use super::assemble::{PlannedRequest, RenderedPrompt};
use super::prepare::PreparedTurn;
use super::research::{self, AssembledTurn, ResearchJournal, ResearchRound, Rounds};
use super::*;
use crate::features::conversation::chat::test_runtime::{FakeRuntime, Scripted};
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use lattice_core::application::ports::LLMPort;
use std::sync::Mutex;

/// A model that never answers, and says when it has been asked.
struct Unanswering {
    asked: tokio::sync::Notify,
}

#[async_trait::async_trait]
impl LLMPort for Unanswering {
    async fn complete(&self, _request: &CompletionRequest) -> Result<CompletionResponse> {
        self.asked.notify_one();
        std::future::pending().await
    }
    fn model_name(&self) -> &str {
        "unanswering"
    }
    fn max_context_tokens(&self) -> usize {
        8192
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

fn prompt() -> RenderedPrompt {
    RenderedPrompt {
        enhanced_message: "question".into(),
        message: None,
        sources: Vec::new(),
        retrieval_trace: None,
        short_circuit_response: None,
        pages_read: Default::default(),
        can_open_pages: false,
        has_grounded_context: false,
        attachment_names: Vec::new(),
        attachment_ids: Vec::new(),
    }
}

fn request(turn: &PreparedTurn) -> PlannedRequest {
    PlannedRequest {
        input: vec![CompletionInput::Message {
            role: "user".into(),
            content: "question".into(),
        }],
        tools: Vec::new(),
        max_output_tokens: turn.budget.output_tokens,
        input_budget: turn.budget.input_budget,
        memory_usage: None,
        summary_refold: None,
    }
}

fn recording_sink() -> (ChatEventSink, Arc<Mutex<Vec<ChatStreamEventDto>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&events);
    let sink: ChatEventSink = Arc::new(move |event| {
        captured.lock().unwrap().push(event);
        Ok(())
    });
    (sink, events)
}

#[tokio::test]
async fn stopping_the_turn_ends_the_generate_stage_while_the_model_is_still_generating() {
    let llm = Arc::new(Unanswering {
        asked: tokio::sync::Notify::new(),
    });
    let runtime = FakeRuntime::default();
    let turn = PreparedTurn::for_test(
        "stage-cancel-conversation",
        "stage-cancel-turn",
        llm.clone(),
        runtime.conversations.clone(),
        runtime.turn_cancellation.clone(),
    )
    .unwrap();
    let (sink, events) = recording_sink();
    let mut metrics = ConversationFlowTimingMetrics::default();
    let flags = SearchFlags::from_preferences(None);
    let planned = request(&turn);

    let stop = async {
        llm.asked.notified().await;
        assert!(runtime
            .turn_cancellation
            .request_cancel("stage-cancel-conversation", Some("stage-cancel-turn")));
    };
    let generate = generate::generate_answer(
        &runtime,
        &turn,
        flags,
        prompt(),
        planned,
        None,
        &sink,
        &mut metrics,
    );
    let (result, ()) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(generate, stop)
    })
    .await
    .expect("a stop press ends the stage, not the time budget");

    let error = result.err().expect("a cancelled turn has no answer");
    assert!(error.to_string().contains("cancelled"), "{error}");
    // The bubble is never left generating: the stream still closes.
    assert!(
        events.lock().unwrap().iter().any(|event| event.done),
        "no terminal event"
    );
}

#[tokio::test]
async fn a_router_answer_skips_the_model_entirely() {
    let llm = Arc::new(Unanswering {
        asked: tokio::sync::Notify::new(),
    });
    let runtime = FakeRuntime::default();
    let turn = PreparedTurn::for_test(
        "stage-canned-conversation",
        "stage-canned-turn",
        llm,
        runtime.conversations.clone(),
        runtime.turn_cancellation.clone(),
    )
    .unwrap();
    let (sink, _) = recording_sink();
    let mut metrics = ConversationFlowTimingMetrics::default();
    let planned = request(&turn);
    let mut canned = prompt();
    canned.short_circuit_response = Some("Hello!".into());

    let generated = generate::generate_answer(
        &runtime,
        &turn,
        SearchFlags::from_preferences(None),
        canned,
        planned,
        None,
        &sink,
        &mut metrics,
    )
    .await
    .unwrap();

    assert_eq!(generated.response, "Hello!");
    assert!(metrics.generation_subtimings.is_some());
}

/// The intent step's answer, as the utility model gives it.
fn intent_reply(needs_web: bool) -> CompletionResponse {
    CompletionResponse::from_text(format!(
        r#"{{"needsKnowledgeBase":false,"needsWeb":{needs_web},"isFollowup":false,"confidence":0.9}}"#
    ))
}

/// Each turn is classified afresh. A turn the intent step sends to the web
/// gets a fallback, never a forced search, and the next turn starts from its
/// own preferences: an earlier turn's fallback does not carry over.
#[tokio::test]
async fn each_turn_decides_its_own_web_fallback() {
    let llm = Scripted::new([intent_reply(true), intent_reply(false)]);
    let runtime = FakeRuntime::default();

    let first = PreparedTurn::for_test(
        "intent-fallback-conversation",
        "intent-fallback-turn-1",
        llm.clone(),
        runtime.conversations.clone(),
        runtime.turn_cancellation.clone(),
    )
    .unwrap();
    let flags = classify::classify_turn(&runtime, &first, None).await;
    assert!(flags.web_fallback);
    assert!(!flags.force_web_search);
    drop(first);

    let second = PreparedTurn::for_test(
        "intent-fallback-conversation",
        "intent-fallback-turn-2",
        llm.clone(),
        runtime.conversations.clone(),
        runtime.turn_cancellation.clone(),
    )
    .unwrap();
    let flags = classify::classify_turn(&runtime, &second, None).await;
    assert!(!flags.web_fallback);
    assert!(!flags.web_planned());
    assert_eq!(
        llm.requests().len(),
        2,
        "each turn asked the intent step once"
    );
}

fn deep_research() -> SearchFlags {
    SearchFlags::from_preferences(Some(&ToolPreferences {
        deep_research_mode: true,
        ..Default::default()
    }))
}

/// The model asks to search the web for `query`.
fn searches(id: &str, query: &str) -> CompletionResponse {
    let call = CompletionInput::ToolCall {
        id: id.into(),
        name: "web_search".into(),
        arguments: serde_json::json!({ "query": query }),
    };
    CompletionResponse {
        tool_calls: vec![call.clone()],
        replay: vec![call],
        finish_reason: "tool_calls".into(),
        ..Default::default()
    }
}

/// Answers every search with one page about it.
struct FakeWeb;

#[async_trait::async_trait]
impl crate::features::conversation::chat::ports::ChatTools for FakeWeb {
    fn list_tools(&self) -> Vec<lattice_core::application::contracts::tools::ToolDefinition> {
        Vec::new()
    }
    async fn execute(
        &self,
        call: lattice_core::application::contracts::tools::FunctionCall,
    ) -> Result<lattice_core::application::contracts::tools::FunctionResult> {
        let query = call.arguments["query"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        Ok(
            lattice_core::application::contracts::tools::FunctionResult::success(
                serde_json::json!({
                    "results": [{
                        "title": format!("About {query}"),
                        "url": format!("https://example.com/{}", query.replace(' ', "-")),
                        "snippet": format!("What the web says about {query}."),
                    }],
                    "query": query,
                    "result_count": 1,
                }),
            ),
        )
    }
}

/// Keeps every save a research turn makes.
#[derive(Default)]
struct RecordingJournal {
    rounds: Mutex<Vec<ResearchRound>>,
}

#[async_trait::async_trait]
impl ResearchJournal for RecordingJournal {
    async fn assembled(&self, _turn: &AssembledTurn) -> Result<()> {
        Ok(())
    }
    async fn round(&self, round: &ResearchRound) -> Result<()> {
        self.rounds.lock().unwrap().push(round.clone());
        Ok(())
    }
    async fn resumes_later(&self) -> bool {
        false
    }
}

fn research_request(turn: &PreparedTurn) -> PlannedRequest {
    PlannedRequest {
        tools: vec![lattice_core::application::ports::ToolDefinition {
            name: "web_search".into(),
            description: "Search the web".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": { "query": { "type": "string" } },
            }),
        }],
        ..request(turn)
    }
}

#[tokio::test]
async fn a_research_turn_saves_each_round_it_finishes() {
    let llm = Scripted::new([
        searches("call-1", "battery recycling"),
        searches("call-2", "lithium recovery rates"),
        CompletionResponse::from_text("Recovery rates vary by process."),
    ]);
    let runtime = FakeRuntime {
        tools: Some(Arc::new(FakeWeb)),
        ..FakeRuntime::default()
    };
    let turn = PreparedTurn::for_test(
        "research-save-conversation",
        "research-save-turn",
        llm.clone(),
        runtime.conversations.clone(),
        runtime.turn_cancellation.clone(),
    )
    .unwrap();
    let (sink, _) = recording_sink();
    let journal = RecordingJournal::default();
    let mut metrics = ConversationFlowTimingMetrics::default();

    let generated = generate::generate_answer(
        &runtime,
        &turn,
        deep_research(),
        prompt(),
        research_request(&turn),
        Some(Rounds {
            journal: &journal,
            start: None,
        }),
        &sink,
        &mut metrics,
    )
    .await
    .unwrap();

    assert_eq!(generated.response, "Recovery rates vary by process.");
    let rounds = journal.rounds.lock().unwrap();
    // The answering round is not a research round: nothing is left to resume.
    assert_eq!(rounds.len(), 2);
    assert_eq!(rounds[0].completed, 1);
    assert_eq!(rounds[1].completed, 2);
    assert_eq!(
        rounds[1].queries,
        ["battery recycling", "lithium recovery rates"]
    );
    // Each save carries every round so far: both calls and both results.
    let results = rounds[1]
        .transcript
        .iter()
        .filter(|item| matches!(item, CompletionInput::ToolResult { .. }))
        .count();
    assert_eq!(results, 2);
    assert_eq!(rounds[1].transcript.len(), 4);
    assert!(
        !rounds[1].steps.is_empty(),
        "the turn record so far is saved"
    );
}

#[tokio::test]
async fn a_resumed_research_turn_carries_on_after_its_last_saved_round() {
    let llm = Scripted::new([CompletionResponse::from_text("Both searches agree [1].")]);
    // No tools: a resumed turn that searched again would fail the test.
    let runtime = FakeRuntime::default();
    let turn = PreparedTurn::for_test(
        "research-resume-conversation",
        "research-resume-turn",
        llm.clone(),
        runtime.conversations.clone(),
        runtime.turn_cancellation.clone(),
    )
    .unwrap();
    let saved = vec![
        CompletionInput::ToolCall {
            id: "call-1".into(),
            name: "web_search".into(),
            arguments: serde_json::json!({ "query": "first" }),
        },
        CompletionInput::ToolResult {
            id: "call-1".into(),
            output: "What the web says about first.".into(),
        },
        CompletionInput::ToolCall {
            id: "call-2".into(),
            name: "web_search".into(),
            arguments: serde_json::json!({ "query": "second" }),
        },
        CompletionInput::ToolResult {
            id: "call-2".into(),
            output: "What the web says about second.".into(),
        },
    ];
    let start = research::LoopStart {
        rounds: 2,
        transcript: saved.clone(),
        queries: vec!["first".into(), "second".into()],
        fetched: Vec::new(),
        timings: ToolLoopTimingMetrics {
            iterations: 2,
            tool_call_count: 2,
            ..Default::default()
        },
    };
    let planned = research_request(&turn);
    let planned_len = planned.input.len();
    let (sink, _) = recording_sink();
    let journal = RecordingJournal::default();
    let mut metrics = ConversationFlowTimingMetrics::default();

    let generated = generate::generate_answer(
        &runtime,
        &turn,
        deep_research(),
        prompt(),
        planned,
        Some(Rounds {
            journal: &journal,
            start: Some(start),
        }),
        &sink,
        &mut metrics,
    )
    .await
    .unwrap();

    assert_eq!(generated.response, "Both searches agree [1].");
    let requests = llm.requests();
    assert_eq!(
        requests.len(),
        1,
        "one round, to answer: nothing is searched again"
    );
    // The planned request, then the saved rounds' calls and results.
    assert_eq!(requests[0].input.len(), planned_len + saved.len());
    assert!(matches!(
        &requests[0].input[planned_len + 3],
        CompletionInput::ToolResult { output, .. } if output == "What the web says about second."
    ));
    // Counting goes on from the saved rounds.
    let timings = metrics.generation_subtimings.unwrap();
    assert_eq!(timings.iterations, 3);
    assert_eq!(timings.tool_call_count, 2);
    assert!(journal.rounds.lock().unwrap().is_empty());
}

/// A turn that saves no answer — it failed or was stopped — releases the
/// pictures it stored, so none stays leased to a request that will never
/// attach them.
#[tokio::test]
async fn a_turn_that_saves_no_answer_releases_its_pictures() {
    let llm = Arc::new(Unanswering {
        asked: tokio::sync::Notify::new(),
    });
    let runtime = FakeRuntime::default();
    let turn = PreparedTurn::for_test(
        "stage-failed-conversation",
        "stage-failed-turn",
        llm,
        runtime.conversations.clone(),
        runtime.turn_cancellation.clone(),
    )
    .unwrap();
    let (sink, _) = recording_sink();
    let answered = Answered {
        flags: SearchFlags::from_preferences(None),
        router: None,
        user_message_id: "stage-failed-question".into(),
        message_tokens: 0,
        flow_start: std::time::Instant::now(),
    };

    let concluded = conclude(
        &runtime,
        &turn,
        None,
        answered,
        Err(AppError::InvalidState("Model returned no answer".into())),
        &sink,
        ConversationFlowTimingMetrics::default(),
    )
    .await;

    assert!(concluded.is_err());
    assert_eq!(
        runtime.images.calls(),
        ["release_request stage-failed-turn"]
    );
}

/// Picture guidance rides with the tool: present exactly when `find_image` is
/// offered, the same text every time, after the standing prompt.
#[test]
fn picture_guidance_comes_only_with_find_image() {
    let tool = |name: &str| lattice_core::application::ports::ToolDefinition {
        name: name.into(),
        description: String::new(),
        parameters: serde_json::json!({"type": "object", "properties": {}}),
    };
    let without = super::assemble::with_picture_guidance("Policy.", &[tool("web_search")]);
    assert_eq!(without, "Policy.");
    let with = super::assemble::with_picture_guidance(
        "Policy.",
        &[tool("web_search"), tool("find_image")],
    );
    assert!(with.starts_with("Policy.\n"));
    assert!(with.contains("find_image"));
    assert_eq!(
        with,
        super::assemble::with_picture_guidance("Policy.", &[tool("find_image")])
    );
}

#[test]
fn a_research_turn_resumes_needing_tools_only_when_it_was_planned_with_them() {
    use lattice_core::application::ports::llm_port::ToolSupport;
    let search = lattice_core::application::ports::ToolDefinition {
        name: "web_search".into(),
        description: "Search the web".into(),
        parameters: serde_json::json!({ "type": "object" }),
    };

    assert_eq!(
        prepare::resume_requirements(&[search]).tools,
        ToolSupport::Required
    );
    // Planned on a model without tools: it resumes on one without tools too.
    assert_eq!(
        prepare::resume_requirements(&[]).tools,
        ToolSupport::NotNeeded
    );
}
