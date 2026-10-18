//! Stage 5: fit what the turn found to its budget, number it, render the
//! prompt, and plan the typed request the model is sent.
use super::carry::CarriedMaterial;
use super::message::{MessageFrame, TurnMessage};
use super::prepare::PreparedTurn;
use super::retrieve::Retrieved;
use super::*;
use crate::features::conversation::chat::fetch_memory::FetchMemory;
use crate::features::conversation::chat::prompting::{kb_passage, kb_passages};
use lattice_core::application::ports::llm_port::CompletionInput;
use lattice_core::application::ports::{LLMPort, ToolDefinition};
use lattice_core::application::services::context_assembler::{render, InlineEvidence};

/// The rendered prompt and the numbered sources it cites.
pub(super) struct RenderedPrompt {
    /// The message with all of its evidence.
    pub(super) enhanced_message: String,
    /// The same message in parts, so the request can carry less of its
    /// evidence than this when the window cannot hold it all. `None` for a
    /// resumed turn, whose request was saved whole.
    pub(super) message: Option<TurnMessage>,
    pub(super) sources: Vec<SourceDto>,
    pub(super) retrieval_trace: Option<RetrievalTraceDto>,
    /// A canned answer the router chose; no model call is made for it.
    pub(super) short_circuit_response: Option<String>,
    /// Every page retrieval opened, so the tool loop never fetches one again.
    pub(super) pages_read: FetchMemory,
    /// Web context is in the prompt, so the model may open pages.
    pub(super) can_open_pages: bool,
    pub(super) has_grounded_context: bool,
    pub(super) attachment_names: Vec<String>,
    pub(super) attachment_ids: Vec<String>,
}

impl RenderedPrompt {
    /// A saved research turn's prompt: what it cites and what it has read.
    /// The request itself was saved whole, so nothing here is rendered again.
    pub(super) fn resumed(
        sources: Vec<SourceDto>,
        retrieval_trace: Option<RetrievalTraceDto>,
        short_circuit_response: Option<String>,
        pages_read: FetchMemory,
    ) -> Self {
        Self {
            enhanced_message: String::new(),
            message: None,
            sources,
            retrieval_trace,
            short_circuit_response,
            pages_read,
            can_open_pages: false,
            has_grounded_context: false,
            attachment_names: Vec::new(),
            attachment_ids: Vec::new(),
        }
    }

    /// Drop the sources of the evidence the request could not carry: a
    /// passage the model was not shown is not a source of its answer, and the
    /// trace should not count it as read.
    fn keep_evidence(&mut self, kept: usize) {
        let Some(message) = &self.message else {
            return;
        };
        let before = self.sources.len();
        message.keep_sources(kept, &mut self.sources);
        if self.sources.len() == before {
            return;
        }
        if let Some(trace) = self.retrieval_trace.as_mut() {
            let TraceCounts {
                passages,
                files,
                web_pages,
            } = trace_counts(&self.sources);
            trace.passages = passages;
            trace.files = files;
            trace.web_pages = web_pages;
        }
    }
}

