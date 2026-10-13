//! The loop end to end against a scripted model and fake tools: the last
//! round's shape, calls run side by side, and condensing that lets a turn
//! read past its window.
use super::final_round::FINAL_ROUND_INSTRUCTION;
use super::research_notes::{CONDENSE_INSTRUCTION, NOTES_HEADER};
use super::*;
use crate::application::ports::llm_port::{
    CompletionRequest, CompletionResponse, ThinkingMode, ToolChoice,
};
use crate::application::ports::LLMPort;
use crate::features::conversation::chat::cancellation;
use crate::features::conversation::chat::ports::ChatTools;
use crate::features::conversation::chat::test_runtime::FakeRuntime;
use crate::features::conversation::chat::turn_record::{TurnStepDto, TurnStepState};
use crate::features::conversation::ConversationServiceTrait;
use crate::features::function_calling::domain::{FunctionCall, FunctionResult};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

const CONVERSATION: &str = "tool-loop-conversation";

/// Answers rounds from a script and condensing requests with fixed notes,
/// keeping every request it was sent.
struct Model {
    script: Mutex<VecDeque<CompletionResponse>>,
    notes: String,
    requests: Mutex<Vec<CompletionRequest>>,
}

impl Model {
    fn new(script: impl IntoIterator<Item = CompletionResponse>) -> Arc<Self> {
        Self::with_notes(script, "- Page A: the bridge opened in 1932 [1].")
    }

    fn with_notes(script: impl IntoIterator<Item = CompletionResponse>, notes: &str) -> Arc<Self> {
        Arc::new(Self {
            script: Mutex::new(script.into_iter().collect()),
            notes: notes.to_string(),
            requests: Mutex::default(),
        })
    }

    fn requests(&self) -> Vec<CompletionRequest> {
        self.requests.lock().unwrap().clone()
    }
}

fn is_condense_request(request: &CompletionRequest) -> bool {
    last_text(request).ends_with(CONDENSE_INSTRUCTION)
}

/// What the model reads last: the final message or tool result.
fn last_text(request: &CompletionRequest) -> &str {
    match request.input.last() {
        Some(CompletionInput::Message { content, .. }) => content,
        Some(CompletionInput::ToolResult { output, .. }) => output,
        _ => "",
    }
}

