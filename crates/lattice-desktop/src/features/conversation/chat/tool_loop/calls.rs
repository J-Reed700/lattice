//! The calls one round asked for.
//!
//! A round that asked for three pages used to read them one after another, and
//! a page read is mostly waiting on someone else's server: the turn sat idle
//! for the sum of every fetch while a single-slot local model had nothing to
//! do. Calls that only read now run together, a few at a time; anything else
//! runs alone, in the order the model wrote it. Results go back in call order
//! whatever order they finish in, so the transcript the next round replays —
//! and the cached prefix it reuses — never depends on timing.

use super::call_labels::{
    available_tool_names, canonical_tool_name, is_tool_allowed, recalled_page_notice,
    tool_activity_label, tool_argument_summary, tool_step_kind,
};
use super::clock::TurnClock;
use super::document_evidence::DocumentEvidence;
use super::document_progress::DocumentProgress;
use super::images::{self, FIND_IMAGE};
use super::tool_sources::{collect_tool_sources, render_with_citations};
use super::{
    chars_in_flight, elapsed_ms, emit_cancelled_stream, scoped_document_tools,
    tool_result_allowance, RequestedCall, ToolLoopTimingMetrics, ToolOutputSettingsDto,
};
use crate::features::conversation::chat::cancellation::cancelled_error;
use crate::features::conversation::chat::dto::SourceDto;
use crate::features::conversation::chat::fetch_memory::{self, Delivery, FetchMemory, Recall};
use crate::features::conversation::chat::focus::FocusScope;
use crate::features::conversation::chat::history_tools::{self, HistoryToolMemo};
use crate::features::conversation::chat::ports::{
    ChatRecords, ChatRuntime, ExplorerTurn, FetchUrlContentOutput,
};
use crate::features::conversation::chat::retrieval::{
    assign_citation_ids, deduplicate_sources, document_page_room, fetched_page_text_room,
    format_tool_result, merge_tool_sources, record_tool_document_references,
};
use crate::features::conversation::chat::turn_record::{StepGuard, TurnRecorder, TurnStepKind};
use crate::features::conversation::chat::{
    source_snapshots, web_steps, ChatEventSink, ChatStreamEventDto, RetrievalTraceDto,
};
use crate::features::conversation::ConversationServiceTrait;
use futures::{FutureExt, StreamExt};
use lattice_core::application::contracts::tools::explorer::is_explorer_tool;
use lattice_core::application::contracts::tools::{FunctionCall, FunctionResult};
use lattice_core::application::ports::llm_port::CompletionInput;
use lattice_core::application::ports::ToolDefinition;
use lattice_core::shared::error::Result;
use lattice_core::shared::http::url_identity::identity;
use std::collections::HashSet;
use std::ops::Range;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// Calls of one round in flight at once: enough to overlap a round's page
/// reads, few enough that one round does not hammer a search provider or a
/// site's queue.
pub(super) const MAX_CONCURRENT_CALLS: usize = 4;

/// Whether a tool only reads, so it may run alongside the other calls of its
/// round.
///
/// Named one by one rather than assumed: a tool added later runs alone until
/// someone decides otherwise, because a write run beside the reads it should
/// have followed is a bug no test of the write itself would catch. The
/// conversation-history tools stay sequential although they only read: they
/// share one memo per turn whose answers depend on what was already read, so
/// together their results would depend on which finished first. A custom
/// query tool is the user's own endpoint and could do anything. `find_image`
/// writes: it stores pictures and numbers their refs after the ones the turn
/// already holds, so two at once could hand out the same `img-N`.
pub(super) fn runs_alongside_others(tool: &str) -> bool {
    matches!(
        tool,
        "web_search"
            | "find_sources"
            | "fetch_url_content"
            | "follow_links"
            | "wiki_search"
            | "wiki_summary"
            | "semantic_search"
            | "get_document"
            | "list_documents"
            | "list_attachments"
    ) || is_explorer_tool(tool)
}

