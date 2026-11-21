//! Stage tests against a fake runtime: no database, no model, no container.
use super::assemble::{PlannedRequest, RenderedPrompt};
use super::prepare::PreparedTurn;
use super::research::{self, AssembledTurn, ResearchJournal, ResearchRound, Rounds};
use super::*;
use crate::features::conversation::chat::test_runtime::{FakeRuntime, Scripted};
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use lattice_core::application::ports::turn_source::SourcePassage;
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::AppError;
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
    fn enforces(
        &self,
        _schema: &lattice_core::application::services::model_contract::shape::Schema,
    ) -> bool {
        false
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
        pictures: Default::default(),
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
    assert!(matches!(error, AppError::Cancelled(_)), "{error}");
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
    let flags = classify::classify_turn(&runtime, &first, None).await.flags;
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
    let flags = classify::classify_turn(&runtime, &second, None).await.flags;
    assert!(!flags.web_fallback);
    assert!(!flags.web_planned());
    assert_eq!(
        llm.requests().len(),
        2,
        "each turn asked the intent step once"
    );
}

/// The question the intent step writes for a follow-up comes out beside the
/// flags, for the search to take.
#[tokio::test]
async fn a_followup_comes_with_the_question_it_stands_for() {
    let llm = Scripted::new([CompletionResponse::from_text(
        r#"{"needsKnowledgeBase":true,"needsWeb":false,"isFollowup":true,"standaloneQuestion":"What is the hold on the edge ring?","confidence":0.9}"#,
    )]);
    let runtime = FakeRuntime::default();
    let turn = PreparedTurn::for_test(
        "intent-followup-conversation",
        "intent-followup-turn",
        llm.clone(),
        runtime.conversations.clone(),
        runtime.turn_cancellation.clone(),
    )
    .unwrap();
    let inferred = classify::classify_turn(&runtime, &turn, None).await;
    assert!(inferred.flags.force_followup_mode);
    assert_eq!(
        inferred.standalone_question.as_deref(),
        Some("What is the hold on the edge ring?")
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
    assert_eq!(requests[0].input().len(), planned_len + saved.len());
    assert!(matches!(
        &requests[0].input()[planned_len + 3],
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
    let without = super::assemble::with_tool_guidance("Policy.", &[tool("web_search")]);
    assert_eq!(without, "Policy.");
    let with =
        super::assemble::with_tool_guidance("Policy.", &[tool("web_search"), tool("find_image")]);
    assert!(with.starts_with("Policy.\n"));
    assert!(with.contains("find_image"));
    assert_eq!(
        with,
        super::assemble::with_tool_guidance("Policy.", &[tool("find_image")])
    );
}

/// The one line on searching memory: it comes with the tool, tells the model
/// when not to reach for it, and stays ahead of the picture guidance so a web
/// turn does not move it.
#[test]
fn the_line_on_searching_memory_comes_only_with_the_tool() {
    let tool = |name: &str| lattice_core::application::ports::ToolDefinition {
        name: name.into(),
        description: String::new(),
        parameters: serde_json::json!({"type": "object", "properties": {}}),
    };
    let memory = tool(super::super::history_tools::TOOL_SEARCH_SAVED_KNOWLEDGE);
    assert_eq!(
        super::assemble::with_tool_guidance("Policy.", &[tool("web_search")]),
        "Policy."
    );
    let with = super::assemble::with_tool_guidance("Policy.", std::slice::from_ref(&memory));
    assert!(with.contains("search_saved_knowledge only for something from before"));
    let both = super::assemble::with_tool_guidance("Policy.", &[memory, tool("find_image")]);
    assert!(both.starts_with(&with));
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

/// A registry holding the library's core tools.
struct LibraryTools;

#[async_trait::async_trait]
impl crate::features::conversation::chat::ports::ChatTools for LibraryTools {
    fn list_tools(&self) -> Vec<lattice_core::application::contracts::tools::ToolDefinition> {
        CORE_TOOL_NAMES
            .iter()
            .map(
                |name| lattice_core::application::contracts::tools::ToolDefinition {
                    name: (*name).into(),
                    description: String::new(),
                    input_schema: serde_json::json!({ "type": "object" }),
                },
            )
            .collect()
    }
    async fn execute(
        &self,
        _call: lattice_core::application::contracts::tools::FunctionCall,
    ) -> Result<lattice_core::application::contracts::tools::FunctionResult> {
        unreachable!("only listed")
    }
}

/// A model that takes tools.
struct ToolCalling;

#[async_trait::async_trait]
impl LLMPort for ToolCalling {
    async fn complete(&self, _request: &CompletionRequest) -> Result<CompletionResponse> {
        unreachable!("only offered tools")
    }
    fn model_name(&self) -> &str {
        "tool-calling"
    }
    fn max_context_tokens(&self) -> usize {
        32_768
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
    fn supports_tool_calling(&self) -> bool {
        true
    }
    fn enforces(
        &self,
        _schema: &lattice_core::application::services::model_contract::shape::Schema,
    ) -> bool {
        false
    }
}

/// A folder that offers one tool of its own.
#[derive(Debug)]
struct Folder;

#[async_trait::async_trait]
impl lattice_core::application::ports::turn_source::TurnSourcePort for Folder {
    fn policy(&self) -> lattice_core::application::ports::turn_source::TurnSourcePolicy {
        lattice_core::application::ports::turn_source::TurnSourcePolicy {
            routes_to_library: false,
            offers_library_tools: false,
            infers_intent: false,
            verifies_cited_only: true,
            drops_session_talk: false,
            tool_rounds: 10,
            evidence_template: None,
        }
    }
    fn owns_tool(&self, tool: &str) -> bool {
        tool == "read_file"
    }
    fn reads_only(&self, tool: &str) -> bool {
        self.owns_tool(tool)
    }
    fn activity_label(&self, tool: &str, _arguments: &serde_json::Value) -> String {
        tool.into()
    }
    fn add_tools(&self, tools: &mut Vec<lattice_core::application::ports::ToolDefinition>) {
        tools.push(lattice_core::application::ports::ToolDefinition {
            name: "read_file".into(),
            description: String::new(),
            parameters: serde_json::json!({ "type": "object" }),
        });
    }
    async fn context_block(
        &self,
        _max_chars: usize,
        _tools_offered: bool,
        _question: Option<&str>,
    ) -> String {
        String::new()
    }
    async fn execute_tool(
        &self,
        _call: lattice_core::application::contracts::tools::FunctionCall,
        _max_chars: usize,
    ) -> Result<lattice_core::application::contracts::tools::FunctionResult> {
        unreachable!("only offered tools")
    }
}

fn passage(chunk: &str, content: &str) -> SourcePassage {
    SourcePassage {
        label: format!("Ranking › Fusion › Notes ({chunk})"),
        source: serde_json::from_value(serde_json::json!({
            "documentId": "course:version-1",
            "chunkId": chunk,
            "content": content,
            "score": 1.0,
            "fileName": "Notes",
            "filePath": "",
            "mimeType": "text/plain",
            "category": "Course source",
            "fileSizeBytes": 0,
            "modifiedAt": "",
        }))
        .unwrap(),
    }
}

/// A source that finds a passage for every question.
#[derive(Debug)]
struct Course {
    policy: lattice_core::application::ports::turn_source::TurnSourcePolicy,
}

impl Course {
    fn new() -> Self {
        Self {
            policy: lattice_core::application::ports::turn_source::TurnSourcePolicy {
                routes_to_library: false,
                offers_library_tools: true,
                infers_intent: false,
                verifies_cited_only: false,
                drops_session_talk: false,
                tool_rounds: 10,
                evidence_template: None,
            },
        }
    }
}

#[async_trait::async_trait]
impl lattice_core::application::ports::turn_source::TurnSourcePort for Course {
    fn policy(&self) -> lattice_core::application::ports::turn_source::TurnSourcePolicy {
        self.policy
    }
    fn owns_tool(&self, tool: &str) -> bool {
        tool == "search_course"
    }
    fn reads_only(&self, _tool: &str) -> bool {
        true
    }
    fn activity_label(&self, tool: &str, _arguments: &serde_json::Value) -> String {
        tool.into()
    }
    fn add_tools(&self, _tools: &mut Vec<lattice_core::application::ports::ToolDefinition>) {}
    async fn context_block(&self, _: usize, _: bool, _: Option<&str>) -> String {
        "Course: Search.".into()
    }
    async fn passages(&self, _question: &str, _max_chars: usize) -> Vec<SourcePassage> {
        vec![passage("chunk-1", "Fusion sums reciprocal ranks.")]
    }
    async fn execute_tool(
        &self,
        _call: lattice_core::application::contracts::tools::FunctionCall,
        _max_chars: usize,
    ) -> Result<lattice_core::application::contracts::tools::FunctionResult> {
        unreachable!("not run")
    }
}

fn course_turn(runtime: &FakeRuntime, source: TurnSource) -> PreparedTurn {
    let llm: Arc<dyn LLMPort> = Arc::new(ToolCalling);
    let mut turn = PreparedTurn::for_test(
        "stage-course-conversation",
        "stage-course-turn",
        llm,
        runtime.conversations.clone(),
        runtime.turn_cancellation.clone(),
    )
    .unwrap();
    turn.source = Some(source);
    turn
}

fn rendered(
    turn: &PreparedTurn,
    passages: Vec<SourcePassage>,
    evidence: lattice_core::application::services::context_assembler::EvidenceBudget,
) -> RenderedPrompt {
    let mut retrieval = retrieval::empty_pipeline_outcome();
    retrieval.evidence = evidence;
    let carried = super::carry::CarriedMaterial {
        attachments: TurnAttachments::default(),
        pictures: Default::default(),
        source_context: Some("Course: Search.".into()),
        source_passages: passages,
        attachment_names: Vec::new(),
        attachment_ids: Vec::new(),
    };
    let retrieved = super::retrieve::Retrieved {
        prior_sources: Vec::new(),
        prior_left_out: 0,
        retrieval,
    };
    let (sink, _) = recording_sink();
    super::assemble::render_prompt(
        turn,
        SearchFlags::from_preferences(None),
        None,
        carried,
        retrieved,
        &sink,
        &mut ConversationFlowTimingMetrics::default(),
    )
}

/// A course's passages reach the prompt as a library passage does: numbered,
/// and in the list the answer's citations resolve against.
#[test]
fn a_source_s_passages_join_the_turn_s_numbered_passages() {
    use lattice_core::application::services::context_assembler::EvidenceBudget;
    let runtime = FakeRuntime::default();
    let turn = course_turn(&runtime, Arc::new(Course::new()));

    let prompt = rendered(
        &turn,
        vec![passage("chunk-1", "Fusion sums reciprocal ranks.")],
        EvidenceBudget::new(10_000),
    );

    let message = &prompt.enhanced_message;
    assert!(
        message.contains(
            "[s1] Document: Ranking › Fusion › Notes (chunk-1)\nContent: Fusion sums reciprocal ranks."
        ),
        "{message}"
    );
    assert!(
        !message.contains("Document ID:"),
        "it reads on with the course's tools: {message}"
    );
    let passage_pos = message.find("[s1] Document:").expect("passage marker");
    let question_pos = message.find("Question:").expect("question line");
    assert!(passage_pos < question_pos, "{message}");
    assert!(
        message.ends_with("translating any passage in another."),
        "{message}"
    );
    assert!(!message.ends_with("Answer:"), "{message}");
    assert_eq!(prompt.sources[0].citation_id, Some(1));
    assert_eq!(prompt.sources[0].document_id, "course:version-1");
    assert_eq!(prompt.sources[0].content, "Fusion sums reciprocal ranks.");
    assert!(prompt.has_grounded_context);
}

/// A passage the budget cut is no source of the answer, as for the library.
#[test]
fn a_source_s_passage_the_budget_cut_is_no_source() {
    use lattice_core::application::services::context_assembler::EvidenceBudget;
    let runtime = FakeRuntime::default();
    let turn = course_turn(&runtime, Arc::new(Course::new()));

    let prompt = rendered(
        &turn,
        vec![passage("chunk-1", &"word ".repeat(400))],
        EvidenceBudget::new(5),
    );

    assert!(prompt.sources.is_empty());
    assert!(!prompt.enhanced_message.contains("[1] Document:"));
}

async fn carried_passages(source: TurnSource, closed_book: bool) -> usize {
    use lattice_core::application::services::context_assembler::EvidenceBudget;
    let runtime = FakeRuntime::default();
    let turn = course_turn(&runtime, source);
    let flags = SearchFlags {
        closed_book,
        ..SearchFlags::from_preferences(None)
    };
    let mut evidence = EvidenceBudget::new(10_000);
    let carried = super::carry::carry_material(
        &runtime,
        &turn,
        flags,
        Vec::new(),
        Vec::new(),
        &mut evidence,
    )
    .await;
    carried.source_passages.len()
}

/// A closed-book turn searches nothing, and a folder cites by line.
#[tokio::test]
async fn only_an_open_turn_on_a_source_with_passages_carries_them() {
    assert_eq!(carried_passages(Arc::new(Course::new()), false).await, 1);
    assert_eq!(carried_passages(Arc::new(Course::new()), true).await, 0);
    assert_eq!(carried_passages(Arc::new(Folder), false).await, 0);
}

/// A tutor keeps a chat's tools: its library search stays offered without the
/// user turning it on.
#[test]
fn a_source_that_offers_the_library_keeps_its_tools() {
    let runtime = FakeRuntime {
        tools: Some(Arc::new(LibraryTools)),
        ..FakeRuntime::default()
    };
    let llm: Arc<dyn LLMPort> = Arc::new(ToolCalling);
    let course: TurnSource = Arc::new(Course::new());
    let offered: Vec<String> = super::assemble::offered_tools(
        &runtime,
        &llm,
        &[],
        Some(&course),
        SearchFlags::from_preferences(None),
        None,
        super::assemble::Grounding {
            can_open_pages: false,
            has_grounded_context: false,
        },
    )
    .into_iter()
    .map(|tool| tool.name)
    .collect();
    for name in LIBRARY_TOOL_NAMES {
        assert!(
            offered.iter().any(|tool| tool == name),
            "{name} in {offered:?}"
        );
    }
}

/// An Explorer turn reads the library only when the user turns it on, so
/// only then is it handed the tools that search it; a document the message
/// names stays readable either way.
#[test]
fn an_explorer_turn_searches_the_library_only_when_the_user_turns_it_on() {
    let runtime = FakeRuntime {
        tools: Some(Arc::new(LibraryTools)),
        ..FakeRuntime::default()
    };
    let llm: Arc<dyn LLMPort> = Arc::new(ToolCalling);
    let folder: TurnSource = Arc::new(Folder);
    let offered = |source: Option<&TurnSource>, force_kb_search: bool| -> Vec<String> {
        let flags = SearchFlags {
            force_kb_search,
            ..SearchFlags::from_preferences(None)
        };
        super::assemble::offered_tools(
            &runtime,
            &llm,
            &[],
            source,
            flags,
            None,
            super::assemble::Grounding {
                can_open_pages: false,
                has_grounded_context: false,
            },
        )
        .into_iter()
        .map(|tool| tool.name)
        .collect()
    };
    let has = |tools: &[String], name: &str| tools.iter().any(|tool| tool == name);

    let folder_only = offered(Some(&folder), false);
    for name in LIBRARY_TOOL_NAMES {
        assert!(!has(&folder_only, name), "{name} in {folder_only:?}");
    }
    for name in ["get_document", "list_attachments", "read_file"] {
        assert!(
            has(&folder_only, name),
            "{name} missing from {folder_only:?}"
        );
    }

    let with_library = offered(Some(&folder), true);
    for name in LIBRARY_TOOL_NAMES {
        assert!(
            has(&with_library, name),
            "{name} missing from {with_library:?}"
        );
    }

    let plain_chat = offered(None, false);
    for name in LIBRARY_TOOL_NAMES {
        assert!(has(&plain_chat, name), "{name} missing from {plain_chat:?}");
    }
}

#[test]
fn tool_preferences_default_to_hybrid_and_keep_kb_and_strict_sources_orthogonal() {
    let defaults = ToolPreferences::default();
    assert!(!defaults.strict_sources);
    assert!(!defaults.knowledge_base);
    let default_flags = SearchFlags::from_preferences(Some(&defaults));
    assert!(!default_flags.strict_sources);
    assert!(!default_flags.user_requested_kb);
    assert!(!default_flags.force_kb_search);

    let kb_only = ToolPreferences {
        knowledge_base: true,
        strict_sources: false,
        ..ToolPreferences::default()
    };
    let kb_only_flags = SearchFlags::from_preferences(Some(&kb_only));
    assert!(kb_only_flags.user_requested_kb);
    assert!(kb_only_flags.force_kb_search);
    assert!(!kb_only_flags.strict_sources);

    let strict_only = ToolPreferences {
        knowledge_base: false,
        strict_sources: true,
        ..ToolPreferences::default()
    };
    let strict_only_flags = SearchFlags::from_preferences(Some(&strict_only));
    assert!(!strict_only_flags.user_requested_kb);
    assert!(!strict_only_flags.force_kb_search);
    assert!(strict_only_flags.strict_sources);
}

#[tokio::test]
async fn background_verification_respects_grounding_mode_and_source_policy() {
    let runtime = FakeRuntime::default();
    let llm: Arc<dyn LLMPort> = Arc::new(ToolCalling);
    let (sink, _) = recording_sink();
    let make_verification = |turn: &PreparedTurn, flags: SearchFlags| {
        let mode = super::finalize::grounding_mode_for(turn, flags);
        super::finalize::background_verification_for(
            &runtime,
            turn,
            flags,
            mode,
            "msg-1".into(),
            crate::features::conversation::chat::reply_text::ReplyText::default(),
            Vec::new(),
            TurnRecordDto::default(),
            Instant::now(),
            &sink,
        )
    };

    let chat_turn = PreparedTurn::for_test(
        "conv-chat",
        "turn-chat",
        Arc::clone(&llm),
        runtime.conversations.clone(),
        runtime.turn_cancellation.clone(),
    )
    .unwrap();

    let hybrid_flags = SearchFlags::from_preferences(Some(&ToolPreferences::default()));
    let hybrid = make_verification(&chat_turn, hybrid_flags);
    assert!(hybrid.verifies_cited_only);
    assert_eq!(hybrid.grounding_mode, Some("hybrid"));

    let strict_flags = SearchFlags::from_preferences(Some(&ToolPreferences {
        strict_sources: true,
        ..ToolPreferences::default()
    }));
    let strict = make_verification(&chat_turn, strict_flags);
    assert!(!strict.verifies_cited_only);
    assert_eq!(strict.grounding_mode, Some("strict"));

    // Explorer (`Folder`) always verifies cited claims only, even if strict_sources is set.
    let mut explorer_turn = PreparedTurn::for_test(
        "conv-explorer",
        "turn-explorer",
        Arc::clone(&llm),
        runtime.conversations.clone(),
        runtime.turn_cancellation.clone(),
    )
    .unwrap();
    explorer_turn.source = Some(Arc::new(Folder));
    let explorer = make_verification(&explorer_turn, strict_flags);
    assert!(explorer.verifies_cited_only);
    assert_eq!(explorer.grounding_mode, None);

    // Tutor (`drops_session_talk: true`) verifies cited claims only in hybrid mode,
    // and checks full non-session-talk claims in strict mode.
    let mut tutor_policy = Course::new().policy;
    tutor_policy.drops_session_talk = true;
    let mut tutor_turn = PreparedTurn::for_test(
        "conv-tutor",
        "turn-tutor",
        Arc::clone(&llm),
        runtime.conversations.clone(),
        runtime.turn_cancellation.clone(),
    )
    .unwrap();
    tutor_turn.source = Some(Arc::new(Course {
        policy: tutor_policy,
    }));
    let tutor_hybrid = make_verification(&tutor_turn, hybrid_flags);
    assert!(tutor_hybrid.verifies_cited_only);
    assert!(!tutor_hybrid.drops_session_talk);
    assert_eq!(tutor_hybrid.grounding_mode, None);

    let tutor_strict = make_verification(&tutor_turn, strict_flags);
    assert!(!tutor_strict.verifies_cited_only);
    assert!(tutor_strict.drops_session_talk);
    assert_eq!(tutor_strict.grounding_mode, None);
}

#[tokio::test]
async fn zero_claim_turns_skip_background_verification_with_immediate_metadata() {
    use super::finalize::{initial_verification_metadata, needs_background_verification};
    use crate::features::conversation::chat::reply_text::ReplyText;

    let runtime = FakeRuntime::default();
    let llm: Arc<dyn LLMPort> = Arc::new(ToolCalling);
    let turn = PreparedTurn::for_test(
        "conv-zero-flash",
        "turn-zero-flash",
        llm,
        runtime.conversations.clone(),
        runtime.turn_cancellation.clone(),
    )
    .unwrap();

    // No sources at all -> no background verification, immediate non-pending metadata.
    let no_sources_needs = needs_background_verification(true, true, &ReplyText::default(), &[]);
    assert!(!no_sources_needs);
    let no_sources_meta =
        initial_verification_metadata(true, no_sources_needs, true, Some("hybrid"));
    assert_eq!(no_sources_meta["enabled"], true);
    assert_eq!(no_sources_meta["claimsEvaluated"], 0);
    assert_eq!(no_sources_meta["citedOnly"], true);
    assert_eq!(no_sources_meta["groundingMode"], "hybrid");
    assert!(no_sources_meta.get("pending").is_none());

    // Hybrid turn with sources present, but reply made zero citations -> no background check.
    let dummy_source: SourceDto = serde_json::from_value(serde_json::json!({
        "documentId": "doc-1",
        "chunkId": "chunk-1",
        "content": "passage",
        "score": 1.0,
        "fileName": "Notes",
        "filePath": "/notes.md",
        "mimeType": "text/markdown",
        "category": "Markdown",
        "fileSizeBytes": 10,
        "modifiedAt": "",
    }))
    .unwrap();
    let uncited_reply = ReplyText {
        text: "A pure general-knowledge answer.".into(),
        references: Vec::new(),
    };
    assert!(!needs_background_verification(
        true,
        true,
        &uncited_reply,
        std::slice::from_ref(&dummy_source),
    ));

    // Strict turn with sources present and zero citations STILL runs background verification
    // so uncited factual claims are checked.
    assert!(needs_background_verification(
        true,
        false,
        &uncited_reply,
        std::slice::from_ref(&dummy_source),
    ));
    let pending_strict = initial_verification_metadata(true, true, false, Some("strict"));
    assert_eq!(pending_strict["enabled"], true);
    assert_eq!(pending_strict["pending"], true);
    assert_eq!(pending_strict["groundingMode"], "strict");
    assert!(pending_strict.get("citedOnly").is_none());

    // `infer_turn_intent` sets `skip_auto_kb` for trivial greetings in hybrid auto mode,
    // and keeps `skip_auto_kb: false` in strict mode.
    let cancel = tokio_util::sync::CancellationToken::new();
    let hybrid_greeting = crate::features::conversation::chat::routing::infer_turn_intent(
        &runtime,
        &turn.llm,
        None,
        SearchFlags::from_preferences(None),
        "hello!",
        &[],
        cancel.clone(),
    )
    .await;
    assert!(hybrid_greeting.flags.skip_auto_kb);

    let strict_greeting = crate::features::conversation::chat::routing::infer_turn_intent(
        &runtime,
        &turn.llm,
        Some(&ToolPreferences {
            strict_sources: true,
            ..ToolPreferences::default()
        }),
        SearchFlags::from_preferences(Some(&ToolPreferences {
            strict_sources: true,
            ..ToolPreferences::default()
        })),
        "hello!",
        &[],
        cancel,
    )
    .await;
    assert!(!strict_greeting.flags.skip_auto_kb);
}
