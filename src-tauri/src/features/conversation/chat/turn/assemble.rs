//! Stage 5: fit what the turn found to its budget, number it, render the
//! prompt, and plan the typed request the model is sent.
use super::carry::CarriedMaterial;
use super::prepare::PreparedTurn;
use super::retrieve::Retrieved;
use super::*;
use crate::application::ports::llm_port::CompletionInput;
use crate::application::ports::ToolDefinition;
use crate::features::conversation::chat::fetch_memory::FetchMemory;

/// The rendered prompt and the numbered sources it cites.
pub(super) struct RenderedPrompt {
    pub(super) enhanced_message: String,
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
    let budgeted_results = evidence.select(&retrieval.search_response.results, |result| {
        llm.count_tokens(&result.content)
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
    let attachment_context = attachments.render(&retrieval.sources);
    let followup_context_text =
        followup_document.and_then(|document| document.render(&retrieval.sources));
    let web_context = retrieval::render_web_context(&retrieval.web_context, &retrieval.sources);
    let prior_evidence_context =
        prior_evidence::render(retrieval.sources.get(prior_start..).unwrap_or_default());

    let retrieval_trace = emit_retrieval_trace(turn, &retrieval, emit);

    let kb_context = build_kb_context(
        &budgeted_results,
        &citation_ids_by_chunk(&retrieval.sources),
    );
    let linked_web_sources_context = turn.linked_web_sources_context.clone();
    let can_open_pages = web_context.is_some() || linked_web_sources_context.is_some();
    let has_grounded_context = attachment_context.is_some()
        || followup_context_text.is_some()
        || kb_context.is_some()
        || can_open_pages
        || prior_evidence_context.is_some();

    let searches_again =
        deep_research_searches_again(flags, tool_preferences, llm.supports_tool_calling());
    let enhanced_message = PromptMessageBuilder::new(
        &turn.prompt_settings,
        &turn.message,
        retrieval.interpretation.query_type == QueryType::Greeting,
        flags,
    )
    .with_explorer_context(explorer_context)
    .with_deep_research_searches(searches_again.then(|| retrieval.web_queries.clone()))
    .with_prior_evidence_context(prior_evidence_context)
    .with_attachment_context(attachment_context)
    .with_followup_context(followup_context_text)
    .with_kb_context(kb_context)
    .with_linked_web_sources_context(linked_web_sources_context)
    .with_web_context(web_context)
    .with_web_search_error(retrieval.web_search_error.clone())
    .with_kb_unavailable_reason(retrieval.kb_unavailable_reason.clone())
    .with_kb_attempted(retrieval.kb_attempted)
    .with_kb_sufficiency(retrieval.sufficiency.as_ref())
    .build();
    metrics.prompt_build_ms = elapsed_ms(prompt_build_start);

    RenderedPrompt {
        enhanced_message,
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
}

/// Choose the turn's tools and plan its input.
///
/// With bounded memory on, the context assembler plans the whole request —
/// policy, tool schemas, memory, history and the prompt — against the
/// window; a plan that would drop unprocessed history compacts first, once.
/// With it off, the string history goes out as it is, already charged whole
/// by the turn's budget.
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
    prompt: &RenderedPrompt,
    metrics: &mut ConversationFlowTimingMetrics,
) -> Result<PlannedRequest> {
    let tool_prep_start = Instant::now();
    let tools = select_tools(container, turn, flags, tool_preferences, prompt);
    metrics.tool_prep_ms = elapsed_ms(tool_prep_start);

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
    let memory_turn = memory_context::prepare_memory_turn(
        || async {
            let memory_turn = memory_context::build_memory_plan_for_query(
                records.as_ref(),
                records.as_ref(),
                &turn.llm,
                &turn.conv_id,
                &turn.system_prompt,
                &prompt.enhanced_message,
                tool_schema_tokens,
                Vec::new(),
                turn.settings.llm.bounded_conversation_memory,
                &turn.message,
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
                    evicted = ?memory_turn.plan.accounting.evicted,
                    elapsed_ms = elapsed_ms(memory_plan_start),
                    "Assembled bounded conversation memory for this turn"
                );
            }
            Ok(memory_turn)
        },
        || container.compact_for_turn(turn.conv_id.as_str(), turn.cancel_token()),
    )
    .await?;

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
        },
        None => PlannedRequest {
            input: crate::application::services::completion_input::from_context(
                &turn.prompt_settings.system_prompt,
                &turn.context,
                &prompt.enhanced_message,
            ),
            tools,
            max_output_tokens: turn.budget.output_tokens,
            input_budget: turn.budget.input_budget,
            memory_usage: None,
        },
    })
}

fn select_tools(
    container: &dyn ChatRuntime,
    turn: &PreparedTurn,
    flags: SearchFlags,
    tool_preferences: Option<&ToolPreferences>,
    prompt: &RenderedPrompt,
) -> Vec<ToolDefinition> {
    let llm = &turn.llm;
    let supports_tools = llm.supports_tool_calling();
    let tool_definitions = build_llm_tool_definitions(
        container,
        llm,
        tool_preferences,
        &turn.settings.llm.custom_tools,
        prompt.can_open_pages,
    );
    let force_tools_for_turn = tool_preferences
        .and_then(|preferences| preferences.turn_mode.as_deref())
        .map(str::trim)
        .is_some_and(|mode| mode.eq_ignore_ascii_case("query"));
    let grounded_tools: Vec<_> = tool_definitions
        .iter()
        .filter(|tool| {
            kept_with_grounded_context(&tool.name, prompt.can_open_pages, flags.deep_research_mode)
        })
        .cloned()
        .collect();
    let selected_tools = if prompt.has_grounded_context && !force_tools_for_turn {
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
    if let Some(explorer) = turn
        .explorer
        .as_ref()
        .filter(|_| supports_tools && !flags.closed_book)
    {
        crate::features::explorer::tools::add_to_turn(&mut turn_tools, explorer.has_folder_index());
    }

    info!(
        provider = llm.provider_name(),
        supports_tools = supports_tools,
        tool_count = turn_tools.len(),
        history_tools_offered = turn_tools
            .iter()
            .filter(|tool| history_tools::is_history_tool(&tool.name))
            .count(),
        tools_enabled_for_turn = !turn_tools.is_empty(),
        force_tools_for_turn = force_tools_for_turn,
        has_grounded_context = prompt.has_grounded_context,
        "LLM capability check for conversation"
    );
    turn_tools
}