#[async_trait::async_trait]
impl LLMPort for Model {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.requests.lock().unwrap().push(request.clone());
        if is_condense_request(request) {
            return Ok(CompletionResponse::from_text(self.notes.clone()));
        }
        let reply = self.script.lock().unwrap().pop_front();
        match reply {
            Some(reply) => Ok(reply),
            // Out of script: wait to be stopped.
            None => std::future::pending().await,
        }
    }
    fn model_name(&self) -> &str {
        "scripted"
    }
    fn max_context_tokens(&self) -> usize {
        32_768
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// A reply that calls tools.
fn calls(calls: &[(&str, &str, Value)]) -> CompletionResponse {
    let calls: Vec<_> = calls
        .iter()
        .map(|(id, name, arguments)| CompletionInput::ToolCall {
            id: (*id).into(),
            name: (*name).into(),
            arguments: arguments.clone(),
        })
        .collect();
    CompletionResponse {
        tool_calls: calls.clone(),
        replay: calls,
        finish_reason: "tool_calls".into(),
        ..Default::default()
    }
}

fn reads(urls: &[&str]) -> CompletionResponse {
    let calls_made: Vec<_> = urls
        .iter()
        .enumerate()
        .map(|(index, url)| {
            (
                format!("read-{index}-{url}"),
                "fetch_url_content",
                json!({ "url": url }),
            )
        })
        .collect();
    calls(
        &calls_made
            .iter()
            .map(|(id, name, arguments)| (id.as_str(), *name, arguments.clone()))
            .collect::<Vec<_>>(),
    )
}

fn search(id: &str, query: &str) -> CompletionResponse {
    calls(&[(id, "web_search", json!({ "query": query }))])
}

fn tool(name: &str) -> ToolDefinition {
    ToolDefinition {
        name: name.into(),
        description: format!("The {name} tool."),
        parameters: json!({"type": "object", "properties": {}}),
    }
}

const PAGE_A: &str = "https://pages.example/a";
const PAGE_B: &str = "https://pages.example/b";
const PAGE_C: &str = "https://pages.example/c";

/// Pages, searches and a tool that may write, each taking as long as the
/// test says, with a log of when each started and ended.
#[derive(Default)]
struct Web {
    delays: HashMap<String, Duration>,
    /// Characters of text on every page.
    page_chars: usize,
    /// No call ever finishes.
    hang: bool,
    in_flight: AtomicUsize,
    most_in_flight: AtomicUsize,
    log: Mutex<Vec<String>>,
}

impl Web {
    fn with_delays(delays: &[(&str, u64)]) -> Arc<Self> {
        Arc::new(Self {
            delays: delays
                .iter()
                .map(|(key, ms)| ((*key).to_string(), Duration::from_millis(*ms)))
                .collect(),
            page_chars: 400,
            ..Self::default()
        })
    }

    fn with_pages_of(page_chars: usize) -> Arc<Self> {
        Arc::new(Self {
            page_chars,
            ..Self::default()
        })
    }

    fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
}

/// What a call is called in the log: the page, the query, or the tool.
fn subject(call: &FunctionCall) -> String {
    call.arguments
        .get("url")
        .or_else(|| call.arguments.get("query"))
        .and_then(Value::as_str)
        .map_or_else(|| call.name.clone(), str::to_string)
}

#[async_trait::async_trait]
impl ChatTools for Web {
    fn list_tools(&self) -> Vec<crate::features::function_calling::domain::ToolDefinition> {
        Vec::new()
    }
    async fn execute(&self, call: FunctionCall) -> Result<FunctionResult> {
        let subject = subject(&call);
        self.log.lock().unwrap().push(format!("start {subject}"));
        let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.most_in_flight.fetch_max(now, Ordering::SeqCst);
        if self.hang {
            std::future::pending::<()>().await;
        }
        tokio::time::sleep(self.delays.get(&subject).copied().unwrap_or_default()).await;
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        self.log.lock().unwrap().push(format!("end {subject}"));
        let data = match call.name.as_str() {
            "fetch_url_content" => {
                let content = format!("Text of {subject}. ")
                    .repeat(self.page_chars / 20 + 1)
                    .chars()
                    .take(self.page_chars)
                    .collect::<String>();
                json!({
                    "url": subject,
                    "title": format!("Page {subject}"),
                    "content": content,
                    "content_truncated": false,
                    "word_count": content.split_whitespace().count(),
                    "fetch_time_ms": 1.0,
                })
            }
            "web_search" => json!({
                "results": [{
                    "title": "Page A",
                    "url": PAGE_A,
                    "snippet": format!("What the web says about {subject}."),
                }],
                "query": subject,
                "result_count": 1,
            }),
            _ => json!({ "saved": true }),
        };
        Ok(FunctionResult::success(data))
    }
}

struct Run {
    outcome: Result<ToolLoopOutcome>,
    sources: Vec<SourceDto>,
    steps: Vec<TurnStepDto>,
}

async fn run(
    model: &Arc<Model>,
    web: &Arc<Web>,
    tools: &[ToolDefinition],
    input_budget: usize,
    rounds: usize,
    request_id: &str,
) -> Run {
    let runtime = FakeRuntime {
        tools: Some(web.clone()),
        ..FakeRuntime::default()
    };
    run_on(runtime, model, tools, input_budget, rounds, request_id).await
}

async fn run_on(
    runtime: FakeRuntime,
    model: &Arc<Model>,
    tools: &[ToolDefinition],
    input_budget: usize,
    rounds: usize,
    request_id: &str,
) -> Run {
    let conv_service: Arc<dyn ConversationServiceTrait> = runtime.conversations.clone();
    let llm: Arc<dyn LLMPort> = model.clone();
    let sink: ChatEventSink = Arc::new(|_| Ok(()));
    let recorder = TurnRecorder::silent(CONVERSATION, request_id);
    let settings = ToolOutputSettingsDto {
        // The citable passage each page adds is this long; kept short so a
        // test's window arithmetic is about the pages.
        excerpt_chars: 300,
        ..ToolOutputSettingsDto::default()
    };
    let mut sources = Vec::new();
    let mut trace = None;
    let outcome = run_agentic_tool_loop(
        &runtime,
        &conv_service,
        CONVERSATION,
        request_id,
        &llm,
        &sink,
        LoopRequest {
            input: vec![
                CompletionInput::Message {
                    role: "system".into(),
                    content: "Answer from sources.".into(),
                },
                CompletionInput::Message {
                    role: "user".into(),
                    content: "When did the bridge open?".into(),
                },
            ],
            tools: Some(tools),
            max_output_tokens: 512,
            input_budget,
            time_budget: Duration::from_secs(60),
            max_tool_rounds: rounds,
            research: None,
        },
        &[],
        &settings,
        &mut sources,
        &mut trace,
        FetchMemory::default(),
        &FocusScope::default(),
        None,
        &recorder,
    )
    .await;
    Run {
        outcome,
        sources,
        steps: recorder.steps(),
    }
}

/// Room for whole pages: nothing here is about the window.
const ROOMY: usize = 30_000;

fn json_of(value: &impl serde::Serialize) -> Value {
    serde_json::to_value(value).unwrap()
}

fn result_ids(request: &CompletionRequest) -> Vec<&str> {
    request
        .input
        .iter()
        .filter_map(|item| match item {
            CompletionInput::ToolResult { id, .. } => Some(id.as_str()),
            _ => None,
        })
        .collect()
}

fn results_mentioning<'r>(request: &'r CompletionRequest, text: &str) -> Vec<&'r str> {
    request
        .input
        .iter()
        .filter_map(|item| match item {
            CompletionInput::ToolResult { output, .. } if output.contains(text) => {
                Some(output.as_str())
            }
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn the_last_round_keeps_the_tools_and_forbids_calling_them() {
    let model = Model::new([
        search("s1", "bridge opening"),
        CompletionResponse::from_text("It opened in 1932 [1]."),
    ]);
    let web = Web::with_delays(&[]);
    let run = run(
        &model,
        &web,
        &[tool("web_search")],
        ROOMY,
        2,
        "last-round-keeps-tools",
    )
    .await;

    assert_eq!(run.outcome.unwrap().response, "It opened in 1932 [1].");
    let requests = model.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].tool_choice, None);
    assert_eq!(requests[1].tool_choice, Some(ToolChoice::None));
    // The same tools, so the prompt the server has cached is still a prefix
    // of this one.
    assert_eq!(json_of(&requests[1].tools), json_of(&requests[0].tools));
    let earlier = json_of(&requests[0].input);
    let later = json_of(&requests[1].input);
    assert_eq!(
        later.as_array().unwrap()[..earlier.as_array().unwrap().len()],
        earlier.as_array().unwrap()[..]
    );
    assert!(last_text(&requests[1]).ends_with(FINAL_ROUND_INSTRUCTION));
    // Said at the end of the last result, not as a message of its own.
    assert!(matches!(
        requests[1].input.last(),
        Some(CompletionInput::ToolResult { .. })
    ));
    // Thinking is off unless the user turns it on.
    assert!(requests
        .iter()
        .all(|request| request.thinking == ThinkingMode::Off));
}