pub(super) fn render_prompt(
    turn: &PreparedTurn,
    flags: SearchFlags,
    tool_preferences: Option<&ToolPreferences>,
    carried: CarriedMaterial,
    retrieved: Retrieved,
    emit: &ChatEventSink,
    metrics: &mut ConversationFlowTimingMetrics,
) -> RenderedPrompt {
    let prompt_build_start = Instant::now();
    let CarriedMaterial {
        attachments,
        explorer_context,
        attachment_names,
        attachment_ids,
    } = carried;
    let Retrieved {
        prior_sources,
        prior_left_out,
        mut retrieval,
    } = retrieved;
    let llm = &turn.llm;

    // An attachment is in the prompt whole, so the same document coming back
    // as search passages is a second copy of text the model already has and a
    // footnote pointing at the same file twice.
    let carried_attachment_ids = attachments.carried_document_ids();
    if !carried_attachment_ids.is_empty() {
        retrieval.search_response.results.retain(|result| {
            result
                .document_id
                .as_deref()
                .map(|id| !carried_attachment_ids.contains(id))
                .unwrap_or(true)
        });
    }
    // Fetched web pages are carried whole where the window allows, so they are
    // no longer small enough to ignore: what they fill is not there for the
    // user's own passages. Counted before the numbers exist; a label is a token
    // or two either way.
    let mut evidence = retrieval.evidence;
    evidence.charge(
        retrieval
            .web_context
            .iter()
            .map(|item| llm.count_tokens(&item.render(0)))
            .sum(),
    );
    let passage_room = evidence.remaining();
    // Priced as the prompt shows a passage, under its document's name and id,
    // rather than as its bare text: on a small window those lines add up.
    let budgeted_results = evidence.select(&retrieval.search_response.results, |result| {
        llm.count_tokens(&kb_passage(result, 0))
    });
    if budgeted_results.len() < retrieval.search_response.results.len() {
        info!(
            kept = budgeted_results.len(),
            total = retrieval.search_response.results.len(),
            tokens_used = passage_room - evidence.remaining(),
            budget = passage_room,
            "RAG context truncated by token budget"
        );
    }
    retrieval::drop_unbudgeted_sources(
        &mut retrieval.sources,
        &retrieval.search_response.results,
        &budgeted_results,
    );

    // Which sources each block of the prompt shows, kept so the sources of a
    // block the request has to leave out can be dropped with it.
    let mut followup_chunks = HashSet::new();
    let followup_document = match retrieval.followup_context.take() {
        // The document under discussion and the file just attached are the
        // same document: it is already carried whole, and reusing it as well
        // would put the same text in the prompt twice under two headings.
        Some((_, followup_sources))
            if !followup_sources.is_empty()
                && followup_sources
                    .iter()
                    .all(|source| carried_attachment_ids.contains(&source.document_id)) =>
        {
            None
        }
        Some((document, mut followup_sources)) => {
            info!(
                conversation_id = turn.conv_id.as_str(),
                "Follow-up detected: reusing conversation document context"
            );
            followup_chunks = followup_sources
                .iter()
                .map(|source| source.chunk_id.clone())
                .collect();
            // Ahead of any web results the turn also fetched, which stay: the
            // prompt shows them, so the list has to number them.
            followup_sources.append(&mut retrieval.sources);
            retrieval.sources = deduplicate_sources(followup_sources);
            Some(document)
        }
        None => None,
    };

    // Attachments lead the list: they are the reader's own material. Whatever
    // is left pointing at a carried document goes: the whole text is already
    // there.
    let attachment_chunks: HashSet<String> = attachments
        .sources()
        .into_iter()
        .map(|source| source.chunk_id)
        .collect();
    if !attachments.is_empty() {
        retrieval
            .sources
            .retain(|source| !carried_attachment_ids.contains(&source.document_id));
        let mut sources = attachments.sources();
        sources.append(&mut retrieval.sources);
        retrieval.sources = deduplicate_sources(sources);
    }

    let prior_start = retrieval.sources.len();
    let existing_prior_keys: HashSet<_> = retrieval
        .sources
        .iter()
        .map(prior_evidence::source_key)
        .collect();
    retrieval
        .sources
        .extend(prior_sources.into_iter().filter(|source| {
            !carried_attachment_ids.contains(&source.document_id)
                && !existing_prior_keys.contains(&prior_evidence::source_key(source))
        }));

    // Number the sources *after* every step that can reorder or replace the
    // list (including the follow-up swap above), then build the prompt from
    // those same numbers. Assigning earlier would let the follow-up path
    // renumber behind the prompt's back.
    assign_citation_ids(&mut retrieval.sources);
    let citations_of = |chunks: &HashSet<String>| -> Vec<u32> {
        retrieval
            .sources
            .iter()
            .filter(|source| chunks.contains(&source.chunk_id))
            .filter_map(|source| source.citation_id)
            .collect()
    };
    let attachment_context = attachments.render(&retrieval.sources);
    let attachment_citations = citations_of(&attachment_chunks);
    let followup_context_text =
        followup_document.and_then(|document| document.render(&retrieval.sources));
    let followup_citations = citations_of(&followup_chunks);
    let web_pages = retrieval::web_context_entries(&retrieval.web_context, &retrieval.sources);
    let prior = retrieval.sources.get(prior_start..).unwrap_or_default();
    let prior_evidence_context = prior_evidence::render(prior, prior_left_out);
    let prior_citations: Vec<u32> = prior
        .iter()
        .filter_map(|source| source.citation_id)
        .collect();

    let retrieval_trace = emit_retrieval_trace(turn, &retrieval, emit);

    let library = kb_passages(
        &budgeted_results,
        &citation_ids_by_chunk(&retrieval.sources),
    );
    let linked_web_sources_context = turn.linked_web_sources_context.clone();
    let can_open_pages = !web_pages.is_empty() || linked_web_sources_context.is_some();
    let has_grounded_context = attachment_context.is_some()
        || followup_context_text.is_some()
        || !budgeted_results.is_empty()
        || can_open_pages
        || prior_evidence_context.is_some();

    let searches_again =
        deep_research_searches_again(flags, tool_preferences, llm.supports_tool_calling());
    let message = TurnMessage::new(MessageFrame {
        prompt_settings: turn.prompt_settings.clone(),
        question: turn.message.clone(),
        is_greeting: retrieval.interpretation.query_type == QueryType::Greeting,
        flags,
        deep_research_searches: searches_again.then(|| retrieval.web_queries.clone()),
        web_search_error: retrieval.web_search_error.clone(),
        kb_unavailable_reason: retrieval.kb_unavailable_reason.clone(),
        kb_attempted: retrieval.kb_attempted,
        sufficiency: retrieval.sufficiency.clone(),
    })
    .with_attachments(attachment_context, attachment_citations)
    .with_folder(explorer_context)
    .with_prior_evidence(prior_evidence_context, prior_citations)
    .with_followup(followup_context_text, followup_citations)
    .with_library(library)
    .with_linked_pages(linked_web_sources_context)
    .with_web_pages(web_pages);
    let enhanced_message = message.render(message.items());
    metrics.prompt_build_ms = elapsed_ms(prompt_build_start);

    RenderedPrompt {
        enhanced_message,
        message: Some(message),
        sources: retrieval.sources,
        retrieval_trace,
        short_circuit_response: retrieval.short_circuit_response.take(),
        pages_read: std::mem::take(&mut retrieval.pages_read),
        can_open_pages,
        has_grounded_context,
        attachment_names,
        attachment_ids,
    }
}