/// The page a call reads, when the call is a page read.
///
/// Only `fetch_url_content` asks for a page's text, so only it is answered
/// from the turn's page memory, kept apart from a second read of the same page
/// and recorded as a failure when it fails. `follow_links` also names a page,
/// one the model has usually read already, but it asks for the page's links:
/// memory of the text cannot answer it, and its failure says nothing about
/// whether the text can be read.
fn page_read_target<'v>(tool: &str, arguments: &'v serde_json::Value) -> Option<&'v str> {
    if tool == "fetch_url_content" {
        fetch_memory::fetch_target(arguments)
    } else {
        None
    }
}

/// What every call of the turn reads and none changes.
pub(super) struct CallScope<'a> {
    pub(super) container: &'a dyn ChatRuntime,
    pub(super) conv_service: &'a Arc<dyn ConversationServiceTrait>,
    pub(super) conv_id: &'a str,
    pub(super) request_id: &'a str,
    pub(super) sink: &'a ChatEventSink,
    pub(super) tools: Option<&'a [ToolDefinition]>,
    pub(super) highlight_terms: &'a [String],
    pub(super) tool_output_settings: &'a ToolOutputSettingsDto,
    pub(super) focus: &'a FocusScope,
    /// The folder of an Explorer conversation; its tools are only offered then.
    pub(super) explorer: Option<&'a ExplorerTurn>,
    pub(super) recorder: &'a TurnRecorder,
    /// Reading this conversation's own transcript is not a vault read, so it
    /// does not go through the space/focus scope path.
    pub(super) history: Arc<dyn ChatRecords>,
    pub(super) cancel: &'a CancellationToken,
    pub(super) clock: &'a TurnClock,
    pub(super) window_chars: usize,
    /// The schemas ride along with every round and take room like any message.
    pub(super) tool_schema_chars: usize,
    /// The turn's model: `find_image` shows it the candidates when it can see.
    pub(super) llm: &'a Arc<dyn lattice_core::application::ports::LLMPort>,
    /// The reader's question, the context a `find_image` pick is judged in.
    pub(super) question: &'a str,
}

/// The turn's tool calls, and what they have learned so far.
pub(super) struct ToolCalls<'a> {
    scope: CallScope<'a>,
    /// Pages are remembered for the whole turn, starting from what retrieval
    /// already opened, so a round is not spent asking for pages the prompt
    /// already carries.
    pub(super) fetch_memory: FetchMemory,
    pub(super) queries: Vec<String>,
    pub(super) fetched: Vec<String>,
    /// One memo for the whole turn: a repeated read of a range already
    /// exhausted answers with a reference instead of the same text twice.
    /// Locked only so a round's calls can share `self`; history calls never
    /// run together.
    history_memo: tokio::sync::Mutex<HistoryToolMemo>,
    document_evidence: DocumentEvidence,
    document_progress: DocumentProgress,
    /// The result each page read in the loop went out in, by page.
    pages_in_results: Vec<(String, String)>,
    /// Pages whose results were condensed into notes.
    condensed_pages: HashSet<String>,
}

/// A call, decided before anything runs.
enum Planned<'a> {
    /// Answered without running: malformed, unavailable, a page that already
    /// failed, or one already read.
    Answered {
        id: String,
        output: String,
    },
    Run(Pending<'a>),
}

struct Pending<'a> {
    id: String,
    /// As the model wrote it, for the logs.
    name: String,
    tool: String,
    arguments: serde_json::Value,
    /// The characters its result may take.
    allowance: usize,
    step: StepGuard<'a>,
}

impl Planned<'_> {
    /// Window room this call may still fill once it is answered.
    fn reserved_chars(&self) -> usize {
        match self {
            Self::Answered { output, .. } => output.chars().count(),
            Self::Run(pending) => pending.allowance,
        }
    }
}

/// How a tool call raced against Stop and the turn's deadline ended.
pub(super) enum ToolRun<T> {
    Finished(T),
    Cancelled,
    TimedOut,
}