/// With tools in the prompt and calls forbidden, Qwen3.5-9B thinking wrote
/// its call as text and answered nothing. A thinking answer gets no tools.
#[tokio::test]
async fn a_thinking_answer_is_asked_for_without_tools() {
    let model = Model::new([
        search("s1", "bridge opening"),
        CompletionResponse::from_text("It opened in 1932 [1]."),
    ]);
    let web = Web::with_delays(&[]);
    let runtime = FakeRuntime {
        tools: Some(web.clone()),
        answer_thinking: true,
        ..FakeRuntime::default()
    };
    let run = run_on(
        runtime,
        &model,
        &[tool("web_search")],
        ROOMY,
        2,
        "thinking-answer",
    )
    .await;

    assert_eq!(run.outcome.unwrap().response, "It opened in 1932 [1].");
    let requests = model.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].thinking, ThinkingMode::Off, "a tool round");
    assert!(requests[1].tools.is_empty());
    assert_eq!(requests[1].tool_choice, None);
    assert_eq!(requests[1].thinking, ThinkingMode::On);
    assert!(last_text(&requests[1]).ends_with(FINAL_ROUND_INSTRUCTION));
}

#[tokio::test]
async fn a_last_round_of_calls_is_asked_again_without_tools() {
    let model = Model::new([
        search("s1", "bridge opening"),
        search("s2", "bridge opening date"),
        CompletionResponse::from_text("It opened in 1932 [1]."),
    ]);
    let web = Web::with_delays(&[]);
    let run = run(
        &model,
        &web,
        &[tool("web_search")],
        ROOMY,
        2,
        "last-round-retry",
    )
    .await;

    assert_eq!(run.outcome.unwrap().response, "It opened in 1932 [1].");
    let requests = model.requests();
    assert_eq!(requests.len(), 3);
    let retry = &requests[2];
    assert!(retry.tools.is_empty(), "no tools at all on the retry");
    assert_eq!(retry.tool_choice, None);
    // The calls it was not allowed to make are not replayed, and the firmer
    // instruction takes the first one's place.
    assert_eq!(retry.input.len(), requests[1].input.len());
    assert!(!last_text(retry).contains(FINAL_ROUND_INSTRUCTION));
    assert!(last_text(retry).contains("Do not call any tools"));
    assert_eq!(web.log().len(), 2, "the last round's call never ran");
    assert!(run
        .steps
        .iter()
        .any(|step| step.label == "Asking again for the answer"));
}