/// Emitted before generation so the UI can show its reading before its
/// writing, and persisted into the assistant message metadata so it survives
/// a reload. Nothing is emitted when KB retrieval never ran — an absent trace
/// is not a trace of zeros.
fn emit_retrieval_trace(
    turn: &PreparedTurn,
    retrieval: &retrieval::RetrievalPipelineOutcome,
    emit: &ChatEventSink,
) -> Option<RetrievalTraceDto> {
    if retrieval.searched_documents == 0
        && retrieval.sources.is_empty()
        && retrieval.kb_unavailable_reason.is_none()
    {
        return None;
    }
    let TraceCounts {
        passages,
        files,
        web_pages,
    } = trace_counts(&retrieval.sources);
    let trace = RetrievalTraceDto {
        searched_documents: retrieval.searched_documents,
        passages,
        files,
        web_pages,
        scope: if retrieval.scope_is_linked {
            "linked"
        } else {
            "vault"
        }
        .to_string(),
        unavailable_reason: retrieval.kb_unavailable_reason.clone(),
        // Only reported when the check actually ran. A turn that never
        // searched the vault has no verdict, and zeros would read as one.
        kb_sufficient: retrieval.sufficiency.as_ref().map(|v| v.sufficient),
        kb_corrective_retries: retrieval
            .sufficiency
            .as_ref()
            .map(|_| retrieval.sub_timings.kb_corrective_retries),
        kb_planner_skipped: retrieval
            .sufficiency
            .as_ref()
            .map(|_| retrieval.sub_timings.kb_planner_skipped),
        sufficiency_reasons: retrieval
            .sufficiency
            .as_ref()
            .map(|v| v.reasons.iter().map(|r| (*r).to_owned()).collect())
            .unwrap_or_default(),
        focused_documents: turn.focus.reported_count(),
    };
    if let Err(e) = emit(ChatStreamEventDto {
        status: Some("retrieval".to_owned()),
        retrieval: Some(trace.clone()),
        ..ChatStreamEventDto::new(&turn.conv_id, &turn.turn_id)
    }) {
        warn!(error = %e, "Failed to emit retrieval trace");
    }
    Some(trace)
}