/// Run one tool call, giving up when the turn is stopped or out of time.
pub(super) async fn run_tool_bounded<T>(
    tool: impl std::future::Future<Output = T>,
    remaining: Duration,
    cancelled: impl std::future::Future<Output = ()>,
) -> ToolRun<T> {
    tokio::select! {
        finished = timeout(remaining, tool) => match finished {
            Ok(result) => ToolRun::Finished(result),
            Err(_) => ToolRun::TimedOut,
        },
        () = cancelled => ToolRun::Cancelled,
    }
}

/// Where the batch of calls that starts at `start` ends.
///
/// A call that may not run alongside others is a batch of its own, so it
/// starts after everything before it has finished and before anything after
/// it begins. A second read of the same page waits for the first, so it is
/// answered from memory exactly as it would have been one call at a time.
fn batch_end(calls: &[RequestedCall], start: usize) -> usize {
    match calls.get(start) {
        Some(first) if runs_alongside_others(canonical_tool_name(&first.name)) => {}
        _ => return start + 1,
    }
    let mut pages = HashSet::new();
    let mut end = start;
    while let Some(call) = calls.get(end) {
        let tool = canonical_tool_name(&call.name);
        if !runs_alongside_others(tool) {
            break;
        }
        if let Some(url) = page_read_target(tool, &call.arguments) {
            if !pages.insert(identity(url)) {
                break;
            }
        }
        end += 1;
    }
    end
}

impl<'a> ToolCalls<'a> {
    pub(super) fn new(
        scope: CallScope<'a>,
        pages_already_read: FetchMemory,
        queries: Vec<String>,
        fetched: Vec<String>,
        sources: &[SourceDto],
    ) -> Self {
        Self {
            scope,
            fetch_memory: pages_already_read,
            queries,
            fetched,
            history_memo: tokio::sync::Mutex::new(HistoryToolMemo::default()),
            document_evidence: DocumentEvidence::default(),
            document_progress: DocumentProgress::new(sources),
            pages_in_results: Vec::new(),
            condensed_pages: HashSet::new(),
        }
    }

    /// Run one round's calls and append their results to `input`, in the
    /// order the model asked for them.
    pub(super) async fn run_round(
        &mut self,
        calls: &[RequestedCall],
        input: &mut Vec<CompletionInput>,
        timings: &mut ToolLoopTimingMetrics,
        sources: &mut Vec<SourceDto>,
        retrieval_trace: &mut Option<RetrievalTraceDto>,
    ) -> Result<()> {
        // Wall time, not the sum of each call's: calls overlap, and a resumed
        // turn's deadline is worked out from this.
        let started = Instant::now();
        let mut start = 0;
        while start < calls.len() {
            let end = batch_end(calls, start);
            let planned = self.prepare(calls, start..end, input, timings)?;
            let outcomes = self.execute(&planned).await?;
            for (plan, outcome) in planned.into_iter().zip(outcomes) {
                self.settle(plan, outcome, input, timings, sources, retrieval_trace)
                    .await?;
            }
            start = end;
        }
        timings.tool_execution_ms = timings
            .tool_execution_ms
            .saturating_add(elapsed_ms(started));
        Ok(())
    }

    /// The window as this turn's latest count of its prompt puts it.
    pub(super) fn set_window_chars(&mut self, window_chars: usize) {
        self.scope.window_chars = window_chars;
    }

    /// The results with these ids were condensed into notes, so the pages
    /// they carried are no longer in the context: a repeat request for one is
    /// read again rather than told the text is above.
    pub(super) fn results_condensed(&mut self, result_ids: &HashSet<String>) {
        let condensed = &mut self.condensed_pages;
        self.pages_in_results.retain(|(page, result_id)| {
            let gone = result_ids.contains(result_id);
            if gone {
                condensed.insert(page.clone());
            }
            !gone
        });
    }

