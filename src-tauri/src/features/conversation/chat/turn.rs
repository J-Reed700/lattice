use super::*;

/// Chat with conversation context
///
/// Maintains persistent conversation history with token-aware context window management.
/// Automatically creates new conversations or continues existing ones.
///
/// # Arguments
///
/// * `container` - DI container with services and security context
/// * `conversation_id` - Optional conversation ID (None = create new)
/// * `message` - User's message
///
/// # Returns
///
/// * `Ok(ChatResponse)` - Response with conversation ID and generated message
/// * `Err(AppError)` - If rate limited, validation fails, or generation fails
///
/// # Errors
///
/// * `AppError::NoActiveModel` - No chat model is active
/// * `AppError::RateLimitExceeded` - Too many requests (10/min)
/// * `AppError::InvalidInput` - Empty or invalid message
/// * `AppError::Database` - Database operation failed
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: 10 requests per minute
/// - **Input Validation**: Message length and content validation
/// - **Audit Logging (CWE-778)**: Logs chat interactions with context size
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface ChatResponse {
///   conversationId: string;
///   message: string;
///   contextUsed: number;
/// }
///
/// // Start new conversation
/// const response = await invoke<ChatResponse>('chat_with_conversation', {
///   conversationId: null,
///   message: 'What is semantic search?'
/// });
///
/// console.log(`Conversation ID: ${response.conversationId}`);
/// console.log(`Response: ${response.message}`);
/// console.log(`Context messages: ${response.contextUsed}`);
///
/// // Continue conversation
/// const followUp = await invoke<ChatResponse>('chat_with_conversation', {
///   conversationId: response.conversationId,
///   message: 'How does it differ from keyword search?'
/// });
///
/// console.log(`Follow-up: ${followUp.message}`);
/// console.log(`Context messages: ${followUp.contextUsed}`);
/// ```
pub(super) async fn run_turn(
    container: &dyn ChatRuntime,
    conversation_id: Option<String>,
    message: String,
    tool_preferences: Option<ToolPreferences>,
    cancel_only: Option<bool>,
    request_id: Option<String>,
    attachment_names: Option<Vec<String>>,
    attachment_document_ids: Option<Vec<String>>,
    emit: ChatEventSink,
) -> Result<ChatResponse> {
    if cancel_only.unwrap_or(false) {
        let conv_id = conversation_id.ok_or_else(|| {
            AppError::InvalidInput("Conversation ID is required to cancel generation".to_string())
        })?;
        let cancelled = cancel_generation_for_conversation(&conv_id, request_id.as_deref());
        return Ok(ChatResponse {
            conversation_id: conv_id,
            message: if cancelled {
                "cancelled".to_string()
            } else {
                "idle".to_string()
            },
            messages: Vec::new(),
            context_used: 0,
            sources: Vec::new(),
            timing_metrics: None,
        });
    }

    // DIAGNOSTIC: Log command start
    tracing::info!(
        conversation_id = conversation_id.as_deref().unwrap_or("NEW"),
        content_length = message.len(),
        "chat_with_conversation: START"
    );

    let turn_id = request_id
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    // Register the turn before any of the slow work below it — request
    // validation, a cold model load, opening the conversation. The composer
    // shows Stop from the moment it sends, so a turn that only becomes
    // cancellable once that work is done leaves every stop press in the
    // meantime with nothing to act on. When the caller named the conversation
    // (every store caller does) the turn registers right here, under the id
    // exactly as the caller wrote it — the same id `get_or_create_conversation_id`
    // hands back and the same one a cancel arrives with, which is what lets the
    // registry match the three up. Only a brand-new conversation has to wait
    // until its id exists.
    let mut turn_guard = match conversation_id.as_deref() {
        Some(id) if !id.trim().is_empty() => {
            Some(TurnCancellationGuard::start(turn_id.clone(), id)?)
        }
        _ => None,
    };
    let cancellation_error = || AppError::InvalidState("Generation cancelled by user.".to_string());

    let flow_start = Instant::now();
    let mut flow_metrics = ConversationFlowTimingMetrics::default();

    let validate_start = Instant::now();
    let validated_message = validate_and_guard_chat_request(container, &message).await?;
    flow_metrics.validate_request_ms = elapsed_ms(validate_start);
    if is_cancel_requested(&turn_id) {
        return Err(cancellation_error());
    }
    let search_flags = SearchFlags::from_preferences(tool_preferences.as_ref());

    info!(
        force_kb_search = search_flags.force_kb_search,
        force_web_search = search_flags.force_web_search,
        force_wiki_search = search_flags.force_wiki_search,
        deep_research_mode = search_flags.deep_research_mode,
        force_followup_mode = search_flags.force_followup_mode,
        "chat_with_conversation: tool preferences"
    );

    let llm_start = Instant::now();
    // A cold load is the longest stretch of the turn. It is not raced against
    // the cancellation flag: dropping it mid-flight would abandon a half-started
    // sidecar. The turn is registered by now, so a stop press during the load is
    // recorded and acted on the moment it returns — before any generation.
    let llm = container.get_or_load_llm().await?;
    flow_metrics.load_llm_ms = elapsed_ms(llm_start);

    let conversation_init_start = Instant::now();
    let conv_id =
        get_or_create_conversation_id(container, conversation_id, &validated_message, &llm).await?;
    if turn_guard.is_none() {
        turn_guard = Some(TurnCancellationGuard::start(turn_id.clone(), &conv_id)?);
    }
    let _turn_guard = turn_guard;
    flow_metrics.conversation_init_ms = elapsed_ms(conversation_init_start);
    if is_cancel_requested(&turn_id) {
        return Err(cancellation_error());
    }

    // Both emits and accumulates: everything below reports what it is doing
    // through this, and the same list is what the finished answer carries.
    let recorder = TurnRecorder::with_emitter(&conv_id, &turn_id, {
        let emit = Arc::clone(&emit);
        Box::new(move |payload| {
            if let Err(error) = emit(payload) {
                warn!(%error, "Failed to emit a turn step");
            }
        })
    });
    if search_flags.deep_research_mode {
        recorder.note(
            turn_record::TurnStepKind::DeepResearch,
            "Deep research",
            None,
            Some("searches wider, follows up on what it finds, and may go back for more".into()),
        );
    }
    // Resolved once, against the conversation's space scope. A request may
    // narrow what the turn reads; it may never widen it.
    let focus = FocusScope::resolve(
        container,
        &conv_id,
        tool_preferences
            .as_ref()
            .and_then(|preferences| preferences.focus_document_ids.as_ref()),
        search_flags.closed_book,
    )
    .await;
    // A focus that survived the intersection is an explicit instruction to read
    // those documents, so the vault is searched whether or not the router would
    // have asked for it. A focus that survived *nothing* forces the search too,
    // because that is the only path that reports why the turn read nothing —
    // skipping it would leave a fail-closed turn silently indistinguishable
    // from one that simply had no reason to search.
    let search_flags = SearchFlags {
        force_kb_search: search_flags.force_kb_search || focus.is_requested(),
        ..search_flags
    };
    // An Explorer conversation reads a folder beside the chat. The stored root
    // is the boundary for every folder tool, and the open file the request
    // names is checked against it here, once.
    let explorer = ExplorerTurn::resolve(
        container.db_pool(),
        &conv_id,
        tool_preferences
            .as_ref()
            .and_then(|preferences| preferences.explorer_focus.as_ref()),
    )
    .await;

    let conv_service = container.conversation_service();

    let settings_start = Instant::now();
    let settings = container.settings().await.unwrap_or_default();
    flow_metrics.settings_load_ms = elapsed_ms(settings_start);
    let prompt_settings = normalize_prompt_settings(settings.llm.prompts.clone());
    let tool_output_settings = settings.llm.tool_output.clone();
    let router_settings = settings.llm.router.clone();
    let highlight_terms = extract_highlight_terms(
        &validated_message,
        tool_output_settings.highlight_terms_max as usize,
    );

    let max_tokens = llm.max_context_tokens();
    let context_build_start = Instant::now();
    let (context, conversation_document_context, linked_web_sources_context, system_prompt) =
        build_conversation_context(
            container.conversation_context(),
            container.conversation_history(),
            &conv_id,
            &llm,
            max_tokens,
            &prompt_settings.system_prompt,
        )
        .await?;
    flow_metrics.context_build_ms = elapsed_ms(context_build_start);
    // Conversation prompt, else the space's, else the global one. The memory
    // plan takes it as a typed policy rather than the `System:` context entry.
    let system_prompt = system_prompt.unwrap_or_default();
    let conversation_document_context =
        confine_document_context(container, &conv_id, conversation_document_context).await;
    // Pages linked to the conversation are outside material too.
    let linked_web_sources_context =
        linked_web_sources_context.filter(|_| !search_flags.closed_book);

    // The utility model infers what an unflagged turn needs (vault search, web,
    // follow-up) so an obvious case retrieves even with every toggle on its
    // default. Enable-only: it can add retrieval, never take any away.
    //
    // Not in an Explorer turn: its subject is the folder on screen, and the
    // library and the web are read there only when the user turns them on.
    let search_flags = if explorer.is_some() {
        search_flags
    } else {
        infer_turn_intent_flags(
            container,
            &llm,
            tool_preferences.as_ref(),
            search_flags,
            &validated_message,
            &context,
            super::cancellation::turn_token(&turn_id),
        )
        .await
    };

    // The files this turn brought in are read, not searched for. Retrieval can
    // miss them, and on a forced-web turn the vault is not searched at all, so
    // carrying them is the only way an attachment reliably reaches the model.
    // `closed_book` still wins: that turn carries its own material by
    // definition and reads nothing else.
    let question_tokens = llm.count_tokens(&validated_message);
    let context_history_tokens: usize = context.iter().map(|entry| llm.count_tokens(entry)).sum();
    // With bounded memory on, the plan — not this string context — carries the
    // history, and it spends at most its own history pools on it.
    let context_history_tokens = if settings.llm.bounded_conversation_memory {
        memory_context::history_tokens_for_rag_budget(
            llm.as_ref(),
            &system_prompt,
            question_tokens,
            context_history_tokens,
        )
    } else {
        context_history_tokens
    };
    let attachment_names = attachment_names.unwrap_or_default();
    let attachment_ids = attachment_document_ids.unwrap_or_default();
    let attachments = if search_flags.closed_book {
        TurnAttachments::default()
    } else if attachment_ids.is_empty() {
        // Files that never made it into the library — an import still running
        // when the message was sent, or one that failed. The chip is on the
        // message either way, so the turn says the file arrived and could not
        // be read rather than denying it.
        TurnAttachments::still_importing(&attachment_names)
    } else {
        build_turn_attachments(
            container,
            &conv_service,
            &conv_id,
            &attachment_ids,
            attachment_token_budget(available_rag_budget(
                max_tokens,
                question_tokens,
                context_history_tokens,
                0,
            )),
            &llm,
            &highlight_terms,
            tool_output_settings.excerpt_chars as usize,
        )
        .await
    };
    let attachment_tokens = attachments.prompt_tokens(&llm);
    // The folder block is the reader's own material too, so it is budgeted
    // before retrieval the way an attachment is: what it fills is not there
    // for library passages. When the folder has an index, the block also
    // carries what it finds for the question, which is how a model without
    // tool calling gets past the open file. A closed-book turn searches
    // nothing, the folder's index included.
    let explorer_context = match &explorer {
        Some(turn) => Some(
            turn.context_block(
                explorer_block_chars(max_tokens),
                llm.supports_tool_calling() && !search_flags.closed_book,
                (!search_flags.closed_book).then_some(validated_message.as_str()),
            )
            .await,
        ),
        None => None,
    };
    let explorer_tokens = explorer_context
        .as_deref()
        .map_or(0, |text| llm.count_tokens(text));
    if !attachments.is_empty() {
        let carried = attachments.carried_count();
        let unreadable = attachments.unreadable_names().len();
        recorder.note(
            turn_record::TurnStepKind::OpenDocument,
            if carried == 1 && unreadable == 0 {
                "Read the attached file".to_string()
            } else {
                "Read the attached files".to_string()
            },
            None,
            Some(match (carried, unreadable) {
                (0, _) => "attached, but no readable text yet".to_string(),
                (_, 0) => format!("{carried} file{}", if carried == 1 { "" } else { "s" }),
                _ => format!("{carried} read, {unreadable} unreadable"),
            }),
        );
    }
    let available_for_rag = available_rag_budget(
        max_tokens,
        question_tokens,
        context_history_tokens,
        attachment_tokens + explorer_tokens,
    );
    // A message that only points at a file ("reference the chat I attached")
    // has no subject of its own, so the web-query rewrite has to read one off
    // the attachment or it searches for the request instead of the question.
    let attachment_digest = attachments.subject_digest(ATTACHMENT_DIGEST_CHARS);

    let router_start = Instant::now();
    let (router_decision, router_record) = resolve_router_decision(
        container,
        &conversation_document_context,
        &validated_message,
        search_flags,
        explorer.is_some(),
        &router_settings,
        &recorder,
        super::cancellation::turn_token(&turn_id),
    )
    .await?;
    flow_metrics.router_ms = elapsed_ms(router_start);
    if is_cancel_requested(&turn_id) {
        return Err(cancellation_error());
    }

    // DIAGNOSTIC: Log context info before LLM generation
    tracing::info!(
        conversation_id = conv_id.as_str(),
        context_messages = context.len(),
        max_tokens = max_tokens,
        "chat_with_conversation: Context built, starting LLM generation"
    );

    // Evidence recall runs independently of web/KB search toggles. Forced web
    // research supplements a conversation; it does not reset its source set.
    let mut allowed_prior_documents: HashSet<String> = conversation_document_context
        .iter()
        .map(|reference| reference.document_id.clone())
        .collect();
    focus.confine(&mut allowed_prior_documents);
    let prior_sources = if search_flags.closed_book {
        Vec::new()
    } else {
        prior_evidence::recall(
            container,
            &conv_id,
            &validated_message,
            &allowed_prior_documents,
            &attachments.sources(),
            (available_for_rag / 4).min(4000),
            &llm,
        )
        .await?
    };
    let prior_evidence_tokens = prior_evidence::render(&prior_sources)
        .as_deref()
        .map(|text| llm.count_tokens(text))
        .unwrap_or(0);
    let available_for_rag = available_for_rag.saturating_sub(prior_evidence_tokens);

    let retrieval_start = Instant::now();
    let mut retrieval = run_retrieval_pipeline(
        container,
        &conv_service,
        &conv_id,
        &turn_id,
        &validated_message,
        &llm,
        &router_settings,
        &router_decision,
        search_flags,
        &conversation_document_context,
        &highlight_terms,
        available_for_rag,
        attachment_digest.as_deref(),
        &tool_output_settings,
        &settings.search,
        &focus,
        &recorder,
    )
    .await;
    flow_metrics.retrieval_pipeline_ms = elapsed_ms(retrieval_start);
    flow_metrics.retrieval_subtimings = Some(retrieval.sub_timings.clone());
    if is_cancel_requested(&turn_id) {
        return Err(cancellation_error());
    }

    let prompt_build_start = Instant::now();
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
    // user's own passages.
    // Counted before the numbers exist; a label is a token or two either way.
    let web_context_tokens: usize = retrieval
        .web_context
        .iter()
        .map(|item| llm.count_tokens(&item.render(0)))
        .sum();
    let budgeted_results = budget_search_results_for_prompt(
        &retrieval.search_response.results,
        retrieval
            .available_for_rag
            .saturating_sub(web_context_tokens),
        &llm,
    );
    if budgeted_results.len() < retrieval.search_response.results.len() {
        let rag_tokens_used: usize = budgeted_results
            .iter()
            .map(|result| llm.count_tokens(&result.content))
            .sum();
        info!(
            kept = budgeted_results.len(),
            total = retrieval.search_response.results.len(),
            tokens_used = rag_tokens_used,
            budget = retrieval.available_for_rag,
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
                conversation_id = conv_id.as_str(),
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

    // Emitted before generation so the frontend can show retrieval progress.
    // so the UI can show its reading before its writing, and persisted into the
    // assistant message metadata so it survives a reload. Nothing is emitted
    // when KB retrieval never ran — an absent trace is not a trace of zeros.
    let mut retrieval_trace = if retrieval.searched_documents > 0
        || !retrieval.sources.is_empty()
        || retrieval.kb_unavailable_reason.is_some()
    {
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
            focused_documents: focus.reported_count(),
        };
        if let Err(e) = emit(ChatStreamEventDto {
            status: Some("retrieval".to_owned()),
            retrieval: Some(trace.clone()),
            ..ChatStreamEventDto::new(&conv_id, &turn_id)
        }) {
            warn!(error = %e, "Failed to emit retrieval trace");
        }
        Some(trace)
    } else {
        None
    };

    let kb_context = build_kb_context(
        &budgeted_results,
        &citation_ids_by_chunk(&retrieval.sources),
    );
    let has_linked_web_sources_context = linked_web_sources_context.is_some();
    let retrieval_web_context = web_context.is_some();
    let has_grounded_context = attachment_context.is_some()
        || followup_context_text.is_some()
        || kb_context.is_some()
        || retrieval_web_context
        || has_linked_web_sources_context
        || prior_evidence_context.is_some();

    let searches_again = deep_research_searches_again(
        search_flags,
        tool_preferences.as_ref(),
        llm.supports_tool_calling(),
    );
    let enhanced_message = PromptMessageBuilder::new(
        &prompt_settings,
        &validated_message,
        retrieval.interpretation.query_type == QueryType::Greeting,
        search_flags,
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
    flow_metrics.prompt_build_ms = elapsed_ms(prompt_build_start);

    let persist_user_message_start = Instant::now();
    let (user_message_id, message_tokens) = persist_user_message_pending(
        &conv_service,
        &conv_id,
        &turn_id,
        &validated_message,
        &attachment_names,
        &attachment_ids,
        &llm,
    )
    .await?;
    flow_metrics.persist_user_message_ms = elapsed_ms(persist_user_message_start);

    let tool_prep_start = Instant::now();
    let supports_tools = llm.supports_tool_calling();
    let tool_definitions = build_llm_tool_definitions(
        container,
        &llm,
        tool_preferences.as_ref(),
        &settings.llm.custom_tools,
        retrieval_web_context || has_linked_web_sources_context,
    );
    let force_tools_for_turn = tool_preferences
        .as_ref()
        .and_then(|preferences| preferences.turn_mode.as_deref())
        .map(str::trim)
        .is_some_and(|mode| mode.eq_ignore_ascii_case("query"));
    let grounded_tools: Vec<_> = tool_definitions
        .iter()
        .filter(|tool| {
            kept_with_grounded_context(
                &tool.name,
                retrieval_web_context || has_linked_web_sources_context,
                search_flags.deep_research_mode,
            )
        })
        .cloned()
        .collect();
    let selected_tools = if has_grounded_context && !force_tools_for_turn {
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
        history_tools::tools_for_turn(selected_tools, search_flags.closed_book, supports_tools);
    // The folder tools ride on top of whichever branch was chosen above, the
    // way history access does: a turn grounded in library passages may still
    // need to open the file the question is about. A closed-book turn reads
    // nothing outside its message, the folder included.
    if let Some(turn) = explorer
        .as_ref()
        .filter(|_| supports_tools && !search_flags.closed_book)
    {
        crate::features::explorer::tools::add_to_turn(&mut turn_tools, turn.has_folder_index());
    }
    let tools_ref = (!turn_tools.is_empty()).then_some(turn_tools.as_slice());
    flow_metrics.tool_prep_ms = elapsed_ms(tool_prep_start);

    info!(
        provider = llm.provider_name(),
        supports_tools = supports_tools,
        tool_count = turn_tools.len(),
        history_tools_offered = turn_tools
            .iter()
            .filter(|tool| history_tools::is_history_tool(&tool.name))
            .count(),
        tools_enabled_for_turn = tools_ref.is_some(),
        force_tools_for_turn = force_tools_for_turn,
        has_grounded_context = has_grounded_context,
        "LLM capability check for conversation"
    );

    // Enabled memory must account for unprocessed history before generation,
    // including conversations that have no extracted ledger yet.
    let memory_plan_start = Instant::now();
    let repository = crate::features::conversation::repository::ConversationRepository::new(
        container.db_pool().clone(),
    )
    .with_memory_embedding(container.get_or_load_embedding().await.ok());
    // Counted as sent: the JSON `parameters` of every tool are prompt text the
    // model reads, and on a 4k-token local window they are a large share of it.
    let tool_schema_tokens: usize = tools_ref.map_or(0, |tools| {
        llm.count_tokens(&tool_loop::tool_schema_text(tools))
    });
    let memory_turn = memory_context::prepare_memory_turn(
        || async {
            let turn = memory_context::build_memory_plan_for_query(
                &repository,
                &repository,
                &llm,
                &conv_id,
                &system_prompt,
                &enhanced_message,
                tool_schema_tokens,
                Vec::new(),
                settings.llm.bounded_conversation_memory,
                &validated_message,
            )
            .await?;

            if let Some(turn) = &turn {
                info!(
                    conversation_id = conv_id.as_str(),
                    memory_revision = turn.plan.memory_revision,
                    active_mandatory = turn.plan.accounting.active_mandatory_count,
                    conflicts = turn.plan.accounting.active_conflict_count,
                    total_input_tokens = turn.plan.accounting.total_input,
                    input_budget = turn.plan.accounting.input_budget,
                    accounting = ?turn.plan.accounting.accounting_method,
                    recall_passages = turn.plan.retrieval.passages_selected,
                    recall_error = ?turn.plan.retrieval.index_error,
                    compaction_required = turn.plan.compaction_required,
                    evicted = ?turn.plan.accounting.evicted,
                    elapsed_ms = elapsed_ms(memory_plan_start),
                    "Assembled bounded conversation memory for this turn"
                );
            }
            Ok(turn)
        },
        || container.compact_for_turn(conv_id.as_str(), super::cancellation::turn_token(&turn_id)),
    )
    .await;

    let memory_usage = memory_turn
        .as_ref()
        .ok()
        .and_then(|turn| turn.as_ref())
        .map(|turn| {
            serde_json::json!({
                "items": turn.plan.used_memory_ids,
                "revision": turn.plan.memory_revision,
                "recallPassages": turn.plan.retrieval.passages_selected,
            })
        });
    let mut sources = retrieval.sources;
    let short_circuit_response = retrieval.short_circuit_response.take();
    let generation_start = Instant::now();
    let mut turn_tokens = TurnTokensDto::default();
    // Route preparation failures through normal turn cleanup, so the saved
    // user message becomes retryable rather than remaining pending forever.
    let response_result: Result<String> = match memory_turn {
        Err(error) => Err(error),
        Ok(memory_turn) => {
            if let Some(response) = short_circuit_response {
                flow_metrics.generation_subtimings = Some(ToolLoopTimingMetrics::default());
                Ok(response)
            } else {
                match run_agentic_tool_loop(
                    container,
                    &conv_service,
                    &conv_id,
                    &turn_id,
                    &llm,
                    &emit,
                    &context,
                    &enhanced_message,
                    &prompt_settings,
                    &highlight_terms,
                    &tool_output_settings,
                    &mut sources,
                    &mut retrieval_trace,
                    tools_ref,
                    generation_time_budget(search_flags),
                    std::mem::take(&mut retrieval.pages_read),
                    &focus,
                    explorer.as_ref(),
                    &recorder,
                    memory_turn.as_ref().map(|turn| &turn.plan),
                    response_token_budget(max_tokens),
                    tool_loop::max_tool_rounds(search_flags.deep_research_mode, explorer.is_some()),
                )
                .await
                {
                    Ok(tool_loop_outcome) => {
                        flow_metrics.generation_subtimings = Some(tool_loop_outcome.timings);
                        turn_tokens = TurnTokensDto {
                            completion: tool_loop_outcome.output_tokens,
                            context_used: tool_loop_outcome.input_tokens,
                        };
                        Ok(tool_loop_outcome.response)
                    }
                    Err(e) => Err(e),
                }
            }
        }
    };
    flow_metrics.generation_ms = elapsed_ms(generation_start);

    match response_result {
        Ok(response) => {
            // DIAGNOSTIC: Log successful generation
            tracing::info!(
                conversation_id = conv_id.as_str(),
                response_length = response.len(),
                "chat_with_conversation: LLM generation successful"
            );

            // Some providers can terminate a stream without yielding content.
            // Persist a friendly fallback message instead of failing the whole turn.
            let assistant_response = if response.trim().is_empty() {
                warn!(
                    conversation_id = conv_id.as_str(),
                    "LLM returned empty response; using fallback message"
                );
                "I couldn't generate a response for that turn. Please try rephrasing your question."
                    .to_string()
            } else {
                response
            };
            let verification_start = Instant::now();
            // A closed-book turn cites nothing, so there is nothing to ground
            // it against — judging it only reports every sentence unsupported.
            let verification_enabled =
                settings.llm.verification.enabled && !search_flags.closed_book;
            // Begun here and left running: the check itself happens after the
            // answer is persisted and returned (see `BackgroundVerification`),
            // which finishes this step in the persisted record when it lands.
            if verification_enabled {
                recorder.begin(TurnStepKind::Verify, "Checking the answer", None);
            }
            let verification_metadata = Some(if verification_enabled {
                pending_metadata()
            } else {
                serde_json::json!({ "enabled": false })
            });
            flow_metrics.verification_ms = elapsed_ms(verification_start);

            // Built here rather than after persistence so `totalMs` is time to
            // the answer the reader is about to see, and so the step list is
            // exactly what was streamed.
            if turn_tokens.completion.is_none() {
                turn_tokens.completion = Some(llm.count_tokens(&assistant_response) as u64);
            }
            let turn_record = TurnRecordDto {
                model: Some(TurnModelDto {
                    id: llm.model_name().to_string(),
                    name: llm.model_name().to_string(),
                }),
                steps: recorder.steps(),
                timing: TurnTimingDto {
                    total_ms: elapsed_ms(flow_start),
                    router_ms: flow_metrics.router_ms,
                    retrieval_ms: flow_metrics.retrieval_pipeline_ms,
                    generation_ms: flow_metrics.generation_ms,
                    verification_ms: flow_metrics.verification_ms,
                    tool_ms: generation_subtimings_or_default(&flow_metrics).tool_execution_ms,
                },
                tokens: turn_tokens,
                router: router_record,
            };

            let background_verification = verification_enabled.then(|| {
                (
                    assistant_response.clone(),
                    sources.clone(),
                    turn_record.clone(),
                )
            });

            let finalize_start = Instant::now();
            // Finalization marks the question failed itself when the commit
            // does not land, and reports success once it has: a failure after
            // the answer is saved is not a failed turn.
            let (mut chat_response, answer_id) = finalize_successful_turn(
                container,
                &conv_service,
                &conv_id,
                &user_message_id,
                &validated_message,
                assistant_response,
                context.len(),
                sources,
                verification_metadata,
                memory_usage,
                retrieval_trace.clone(),
                Some(turn_record),
                message_tokens,
                &llm,
            )
            .await?;
            flow_metrics.finalize_persistence_ms = elapsed_ms(finalize_start);

            if let Some((response, sources, turn)) = background_verification {
                let message_id = answer_id;
                let emit = Arc::clone(&emit);
                let load_utility_llm = container.utility_llm_loader();
                BackgroundVerification {
                    conversation_repository: Arc::new(
                        crate::features::conversation::repository::ConversationRepository::new(
                            container.db_pool().clone(),
                        ),
                    ),
                    conversation_service: Arc::clone(&conv_service),
                    load_utility_llm,
                    conversation_id: conv_id.clone(),
                    request_id: turn_id.clone(),
                    message_id,
                    response,
                    sources,
                    tuning: settings.llm.verification.clone(),
                    chat_llm: Arc::clone(&llm),
                    turn: Some(turn),
                    started: verification_start,
                    emit: Arc::new(move |payload| {
                        if let Err(error) = emit(payload) {
                            warn!(%error, "Failed to emit a finished grounding check");
                        }
                    }),
                }
                .spawn()
                .await;
            }
            flow_metrics.total_ms = elapsed_ms(flow_start);
            let retrieval_sub = retrieval_subtimings_or_default(&flow_metrics);
            let generation_sub = generation_subtimings_or_default(&flow_metrics);

            info!(
                conversation_id = conv_id.as_str(),
                validate_request_ms = flow_metrics.validate_request_ms,
                load_llm_ms = flow_metrics.load_llm_ms,
                conversation_init_ms = flow_metrics.conversation_init_ms,
                settings_load_ms = flow_metrics.settings_load_ms,
                context_build_ms = flow_metrics.context_build_ms,
                router_ms = flow_metrics.router_ms,
                retrieval_pipeline_ms = flow_metrics.retrieval_pipeline_ms,
                retrieval_sub_total_ms = retrieval_sub.total_ms,
                retrieval_sub_kb_total_ms = retrieval_sub.kb_total_ms,
                retrieval_sub_kb_scope_load_ms = retrieval_sub.kb_scope_load_ms,
                retrieval_sub_kb_hyde_interpretation_ms = retrieval_sub.kb_hyde_interpretation_ms,
                retrieval_sub_kb_search_plan_ms = retrieval_sub.kb_search_plan_ms,
                retrieval_sub_kb_shortlist_planning_ms = retrieval_sub.kb_shortlist_planning_ms,
                retrieval_sub_kb_query_execution_ms = retrieval_sub.kb_query_execution_ms,
                retrieval_sub_kb_merge_shortlist_gate_ms = retrieval_sub.kb_merge_shortlist_gate_ms,
                retrieval_sub_kb_post_filters_ms = retrieval_sub.kb_post_filters_ms,
                retrieval_sub_kb_rerank_ms = retrieval_sub.kb_rerank_ms,
                retrieval_sub_kb_build_sources_ms = retrieval_sub.kb_build_sources_ms,
                retrieval_sub_kb_persist_references_ms = retrieval_sub.kb_persist_references_ms,
                retrieval_sub_external_hyde_interpretation_ms =
                    retrieval_sub.external_hyde_interpretation_ms,
                retrieval_sub_wiki_search_ms = retrieval_sub.wiki_search_ms,
                retrieval_sub_web_search_ms = retrieval_sub.web_search_ms,
                prompt_build_ms = flow_metrics.prompt_build_ms,
                persist_user_message_ms = flow_metrics.persist_user_message_ms,
                tool_prep_ms = flow_metrics.tool_prep_ms,
                generation_ms = flow_metrics.generation_ms,
                generation_sub_total_ms = generation_sub.total_ms,
                generation_sub_iterations = generation_sub.iterations,
                generation_sub_llm_stream_ms = generation_sub.llm_stream_ms,
                generation_sub_tool_execution_ms = generation_sub.tool_execution_ms,
                generation_sub_tool_call_count = generation_sub.tool_call_count,
                generation_sub_tool_success_count = generation_sub.tool_success_count,
                generation_sub_tool_failure_count = generation_sub.tool_failure_count,
                generation_sub_empty_response_retries = generation_sub.empty_response_retries,
                generation_sub_followup_prompt_build_ms = generation_sub.followup_prompt_build_ms,
                verification_ms = flow_metrics.verification_ms,
                finalize_persistence_ms = flow_metrics.finalize_persistence_ms,
                total_ms = flow_metrics.total_ms,
                "chat_with_conversation: flow timing metrics"
            );

            chat_response.timing_metrics = Some(flow_metrics.clone());
            Ok(chat_response)
        }
        Err(e) => {
            // DIAGNOSTIC: Log generation failure with full error details
            tracing::error!(
                conversation_id = conv_id.as_str(),
                error = %e,
                "chat_with_conversation: LLM generation FAILED"
            );

            mark_user_message_failed(&conv_service, &user_message_id).await;

            flow_metrics.total_ms = elapsed_ms(flow_start);
            let retrieval_sub = retrieval_subtimings_or_default(&flow_metrics);
            let generation_sub = generation_subtimings_or_default(&flow_metrics);
            tracing::error!(
                conversation_id = conv_id.as_str(),
                validate_request_ms = flow_metrics.validate_request_ms,
                load_llm_ms = flow_metrics.load_llm_ms,
                conversation_init_ms = flow_metrics.conversation_init_ms,
                settings_load_ms = flow_metrics.settings_load_ms,
                context_build_ms = flow_metrics.context_build_ms,
                router_ms = flow_metrics.router_ms,
                retrieval_pipeline_ms = flow_metrics.retrieval_pipeline_ms,
                retrieval_sub_total_ms = retrieval_sub.total_ms,
                retrieval_sub_kb_total_ms = retrieval_sub.kb_total_ms,
                retrieval_sub_kb_scope_load_ms = retrieval_sub.kb_scope_load_ms,
                retrieval_sub_kb_hyde_interpretation_ms = retrieval_sub.kb_hyde_interpretation_ms,
                retrieval_sub_kb_search_plan_ms = retrieval_sub.kb_search_plan_ms,
                retrieval_sub_kb_shortlist_planning_ms = retrieval_sub.kb_shortlist_planning_ms,
                retrieval_sub_kb_query_execution_ms = retrieval_sub.kb_query_execution_ms,
                retrieval_sub_kb_merge_shortlist_gate_ms = retrieval_sub.kb_merge_shortlist_gate_ms,
                retrieval_sub_kb_post_filters_ms = retrieval_sub.kb_post_filters_ms,
                retrieval_sub_kb_rerank_ms = retrieval_sub.kb_rerank_ms,
                retrieval_sub_kb_build_sources_ms = retrieval_sub.kb_build_sources_ms,
                retrieval_sub_kb_persist_references_ms = retrieval_sub.kb_persist_references_ms,
                retrieval_sub_external_hyde_interpretation_ms =
                    retrieval_sub.external_hyde_interpretation_ms,
                retrieval_sub_wiki_search_ms = retrieval_sub.wiki_search_ms,
                retrieval_sub_web_search_ms = retrieval_sub.web_search_ms,
                prompt_build_ms = flow_metrics.prompt_build_ms,
                persist_user_message_ms = flow_metrics.persist_user_message_ms,
                tool_prep_ms = flow_metrics.tool_prep_ms,
                generation_ms = flow_metrics.generation_ms,
                generation_sub_total_ms = generation_sub.total_ms,
                generation_sub_iterations = generation_sub.iterations,
                generation_sub_llm_stream_ms = generation_sub.llm_stream_ms,
                generation_sub_tool_execution_ms = generation_sub.tool_execution_ms,
                generation_sub_tool_call_count = generation_sub.tool_call_count,
                generation_sub_tool_success_count = generation_sub.tool_success_count,
                generation_sub_tool_failure_count = generation_sub.tool_failure_count,
                generation_sub_empty_response_retries = generation_sub.empty_response_retries,
                generation_sub_followup_prompt_build_ms = generation_sub.followup_prompt_build_ms,
                verification_ms = flow_metrics.verification_ms,
                finalize_persistence_ms = flow_metrics.finalize_persistence_ms,
                total_ms = flow_metrics.total_ms,
                "chat_with_conversation: flow timing metrics (failed turn)"
            );

            Err(e)
        }
    }
}