/// The typed request: the tools offered, and the input the context
/// assembler planned.
pub(super) struct PlannedRequest {
    pub(super) input: Vec<CompletionInput>,
    pub(super) tools: Vec<ToolDefinition>,
    pub(super) max_output_tokens: usize,
    pub(super) input_budget: usize,
    /// The memory the answer was given, recorded on it.
    pub(super) memory_usage: Option<serde_json::Value>,
    /// The plan asking for the working summary to be refolded, because its
    /// window had to drop turns the summary does not cover.
    pub(super) summary_refold: Option<lattice_core::domain::conversation::memory::SummaryRefold>,
}

/// Choose the turn's tools and plan its input.
///
/// With bounded memory on, the context assembler plans the whole request —
/// policy, tool schemas, memory, history and the prompt — against the
/// window; a plan that would drop unprocessed history compacts first, once.
/// The prompt's evidence goes to it as evidence rather than as part of the
/// message, so a window that cannot hold all of it carries the best of it,
/// and the sources of what was left out are dropped from `prompt`. With it
/// off, the string history goes out as it is, already charged whole by the
/// turn's budget.
///
/// # Errors
///
/// The assembler's refusals: a prompt larger than the window, an unusable
/// model, active requirements that do not fit, or history compaction could
/// not cover. Each is reported rather than resolved by dropping something.
pub(super) async fn plan_request(
    container: &dyn ChatRuntime,
    turn: &PreparedTurn,
    flags: SearchFlags,
    tool_preferences: Option<&ToolPreferences>,
    prompt: &mut RenderedPrompt,
    metrics: &mut ConversationFlowTimingMetrics,
) -> Result<PlannedRequest> {
    let tool_prep_start = Instant::now();
    let tools = select_tools(container, turn, flags, tool_preferences, prompt);
    metrics.tool_prep_ms = elapsed_ms(tool_prep_start);
    let system_policy = with_picture_guidance(&turn.system_prompt, &tools);
    let plain_system_prompt = with_picture_guidance(&turn.prompt_settings.system_prompt, &tools);

    // Enabled memory must account for unprocessed history before generation,
    // including conversations that have no extracted ledger yet.
    let memory_plan_start = Instant::now();
    let records = container
        .chat_records()
        .with_recall_embedding(container.get_or_load_embedding().await.ok());
    // Counted as sent: the JSON `parameters` of every tool are prompt text the
    // model reads, and on a 4k-token local window they are a large share of it.
    let tool_schema_tokens = if tools.is_empty() {
        0
    } else {
        turn.llm.count_tokens(&tool_loop::tool_schema_text(&tools))
    };
    // Read once, so a compaction retry and every tool round carry the same day.
    let today = chrono::Local::now().date_naive();
    // The message without its evidence is what the plan charges as fixed;
    // the evidence is charged after the history and cut, lowest-ranked first,
    // if the request will not fit.
    let inline_message = prompt
        .message
        .as_ref()
        .filter(|message| message.items() > 0);
    let bare_message = inline_message.map(|message| message.render(0));
    let render_kept =
        |keep: usize| inline_message.map_or_else(String::new, |message| message.render(keep));
    let inline_evidence = inline_message.map(|message| InlineEvidence {
        items: message.items(),
        render: &render_kept,
    });
    let current_input = bare_message
        .as_deref()
        .unwrap_or(prompt.enhanced_message.as_str());
    let memory_turn = memory_context::prepare_memory_turn(
        || async {
            let memory_turn = memory_context::build_memory_plan_for_query(
                records.as_ref(),
                records.as_ref(),
                &turn.llm,
                &turn.conv_id,
                memory_context::TurnPrompt {
                    system_policy: &system_policy,
                    current_input,
                    inline_evidence,
                    tool_schema_tokens,
                    document_evidence: Vec::new(),
                },
                turn.settings.llm.bounded_conversation_memory,
                &turn.message,
                today,
            )
            .await?;

            if let Some(memory_turn) = &memory_turn {
                info!(
                    conversation_id = turn.conv_id.as_str(),
                    memory_revision = memory_turn.plan.memory_revision,
                    active_mandatory = memory_turn.plan.accounting.active_mandatory_count,
                    conflicts = memory_turn.plan.accounting.active_conflict_count,
                    total_input_tokens = memory_turn.plan.accounting.total_input,
                    input_budget = memory_turn.plan.accounting.input_budget,
                    accounting = ?memory_turn.plan.accounting.accounting_method,
                    recall_passages = memory_turn.plan.retrieval.passages_selected,
                    recall_error = ?memory_turn.plan.retrieval.index_error,
                    compaction_required = memory_turn.plan.compaction_required,
                    summary_refold = ?memory_turn.plan.summary_refold,
                    evicted = ?memory_turn.plan.accounting.evicted,
                    elapsed_ms = elapsed_ms(memory_plan_start),
                    "Assembled bounded conversation memory for this turn"
                );
            }
            Ok(memory_turn)
        },
        || {
            container.compact_for_turn(
                turn.conv_id.as_str(),
                Arc::clone(&turn.llm),
                turn.cancel_token(),
            )
        },
    )
    .await?;
    let carried = inline_message
        .zip(memory_turn.as_ref())
        .map(|(message, memory_turn)| {
            (
                memory_turn.plan.accounting.inline_evidence_kept,
                message.items(),
            )
        });
    if let Some((kept, items)) = carried.filter(|(kept, items)| kept < items) {
        info!(
            conversation_id = turn.conv_id.as_str(),
            kept, items, "Cut the turn's lowest-ranked evidence to fit the model's window"
        );
        prompt.keep_evidence(kept);
    }

    Ok(match memory_turn {
        Some(memory_turn) => PlannedRequest {
            memory_usage: Some(serde_json::json!({
                "items": memory_turn.plan.used_memory_ids,
                "revision": memory_turn.plan.memory_revision,
                "recallPassages": memory_turn.plan.retrieval.passages_selected,
            })),
            input: memory_turn.plan.messages,
            tools,
            max_output_tokens: memory_turn.plan.max_output_tokens,
            input_budget: memory_turn.plan.accounting.input_budget,
            summary_refold: memory_turn.plan.summary_refold,
        },
        None => PlannedRequest {
            // The date leads the final message here too, never the system
            // prompt: that would change the head of the request every day.
            input: lattice_core::application::services::completion_input::from_context(
                &plain_system_prompt,
                &turn.context,
                &format!(
                    "{}\n\n{}",
                    render::render_date_line(today),
                    prompt.enhanced_message
                ),
            ),
            tools,
            max_output_tokens: turn.budget.output_tokens,
            input_budget: turn.budget.input_budget,
            memory_usage: None,
            summary_refold: None,
        },
    })
}

