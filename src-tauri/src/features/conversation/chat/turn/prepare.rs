//! Stage 1: everything the turn establishes before it decides what to read.
use super::budget::TurnBudget;
use super::*;
use crate::application::ports::LLMPort;
use crate::domain::conversation::DocumentReference;
use crate::features::conversation::chat::ports::SettingsDto;
use crate::features::conversation::ConversationServiceTrait;

/// The turn as it stands once the conversation is open and its history read.
pub(super) struct PreparedTurn {
    pub(super) turn_id: String,
    pub(super) conv_id: String,
    pub(super) llm: Arc<dyn LLMPort>,
    /// The user's message, validated.
    pub(super) message: String,
    /// What the request asked for, widened by an explicit document focus.
    pub(super) requested_flags: SearchFlags,
    pub(super) recorder: TurnRecorder,
    pub(super) focus: FocusScope,
    pub(super) explorer: Option<ExplorerTurn>,
    pub(super) conv_service: Arc<dyn ConversationServiceTrait>,
    pub(super) settings: SettingsDto,
    pub(super) prompt_settings: LLMPromptSettingsDto,
    pub(super) highlight_terms: Vec<String>,
    /// The string history, `System:` entry included.
    pub(super) context: Vec<String>,
    /// Documents the conversation has read, confined to its space.
    pub(super) document_context: Vec<DocumentReference>,
    /// Pages linked to the conversation; `None` on a closed-book turn.
    pub(super) linked_web_sources_context: Option<String>,
    /// Conversation prompt, else the space's, else the global one.
    pub(super) system_prompt: String,
    pub(super) budget: TurnBudget,
    /// Held for the whole turn: dropping it unregisters the turn's stop button.
    _guard: TurnCancellationGuard,
}

impl PreparedTurn {
    pub(super) fn cancel_token(&self) -> tokio_util::sync::CancellationToken {
        crate::features::conversation::chat::cancellation::turn_token(&self.turn_id)
    }

    /// The stop button, honoured between stages.
    pub(super) fn ensure_not_cancelled(&self) -> Result<()> {
        if is_cancel_requested(&self.turn_id) {
            return Err(cancelled());
        }
        Ok(())
    }
}

pub(super) fn cancelled() -> AppError {
    AppError::InvalidState("Generation cancelled by user.".to_string())
}

pub(super) async fn prepare_turn(
    container: &dyn ChatRuntime,
    conversation_id: Option<String>,
    message: &str,
    request_id: Option<String>,
    tool_preferences: Option<&ToolPreferences>,
    emit: &ChatEventSink,
    metrics: &mut ConversationFlowTimingMetrics,
) -> Result<PreparedTurn> {
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

    let validate_start = Instant::now();
    let validated_message = validate_and_guard_chat_request(container, message).await?;
    metrics.validate_request_ms = elapsed_ms(validate_start);
    if is_cancel_requested(&turn_id) {
        return Err(cancelled());
    }
    let search_flags = SearchFlags::from_preferences(tool_preferences);

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
    metrics.load_llm_ms = elapsed_ms(llm_start);

    let conversation_init_start = Instant::now();
    let conv_id =
        get_or_create_conversation_id(container, conversation_id, &validated_message, &llm).await?;
    let guard = match turn_guard.take() {
        Some(guard) => guard,
        None => TurnCancellationGuard::start(turn_id.clone(), &conv_id)?,
    };
    metrics.conversation_init_ms = elapsed_ms(conversation_init_start);
    if is_cancel_requested(&turn_id) {
        return Err(cancelled());
    }

    // Both emits and accumulates: everything below reports what it is doing
    // through this, and the same list is what the finished answer carries.
    let recorder = TurnRecorder::with_emitter(&conv_id, &turn_id, {
        let emit = Arc::clone(emit);
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
        tool_preferences.and_then(|preferences| preferences.focus_document_ids.as_ref()),
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
    let explorer = container
        .explorer_turn(
            &conv_id,
            tool_preferences.and_then(|preferences| preferences.explorer_focus.as_ref()),
        )
        .await;

    let conv_service = container.conversation_service();

    let settings_start = Instant::now();
    let settings = container.settings().await.unwrap_or_default();
    metrics.settings_load_ms = elapsed_ms(settings_start);
    let prompt_settings = normalize_prompt_settings(settings.llm.prompts.clone());
    let highlight_terms = extract_highlight_terms(
        &validated_message,
        settings.llm.tool_output.highlight_terms_max as usize,
    );

    let context_build_start = Instant::now();
    let (context, document_context, linked_web_sources_context, system_prompt) =
        build_conversation_context(
            container.conversation_context(),
            container.conversation_history(),
            &conv_id,
            &llm,
            llm.max_context_tokens(),
            &prompt_settings.system_prompt,
        )
        .await?;
    metrics.context_build_ms = elapsed_ms(context_build_start);
    // The memory plan takes the system prompt as a typed policy rather than
    // the `System:` context entry.
    let system_prompt = system_prompt.unwrap_or_default();
    let document_context = confine_document_context(container, &conv_id, document_context).await;
    // Pages linked to the conversation are outside material too.
    let linked_web_sources_context =
        linked_web_sources_context.filter(|_| !search_flags.closed_book);

    let budget = TurnBudget::plan(
        llm.as_ref(),
        &system_prompt,
        &validated_message,
        &context,
        settings.llm.bounded_conversation_memory,
    );

    Ok(PreparedTurn {
        turn_id,
        conv_id,
        llm,
        message: validated_message,
        requested_flags: search_flags,
        recorder,
        focus,
        explorer,
        conv_service,
        settings,
        prompt_settings,
        highlight_terms,
        context,
        document_context,
        linked_web_sources_context,
        system_prompt,
        budget,
        _guard: guard,
    })
}

#[cfg(test)]
impl PreparedTurn {
    /// A turn past preparation with an empty history and default settings,
    /// registered so its stop button works.
    pub(super) fn for_test(
        conv_id: &str,
        turn_id: &str,
        llm: Arc<dyn LLMPort>,
        conv_service: Arc<dyn ConversationServiceTrait>,
    ) -> Result<Self> {
        let settings = SettingsDto::default();
        Ok(Self {
            budget: TurnBudget::plan(llm.as_ref(), "", "question", &[], true),
            turn_id: turn_id.to_string(),
            conv_id: conv_id.to_string(),
            llm,
            message: "question".to_string(),
            requested_flags: SearchFlags::from_preferences(None),
            recorder: TurnRecorder::silent(conv_id, turn_id),
            focus: FocusScope::default(),
            explorer: None,
            conv_service,
            prompt_settings: normalize_prompt_settings(settings.llm.prompts.clone()),
            settings,
            highlight_terms: Vec::new(),
            context: Vec::new(),
            document_context: Vec::new(),
            linked_web_sources_context: None,
            system_prompt: String::new(),
            _guard: TurnCancellationGuard::start(turn_id.to_string(), conv_id)?,
        })
    }
}