/// Qwen 3.5 with thinking on writes the forbidden call into its text.
#[tokio::test]
async fn a_call_written_as_json_in_the_last_round_is_asked_again() {
    let model = Model::new([
        search("s1", "bridge opening"),
        CompletionResponse::from_text(
            r#"{"name": "web_search", "arguments": {"query": "bridge opening date"}}"#,
        ),
        CompletionResponse::from_text("It opened in 1932 [1]."),
    ]);
    let web = Web::with_delays(&[]);
    let run = run(
        &model,
        &web,
        &[tool("web_search")],
        ROOMY,
        2,
        "last-round-json-call",
    )
    .await;

    assert_eq!(run.outcome.unwrap().response, "It opened in 1932 [1].");
    assert_eq!(model.requests().len(), 3);
    assert!(model.requests()[2].tools.is_empty());
}

/// One retry, then the loop ends as it always has for a reply with no answer.
#[tokio::test]
async fn a_retry_that_calls_again_ends_as_an_empty_answer_always_has() {
    let model = Model::new([
        search("s1", "bridge opening"),
        search("s2", "again"),
        search("s3", "and again"),
        CompletionResponse::from_text("never asked for"),
    ]);
    let web = Web::with_delays(&[]);
    let run = run(
        &model,
        &web,
        &[tool("web_search")],
        ROOMY,
        2,
        "last-round-retry-fails",
    )
    .await;

    let error = run.outcome.expect_err("no answer was written");
    assert!(error.to_string().contains("no answer"), "{error}");
    assert_eq!(model.requests().len(), 3, "one retry, no more");
}

#[tokio::test]
async fn reads_in_one_round_run_together_and_come_back_in_call_order() {
    // The first call is the slowest, so they finish in reverse.
    let web = Web::with_delays(&[(PAGE_A, 300), (PAGE_B, 200), (PAGE_C, 100)]);
    let model = Model::new([
        reads(&[PAGE_A, PAGE_B, PAGE_C]),
        CompletionResponse::from_text("All three agree."),
    ]);
    let run = run(
        &model,
        &web,
        &[tool("fetch_url_content")],
        ROOMY,
        3,
        "concurrent-reads",
    )
    .await;

    assert_eq!(run.outcome.unwrap().response, "All three agree.");
    assert_eq!(web.most_in_flight.load(Ordering::SeqCst), 3);
    let log = web.log();
    let ends: Vec<_> = log.iter().filter(|line| line.starts_with("end")).collect();
    assert_eq!(
        ends,
        [
            &format!("end {PAGE_C}"),
            &format!("end {PAGE_B}"),
            &format!("end {PAGE_A}")
        ]
    );
    let answer_round = &model.requests()[1];
    assert_eq!(
        result_ids(answer_round),
        [
            format!("read-0-{PAGE_A}"),
            format!("read-1-{PAGE_B}"),
            format!("read-2-{PAGE_C}")
        ]
    );
    for (index, page) in [PAGE_A, PAGE_B, PAGE_C].iter().enumerate() {
        let CompletionInput::ToolResult { output, .. } = answer_round
            .input
            .iter()
            .filter(|item| matches!(item, CompletionInput::ToolResult { .. }))
            .nth(index)
            .unwrap()
        else {
            unreachable!()
        };
        assert!(output.contains(&format!("Text of {page}.")), "{output}");
    }
}