/// What the tool schemas a turn sends cost once its prompt carries evidence:
/// the grounded subset with page opening, and history and folder access on
/// top. The turn's budget reserves this before it sizes the evidence, since
/// the evidence and the schemas share one window.
pub(super) fn grounded_tool_tokens(
    container: &dyn ChatRuntime,
    llm: &Arc<dyn LLMPort>,
    custom_tools: &[CustomToolSettingsDto],
    explorer: Option<&ExplorerTurn>,
    flags: SearchFlags,
    tool_preferences: Option<&ToolPreferences>,
) -> usize {
    let tools = offered_tools(
        container,
        llm,
        custom_tools,
        explorer,
        flags,
        tool_preferences,
        Grounding {
            can_open_pages: true,
            has_grounded_context: true,
        },
    );
    if tools.is_empty() {
        0
    } else {
        let guidance = if offers_pictures(&tools) {
            llm.count_tokens(PICTURE_GUIDANCE)
        } else {
            0
        };
        llm.count_tokens(&tool_loop::tool_schema_text(&tools)) + guidance
    }
}

/// How to place a picture, sent on every turn that offers `find_image` and on
/// no other: the same text each time, so it never moves the cached prefix of
/// a web turn.
const PICTURE_GUIDANCE: &str = "\nA picture helps when the reader would otherwise have to imagine a \
    concrete thing. Get one only with find_image and paste its `markdown` line as its own paragraph, \
    unchanged; never write an image address of your own, and do not add a citation to the picture line.";

fn offers_pictures(tools: &[ToolDefinition]) -> bool {
    tools.iter().any(|tool| tool.name == "find_image")
}