    fn prepare(
        &mut self,
        calls: &[RequestedCall],
        batch: Range<usize>,
        input: &[CompletionInput],
        timings: &mut ToolLoopTimingMetrics,
    ) -> Result<Vec<Planned<'a>>> {
        let mut planned = Vec::with_capacity(batch.len());
        // What this batch's earlier calls may still add, counted at their
        // full allowance since none has come back yet.
        let mut reserved = 0;
        for (index, call) in calls.iter().enumerate().take(batch.end).skip(batch.start) {
            if self.scope.cancel.is_cancelled() {
                emit_cancelled_stream(self.scope.sink, self.scope.conv_id, self.scope.request_id);
                return Err(cancelled_error());
            }
            let plan = self.prepare_call(call, input, reserved, calls.len() - index, timings);
            reserved += plan.reserved_chars();
            planned.push(plan);
        }
        Ok(planned)
    }

    fn prepare_call(
        &mut self,
        call: &RequestedCall,
        input: &[CompletionInput],
        reserved: usize,
        calls_left: usize,
        timings: &mut ToolLoopTimingMetrics,
    ) -> Planned<'a> {
        timings.tool_call_count = timings.tool_call_count.saturating_add(1);
        // What is left of the window, shared between the calls this round
        // still has to answer. A fixed cap let one round of page fetches
        // outgrow a small model's entire context, and cut every result on a
        // large one; the window alone decides now.
        let allowance = tool_result_allowance(
            self.scope.window_chars,
            chars_in_flight(input) + self.scope.tool_schema_chars + reserved,
            calls_left,
        );
        let answered = |output: String| Planned::Answered {
            id: call.id.clone(),
            output,
        };
        // A call the model wrote but that cannot be run is answered with why,
        // and the model tries again.
        if let Some(problem) =
            lattice_core::application::ports::llm_port::invalid_tool_call_problem(&call.arguments)
        {
            warn!(
                requested_function = call.name.as_str(),
                %problem,
                "Model wrote a tool call that cannot be run"
            );
            timings.tool_failure_count = timings.tool_failure_count.saturating_add(1);
            self.scope.recorder.note(
                TurnStepKind::Retry,
                "The model's tool call was malformed",
                None,
                Some(call.name.clone()),
            );
            return answered(problem);
        }
        let tool = canonical_tool_name(call.name.as_str());
        if !is_tool_allowed(tool, self.scope.tools) {
            warn!(
                requested_function = call.name.as_str(),
                resolved_function = tool,
                available_tools = available_tool_names(self.scope.tools).join(", "),
                "Skipping unavailable tool requested by LLM"
            );
            return answered("Requested tool is unavailable".into());
        }
        let mut reread_from = None;
        if let Some(url) = page_read_target(tool, &call.arguments) {
            // A URL this turn already failed on costs a request to learn
            // nothing. Answer it from memory so the round can still spend its
            // time somewhere useful.
            if let Some(reason) = self.fetch_memory.previous_failure(url) {
                warn!(
                    requested_function = call.name.as_str(),
                    resolved_function = tool,
                    url,
                    reason,
                    "Skipping a URL that already failed this turn"
                );
                let notice = format!(
                    "Not retried: {url} already failed this turn ({reason}). Use a different source.",
                );
                timings.tool_failure_count = timings.tool_failure_count.saturating_add(1);
                return answered(notice);
            }
            // A page whose result was condensed into notes is not "above"
            // any more: it is read again, from the conversation's archive.
            let condensed = self.condensed_pages.remove(&identity(url));
            // A page this turn already read is answered from memory: no
            // request, no politeness delay, and nothing re-sent that the
            // model has. A page whose text memory no longer holds is read
            // again from the archive, from where the model's copy stops.
            let recall = (!condensed)
                .then(|| {
                    self.fetch_memory.recall(
                        url,
                        fetch_memory::fetch_offset(&call.arguments),
                        fetched_page_text_room(allowance),
                    )
                })
                .flatten();
            match recall {
                Some(Recall::Reread { offset }) => reread_from = Some(offset),
                Some(recall) => {
                    info!(
                        requested_function = call.name.as_str(),
                        resolved_function = tool,
                        url,
                        already_whole = matches!(recall, Recall::AlreadyWhole { .. }),
                        "Answered a repeat page request from this turn's memory"
                    );
                    timings.tool_success_count = timings.tool_success_count.saturating_add(1);
                    return answered(recalled_page_notice(url, &recall));
                }
                None => {}
            }
        }
        // A picture's subject is not something the answer searched for
        // evidence on, so it stays out of the turn's queries.
        if let Some(query) = call
            .arguments
            .get("query")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|query| !query.is_empty() && tool != FIND_IMAGE)
        {
            self.queries.push(query.to_string());
        }
        let step = self.scope.recorder.begin_guarded_with_links(
            tool_step_kind(tool),
            tool_activity_label(tool, &call.arguments),
            web_steps::tool_detail(tool, &call.arguments)
                .or_else(|| Some(tool_argument_summary(&call.arguments))),
            web_steps::links_for_call(tool, &call.arguments),
        );
        info!(
            requested_function = call.name.as_str(),
            resolved_function = tool,
            "Executing tool call from LLM"
        );
        let mut arguments = if tool == "get_document" {
            document_part_sized_to(&call.arguments, allowance)
        } else {
            call.arguments.clone()
        };
        if let (Some(offset), Some(object)) = (reread_from, arguments.as_object_mut()) {
            object.insert("offset".into(), serde_json::json!(offset));
        }
        Planned::Run(Pending {
            id: call.id.clone(),
            name: call.name.clone(),
            tool: tool.to_string(),
            arguments,
            allowance,
            step,
        })
    }

    /// Run a batch's calls, at most [`MAX_CONCURRENT_CALLS`] at once, with
    /// the outcomes in call order.
    async fn execute(
        &self,
        planned: &[Planned<'a>],
    ) -> Result<Vec<Option<ToolRun<Result<FunctionResult>>>>> {
        if !planned.iter().any(|plan| matches!(plan, Planned::Run(_))) {
            return Ok(planned.iter().map(|_| None).collect());
        }
        // A fetch can sit in a site's queue, then its HTTP timeout, then the
        // browser fallback. Stop and the turn's deadline must not wait for
        // all three.
        let remaining = self.scope.clock.remaining()?;
        // Boxed one by one rather than mapped by a closure: a closure over
        // the borrowed plans makes the future's `Send` depend on a
        // higher-ranked lifetime the compiler cannot prove.
        let mut runs = Vec::with_capacity(planned.len());
        for plan in planned {
            runs.push(self.run_planned(plan, remaining).boxed());
        }
        // `buffered`, not `buffer_unordered`: outcomes come back in the order
        // the calls were made.
        Ok(futures::stream::iter(runs)
            .buffered(MAX_CONCURRENT_CALLS)
            .collect()
            .await)
    }

    async fn run_planned(
        &self,
        plan: &Planned<'_>,
        remaining: Duration,
    ) -> Option<ToolRun<Result<FunctionResult>>> {
        let Planned::Run(pending) = plan else {
            return None;
        };
        Some(
            run_tool_bounded(
                self.execute_one(pending),
                remaining,
                self.scope.cancel.cancelled(),
            )
            .await,
        )
    }

    async fn execute_one(&self, pending: &Pending<'_>) -> Result<FunctionResult> {
        let scope = &self.scope;
        let tool = pending.tool.as_str();
        let call = FunctionCall::new(
            uuid::Uuid::new_v4().to_string(),
            tool,
            pending.arguments.clone(),
        );
        let explorer_turn = scope.explorer.filter(|_| is_explorer_tool(tool));
        if let Some(turn) = explorer_turn {
            // Cut to the same allowance as every other result, at a line
            // boundary, with a note saying where to read on.
            return turn.execute_tool(call, pending.allowance).await;
        }
        if history_tools::is_history_tool(tool) {
            let budget = history_tools::HistoryToolBudget {
                // A char allowance used as a byte ceiling is conservative in
                // the safe direction.
                max_response_bytes: pending.allowance,
                // The turn's history reads together come to at most one
                // window, however many rounds ask.
                turn_response_bytes: if scope.window_chars == 0 {
                    history_tools::HistoryToolBudget::default().turn_response_bytes
                } else {
                    scope.window_chars
                },
                deadline: Some(scope.clock.deadline()),
            };
            // The conversation id comes from the turn, and the scope has no
            // setter, so no tool argument can point it at another thread.
            let history_scope = history_tools::HistoryToolScope::new(scope.conv_id, budget);
            let mut memo = self.history_memo.lock().await;
            return history_tools::execute(scope.history.as_ref(), &history_scope, &mut memo, call)
                .await;
        }
        if tool == FIND_IMAGE {
            return images::execute(scope, call).await;
        }
        if tool == "fetch_url_content" {
            // A page this conversation already read is served from its
            // permanent archive — the citation's snapshot, not another
            // network request.
            if let Some(url) = fetch_memory::fetch_target(&pending.arguments) {
                if let Some(page) =
                    source_snapshots::archived_page(scope.container, scope.conv_id, url).await
                {
                    return Ok(FunctionResult::success(
                        serde_json::to_value(page).unwrap_or_default(),
                    ));
                }
            }
        }
        scoped_document_tools::execute(scope.container, scope.conv_id, scope.focus, call).await
    }

    /// Record one call's outcome and append its result.
    async fn settle(
        &mut self,
        plan: Planned<'a>,
        outcome: Option<ToolRun<Result<FunctionResult>>>,
        input: &mut Vec<CompletionInput>,
        timings: &mut ToolLoopTimingMetrics,
        sources: &mut Vec<SourceDto>,
        retrieval_trace: &mut Option<RetrievalTraceDto>,
    ) -> Result<()> {
        let pending = match plan {
            Planned::Answered { id, output } => {
                input.push(CompletionInput::ToolResult { id, output });
                return Ok(());
            }
            Planned::Run(pending) => pending,
        };
        let executed = match outcome {
            Some(ToolRun::Finished(result)) => result,
            Some(ToolRun::Cancelled) => {
                pending.step.failed(Some("Stopped".into()));
                emit_cancelled_stream(self.scope.sink, self.scope.conv_id, self.scope.request_id);
                return Err(cancelled_error());
            }
            Some(ToolRun::TimedOut) | None => {
                pending.step.failed(Some("Out of time".into()));
                return Err(self.scope.clock.exhausted());
            }
        };
        let Pending {
            id,
            name,
            tool,
            arguments,
            allowance,
            step,
        } = pending;
        let tool = tool.as_str();
        let result = match executed {
            Ok(result) => result,
            Err(error) => {
                step.failed(Some(error.to_string()));
                timings.tool_failure_count = timings.tool_failure_count.saturating_add(1);
                warn!(
                    requested_function = name.as_str(),
                    resolved_function = tool,
                    error = %error,
                    "Tool call failed"
                );
                input.push(CompletionInput::ToolResult {
                    id,
                    output: format!("Tool failed: {error}"),
                });
                if let Some(url) = page_read_target(tool, &arguments) {
                    self.fetch_memory.record_failure(url, &error.to_string());
                }
                return Ok(());
            }
        };
        if result.success {
            web_steps::finish_tool_step(
                step,
                self.scope.recorder,
                tool,
                &arguments,
                result.data.as_ref(),
            );
            timings.tool_success_count = timings.tool_success_count.saturating_add(1);
        } else {
            step.failed(result.error_message.clone());
            timings.tool_failure_count = timings.tool_failure_count.saturating_add(1);
        }
        let settings = self.scope.tool_output_settings;
        let tool_sources =
            collect_tool_sources(tool, &result, self.scope.highlight_terms, settings);
        let mut tool_chunk_ids = HashSet::new();
        if !tool_sources.is_empty() {
            let offered = tool_sources.len();
            let before = sources.len();
            tool_chunk_ids = merge_tool_sources(sources, tool_sources);
            *sources = deduplicate_sources(std::mem::take(sources));
            assign_citation_ids(sources);
            info!(
                requested_function = name.as_str(),
                resolved_function = tool,
                offered_sources = offered,
                added_sources = sources.len().saturating_sub(before),
                merged_sources = sources.len(),
                "Tool call added verifiable sources"
            );
        }
        if self.document_progress.record(tool, &result) {
            let trace = retrieval_trace.get_or_insert_with(|| RetrievalTraceDto {
                scope: "vault".to_string(),
                ..Default::default()
            });
            self.document_progress.update_trace(trace);
            if let Err(error) = (self.scope.sink)(ChatStreamEventDto {
                status: Some("retrieval".to_owned()),
                retrieval: Some(trace.clone()),
                ..ChatStreamEventDto::new(self.scope.conv_id, self.scope.request_id)
            }) {
                warn!(%error, "Failed to update retrieval trace after tool search");
            }
        }
        let page_offset = fetch_memory::fetch_offset(&arguments).unwrap_or(0);
        // The citation numbers this result's sources take come out of the
        // same allowance as the result, so the two together fit it.
        // A picture's source page is numbered for its caption, which the app
        // cites; the model is told not to cite the picture line, so the
        // number is not offered to it.
        let citable: Vec<(u32, &SourceDto)> = sources
            .iter()
            .filter(|_| tool != FIND_IMAGE)
            .filter(|source| tool_chunk_ids.contains(&source.chunk_id))
            .filter_map(|source| source.citation_id.map(|number| (number, source)))
            .collect();
        let result_text = render_with_citations(&citable, allowance, |room| {
            format_tool_result(tool, &result, settings, room, page_offset)
        });
        let result_text = self
            .document_evidence
            .render(tool, &result, result_text, |excerpt| {
                input.iter().any(|item| {
                    matches!(item, CompletionInput::ToolResult { output, .. } if output == excerpt)
                })
            });
        input.push(CompletionInput::ToolResult {
            id: id.clone(),
            output: result_text,
        });
        if let Err(error) = record_tool_document_references(
            self.scope.conv_service,
            self.scope.conv_id,
            tool,
            &result,
        )
        .await
        {
            warn!(
                error = %error,
                requested_function = name.as_str(),
                resolved_function = tool,
                "Failed to persist tool document references"
            );
        }
        let url = page_read_target(tool, &arguments);
        if let Some(url) = url.filter(|_| result.success) {
            let page = result
                .data
                .clone()
                .and_then(|data| serde_json::from_value::<FetchUrlContentOutput>(data).ok());
            if let Some(page) = page {
                let text = page.content.trim();
                // The part this result showed; a later request without an
                // offset carries on from the first part not yet shown.
                let total = text.chars().count();
                let delivery = Delivery::Part {
                    start: page_offset.min(total),
                    end: page_offset
                        .saturating_add(fetched_page_text_room(allowance))
                        .min(total),
                };
                self.fetch_memory.record_page(url, text, delivery);
                self.fetched.push(url.to_string());
                self.pages_in_results.push((identity(url), id));
                // A live read becomes the conversation's permanent archive; an
                // archived one already is.
                source_snapshots::archive_page_for_url(
                    self.scope.container,
                    self.scope.conv_id,
                    url,
                    &page,
                )
                .await;
            }
        }
        if !result.success {
            if let Some(url) = url {
                self.fetch_memory.record_failure(
                    url,
                    result
                        .error_message
                        .as_deref()
                        .unwrap_or("the fetch did not succeed"),
                );
            }
        }
        info!(
            requested_function = name.as_str(),
            resolved_function = tool,
            success = result.success,
            "Tool call executed"
        );
        Ok(())
    }
}

/// A `get_document` call whose part is sized to this call's allowance, so the
/// part the tool returns is shown whole. Parts are addressed by character
/// offset and each result names where the next starts, so parts of different
/// sizes neither overlap nor skip text. A model asking for less gets less.
fn document_part_sized_to(arguments: &serde_json::Value, allowance: usize) -> serde_json::Value {
    let mut arguments = arguments.clone();
    let room = document_page_room(allowance).max(1);
    let asked = arguments
        .get("max_content_length")
        .and_then(serde_json::Value::as_u64)
        .and_then(|asked| usize::try_from(asked).ok())
        .filter(|asked| *asked > 0)
        .unwrap_or(usize::MAX);
    if let Some(object) = arguments.as_object_mut() {
        object.insert(
            "max_content_length".into(),
            serde_json::json!(asked.min(room)),
        );
    }
    arguments
}

#[cfg(test)]
mod tests;