#[tokio::test]
async fn no_more_than_four_calls_are_in_flight_at_once() {
    let pages: Vec<String> = (0..6)
        .map(|n| format!("https://pages.example/{n}"))
        .collect();
    let delays: Vec<(&str, u64)> = pages.iter().map(|page| (page.as_str(), 50)).collect();
    let web = Web::with_delays(&delays);
    let model = Model::new([
        reads(&pages.iter().map(String::as_str).collect::<Vec<_>>()),
        CompletionResponse::from_text("Done."),
    ]);
    let run = run(
        &model,
        &web,
        &[tool("fetch_url_content")],
        ROOMY,
        3,
        "four-at-once",
    )
    .await;

    assert!(run.outcome.is_ok());
    assert_eq!(web.most_in_flight.load(Ordering::SeqCst), 4);
    assert_eq!(result_ids(&model.requests()[1]).len(), 6);
}

#[tokio::test]
async fn a_tool_that_may_write_runs_alone_and_in_order() {
    let web = Web::with_delays(&[(PAGE_A, 100), ("capture_into_journal", 50), (PAGE_B, 10)]);
    let model = Model::new([
        calls(&[
            ("c1", "fetch_url_content", json!({ "url": PAGE_A })),
            ("c2", "capture_into_journal", json!({})),
            ("c3", "fetch_url_content", json!({ "url": PAGE_B })),
        ]),
        CompletionResponse::from_text("Saved."),
    ]);
    let run = run(
        &model,
        &web,
        &[tool("fetch_url_content"), tool("capture_into_journal")],
        ROOMY,
        3,
        "writes-run-alone",
    )
    .await;

    assert!(run.outcome.is_ok());
    assert_eq!(
        web.log(),
        [
            format!("start {PAGE_A}"),
            format!("end {PAGE_A}"),
            "start capture_into_journal".to_string(),
            "end capture_into_journal".to_string(),
            format!("start {PAGE_B}"),
            format!("end {PAGE_B}"),
        ]
    );
    assert_eq!(web.most_in_flight.load(Ordering::SeqCst), 1);
    assert_eq!(result_ids(&model.requests()[1]), ["c1", "c2", "c3"]);
}

#[tokio::test]
async fn stop_ends_a_round_of_reads_without_waiting_for_them() {
    let request_id = "concurrent-reads-stopped";
    assert!(cancellation::begin_turn_within(
        request_id,
        CONVERSATION,
        None
    ));
    let web = Arc::new(Web {
        hang: true,
        ..Web::default()
    });
    let model = Model::new([reads(&[PAGE_A, PAGE_B, PAGE_C])]);
    let tools = [tool("fetch_url_content")];
    let stop = async {
        while web.in_flight.load(Ordering::SeqCst) < 3 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(cancellation::request_cancel(CONVERSATION, Some(request_id)));
    };
    let (run, ()) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(run(&model, &web, &tools, ROOMY, 3, request_id), stop)
    })
    .await
    .expect("stop ends the round, not the time budget");
    cancellation::finish_turn(request_id);

    let error = run.outcome.expect_err("a stopped turn has no answer");
    assert!(cancellation::is_cancellation(&error), "{error}");
    let reads: Vec<_> = run
        .steps
        .iter()
        .filter(|step| step.kind == TurnStepKind::ReadPage)
        .collect();
    assert_eq!(reads.len(), 3);
    assert!(reads.iter().all(|step| step.state == TurnStepState::Failed));
}

/// A 4,000-token budget: 12,000 characters, about one page.
const ONE_PAGE_WINDOW: usize = 4_000;