/// The turn's standing instructions, with the picture guidance when the turn
/// can fetch pictures. Charged with the tool schemas by `grounded_tool_tokens`.
pub(super) fn with_picture_guidance(system_prompt: &str, tools: &[ToolDefinition]) -> String {
    if offers_pictures(tools) {
        format!("{system_prompt}{PICTURE_GUIDANCE}")
    } else {
        system_prompt.to_string()
    }
}

/// What the prompt carries, as far as the choice of tools goes.
#[derive(Debug, Clone, Copy)]
struct Grounding {
    can_open_pages: bool,
    has_grounded_context: bool,
}

fn select_tools(
    container: &dyn ChatRuntime,
    turn: &PreparedTurn,
    flags: SearchFlags,
    tool_preferences: Option<&ToolPreferences>,
    prompt: &RenderedPrompt,
) -> Vec<ToolDefinition> {
    let grounding = Grounding {
        can_open_pages: prompt.can_open_pages,
        has_grounded_context: prompt.has_grounded_context,
    };
    let turn_tools = offered_tools(
        container,
        &turn.llm,
        &turn.settings.llm.custom_tools,
        turn.explorer.as_ref(),
        flags,
        tool_preferences,
        grounding,
    );
    let force_tools_for_turn = forces_tools(tool_preferences);
    info!(
        provider = turn.llm.provider_name(),
        supports_tools = turn.llm.supports_tool_calling(),
        tool_count = turn_tools.len(),
        history_tools_offered = turn_tools
            .iter()
            .filter(|tool| history_tools::is_history_tool(&tool.name))
            .count(),
        tools_enabled_for_turn = !turn_tools.is_empty(),
        force_tools_for_turn = force_tools_for_turn,
        has_grounded_context = grounding.has_grounded_context,
        "LLM capability check for conversation"
    );
    turn_tools
}

/// A turn in "query" mode is offered every tool, grounded or not.
fn forces_tools(tool_preferences: Option<&ToolPreferences>) -> bool {
    tool_preferences
        .and_then(|preferences| preferences.turn_mode.as_deref())
        .map(str::trim)
        .is_some_and(|mode| mode.eq_ignore_ascii_case("query"))
}

fn offered_tools(
    container: &dyn ChatRuntime,
    llm: &Arc<dyn LLMPort>,
    custom_tools: &[CustomToolSettingsDto],
    explorer: Option<&ExplorerTurn>,
    flags: SearchFlags,
    tool_preferences: Option<&ToolPreferences>,
    grounding: Grounding,
) -> Vec<ToolDefinition> {
    let supports_tools = llm.supports_tool_calling();
    let tool_definitions = build_llm_tool_definitions(
        container,
        llm,
        tool_preferences,
        custom_tools,
        grounding.can_open_pages,
    );
    let grounded_tools: Vec<_> = tool_definitions
        .iter()
        .filter(|tool| {
            kept_with_grounded_context(
                &tool.name,
                grounding.can_open_pages,
                flags.deep_research_mode,
            )
        })
        .cloned()
        .collect();
    let selected_tools = if grounding.has_grounded_context && !forces_tools(tool_preferences) {
        &grounded_tools
    } else {
        &tool_definitions
    };
    // A closed-book turn loses every external tool — offering it `semantic_search`
    // would reopen the vault one tool call later — but it keeps read-only access
    // to its own transcript. This conversation is internal task context, not an
    // external source, so a turn that may not search the web must still be able
    // to recover a requirement the user stated twenty turns ago.
    //
    // Gated on `supports_tools` because a model without tool calling must not be
    // handed a schema it cannot use: telling it to call a tool that was never
    // supplied is its own failure mode.
    let mut turn_tools =
        history_tools::tools_for_turn(selected_tools, flags.closed_book, supports_tools);
    // The folder tools ride on top of whichever branch was chosen above, the
    // way history access does: a turn grounded in library passages may still
    // need to open the file the question is about. A closed-book turn reads
    // nothing outside its message, the folder included.
    if let Some(explorer) = explorer.filter(|_| supports_tools && !flags.closed_book) {
        explorer.add_tools(&mut turn_tools);
    }
    turn_tools
}