/// Without condensing, the second round finds the window full and has to
/// answer from page A alone.
#[tokio::test]
async fn a_turn_that_fills_its_window_condenses_and_keeps_reading() {
    let web = Web::with_pages_of(30_000);
    let model = Model::new([
        reads(&[PAGE_A]),
        reads(&[PAGE_B]),
        CompletionResponse::from_text("It opened in 1932 [1] and was widened in 1960 [2]."),
    ]);
    let run = run(
        &model,
        &web,
        &[tool("fetch_url_content")],
        ONE_PAGE_WINDOW,
        5,
        "condense-and-continue",
    )
    .await;

    assert_eq!(
        run.outcome.unwrap().response,
        "It opened in 1932 [1] and was widened in 1960 [2]."
    );
    let pages_read: Vec<_> = web
        .log()
        .into_iter()
        .filter(|line| line.starts_with("start"))
        .collect();
    assert_eq!(
        pages_read,
        [format!("start {PAGE_A}"), format!("start {PAGE_B}")]
    );
    let requests = model.requests();
    let kinds: Vec<_> = requests.iter().map(is_condense_request).collect();
    assert_eq!(kinds, [false, true, false, true, false]);
    // The round after condensing is an ordinary one: tools callable, no
    // instruction to stop, and the notes in place of page A's text.
    let after = &requests[2];
    assert_eq!(after.tool_choice, None);
    assert!(!last_text(after).contains(FINAL_ROUND_INSTRUCTION));
    assert!(last_text(after).starts_with(NOTES_HEADER));
    assert!(results_mentioning(after, &format!("Text of {PAGE_A}")).is_empty());
    // The condensing request is the round before it and an instruction: the
    // cached prompt is a prefix of it, tools and all.
    let condense = &requests[1];
    assert_eq!(json_of(&condense.tools), json_of(&requests[0].tools));
    assert_eq!(condense.tool_choice, Some(ToolChoice::None));
    assert!(run.steps.iter().any(
        |step| step.label == "Condensing what it has read" && step.state == TurnStepState::Done
    ));
}

/// The sources, their numbers and the step cards live outside the
/// transcript, so an answer written after condensing cites as one written
/// before it.
#[tokio::test]
async fn citations_from_before_condensing_still_resolve() {
    let web = Web::with_pages_of(30_000);
    let model = Model::new([
        reads(&[PAGE_A]),
        reads(&[PAGE_B]),
        CompletionResponse::from_text("It opened in 1932 [1]."),
    ]);
    let run = run(
        &model,
        &web,
        &[tool("fetch_url_content")],
        ONE_PAGE_WINDOW,
        5,
        "condense-citations",
    )
    .await;

    assert!(run.outcome.is_ok());
    let cited = run
        .sources
        .iter()
        .find(|source| source.citation_id == Some(1))
        .expect("[1] still has a source");
    assert_eq!(cited.path.as_deref(), Some(PAGE_A));
    // The model was shown the number it has to keep.
    let condense = &model.requests()[1];
    assert!(!results_mentioning(condense, "Citable passage [1]").is_empty());
    // The research deck's card for page A still links to it.
    assert!(run
        .steps
        .iter()
        .any(|step| step.kind == TurnStepKind::ReadPage
            && step.state == TurnStepState::Done
            && step.links.iter().any(|link| link.url == PAGE_A)));
}

/// Told "it is in your context above" after the text was condensed away, the
/// model would have nothing to read.
#[tokio::test]
async fn a_page_condensed_away_is_read_again_when_asked_for() {
    let web = Web::with_pages_of(30_000);
    let model = Model::new([
        reads(&[PAGE_A]),
        reads(&[PAGE_A]),
        CompletionResponse::from_text("It opened in 1932 [1]."),
    ]);
    let run = run(
        &model,
        &web,
        &[tool("fetch_url_content")],
        ONE_PAGE_WINDOW,
        5,
        "condensed-page-reread",
    )
    .await;

    assert!(run.outcome.is_ok());
    let reads = web
        .log()
        .iter()
        .filter(|line| *line == &format!("start {PAGE_A}"))
        .count();
    assert_eq!(reads, 2);
}

/// Notes that save nothing are refused, and the full-window ending is still
/// there behind them.
#[tokio::test]
async fn when_condensing_saves_nothing_a_full_window_still_ends_the_research() {
    let web = Web::with_pages_of(30_000);
    let model = Model::with_notes(
        [
            reads(&[PAGE_A]),
            CompletionResponse::from_text("It opened in 1932 [1]."),
        ],
        &"n".repeat(40_000),
    );
    let run = run(
        &model,
        &web,
        &[tool("fetch_url_content")],
        ONE_PAGE_WINDOW,
        5,
        "condense-refused",
    )
    .await;

    assert_eq!(run.outcome.unwrap().response, "It opened in 1932 [1].");
    let requests = model.requests();
    assert_eq!(requests.len(), 3);
    assert!(is_condense_request(&requests[1]));
    assert!(last_text(&requests[2]).ends_with(FINAL_ROUND_INSTRUCTION));
    assert!(!results_mentioning(&requests[2], &format!("Text of {PAGE_A}")).is_empty());
}
