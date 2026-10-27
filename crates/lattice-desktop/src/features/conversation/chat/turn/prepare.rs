//! Stage 1: everything the turn establishes before it decides what to read.
use super::budget::TurnBudget;
use super::*;
use crate::features::conversation::chat::ports::SettingsDto;
use crate::features::conversation::ConversationServiceTrait;
use lattice_core::application::contracts::images::ChatImageDto;
use lattice_core::application::ports::llm_port::{ModelRequirements, PinnedModel, ToolSupport};
use lattice_core::application::ports::{LLMPort, ToolDefinition};
use lattice_core::domain::conversation::DocumentReference;
use lattice_core::domain::models::model_ref::ModelRef;
use tokio_util::sync::CancellationToken;

/// The turn as it stands once the conversation is open and its history read.
pub(super) struct PreparedTurn {
    pub(super) turn_id: String,
    pub(super) conv_id: String,
    pub(super) llm: Arc<dyn LLMPort>,
    /// What answers this turn and why, as its record keeps it.
    pub(super) model: TurnModelDto,
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
    /// The pictures the reader attached, as stored: what the budget charged
    /// and what [`super::pictures`] prepares for the request.
    pub(super) pictures: Vec<ChatImageDto>,
    /// Messages of history the turn was planned with.
    pub(super) history_len: usize,
    /// Held for the whole turn: a model chosen in the meantime never reaches
    /// it, and dropping it releases the model instance.
    _pin: PinnedModel,
    /// Held for the whole turn: dropping it unregisters the turn's stop button.
    _guard: TurnCancellationGuard,
}

impl PreparedTurn {
    /// The turn as retrieval, the tool loop and persistence read it.
    pub(super) fn chat_turn(&self) -> ChatTurn<'_> {
        ChatTurn {
            conv_service: &self.conv_service,
            conversation_id: &self.conv_id,
            request_id: &self.turn_id,
            message: &self.message,
            llm: &self.llm,
            highlight_terms: &self.highlight_terms,
            focus: &self.focus,
            recorder: &self.recorder,
        }
    }

    /// The ids of the attached pictures this conversation holds.
    pub(super) fn picture_ids(&self) -> Vec<String> {
        self.pictures
            .iter()
            .map(|picture| picture.id.clone())
            .collect()
    }

    pub(super) fn cancel_token(&self) -> tokio_util::sync::CancellationToken {
        self._guard.turn_token()
    }

    /// The stop button, honoured between stages.
    pub(super) fn ensure_not_cancelled(&self) -> Result<()> {
        if self._guard.is_cancel_requested() {
            return Err(cancelled());
        }
        Ok(())
    }
}

pub(super) fn cancelled() -> AppError {
    cancellation::cancelled_error()
}

/// `parent` is the stop of the job running the turn, if one is.
/// `request_override` answers this request with another model than the
/// chat's own choice.
pub(super) async fn prepare_turn(
    container: &dyn ChatRuntime,
    conversation_id: Option<String>,
    message: &str,
    request_id: Option<String>,
    request_override: Option<&ModelRef>,
    image_ids: &[String],
    tool_preferences: Option<&ToolPreferences>,
    parent: Option<&CancellationToken>,
    emit: &ChatEventSink,
    metrics: &mut ConversationFlowTimingMetrics,
) -> Result<PreparedTurn> {
    let turn_id = request_id
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    // Register the turn before the slow work below (validation, opening the
    // conversation, a cold model load): the composer shows Stop from the moment it
    // sends. A named conversation registers here under the id exactly as the
    // caller wrote it, the same id a cancel arrives with; only a new one waits.
    let turn_cancellation = container.turn_cancellation();
    let mut turn_guard = match conversation_id.as_deref() {
        Some(id) if !id.trim().is_empty() => Some(TurnCancellationGuard::start(
            Arc::clone(&turn_cancellation),
            turn_id.clone(),
            id,
            parent,
        )?),
        _ => None,
    };

    let validate_start = Instant::now();
    let validated_message = validate_and_guard_chat_request(container, message).await?;
    metrics.validate_request_ms = elapsed_ms(validate_start);
    if turn_cancellation.is_cancel_requested(&turn_id) {
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

    // The conversation comes first: the model is the chat's own choice, so a
    // new chat has to exist before its model can be resolved.
    let conversation_init_start = Instant::now();
    let conv_id =
        get_or_create_conversation_id(container, conversation_id, &validated_message).await?;
    let guard = match turn_guard.take() {
        Some(guard) => guard,
        None => TurnCancellationGuard::start(
            Arc::clone(&turn_cancellation),
            turn_id.clone(),
            &conv_id,
            parent,
        )?,
    };
    metrics.conversation_init_ms = elapsed_ms(conversation_init_start);
    if turn_cancellation.is_cancel_requested(&turn_id) {
        return Err(cancelled());
    }

    let llm_start = Instant::now();
    // A cold load is the longest stretch of the turn. It is not raced against
    // the cancellation flag: dropping it mid-flight would abandon a half-started
    // sidecar. The turn is registered by now, so a stop press during the load is
    // recorded and acted on the moment it returns — before any generation.
    let (pin, model) = pin_model(
        container,
        &conv_id,
        &turn_id,
        request_override,
        ModelRequirements::default(),
        emit,
    )
    .await?;
    let llm = Arc::clone(&pin.llm);
    metrics.load_llm_ms = elapsed_ms(llm_start);
    if turn_cancellation.is_cancel_requested(&turn_id) {
        return Err(cancelled());
    }

    // Both emits and accumulates: everything below reports what it is doing
    // through this, and the same list is what the finished answer carries.
    let recorder = TurnRecorder::with_emitter(&conv_id, &turn_id, step_emitter(emit));
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
    // A surviving focus forces the vault search; so does a focus that survived
    // nothing, since only that path reports why the turn read nothing.
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

    // Sized before retrieval, so the evidence it reads leaves room for what
    // every evidence-carrying request also sends: the tool schemas and the
    // template the evidence travels in.
    let tool_schema_tokens = super::assemble::grounded_tool_tokens(
        container,
        &llm,
        &settings.llm.custom_tools,
        explorer.as_ref(),
        search_flags,
        tool_preferences,
    );
    // The reader's pictures are read before the budget is planned, so what
    // they cost is paid with the message, before any evidence is sized.
    let pictures = super::pictures::attached_rows(container, &conv_id, image_ids).await;
    let budget = TurnBudget::plan(
        &llm,
        &system_prompt,
        &crate::features::conversation::chat::prompting::evidence_frame(
            &prompt_settings,
            &validated_message,
            explorer.is_some(),
        ),
        &context,
        settings.llm.bounded_conversation_memory,
        tool_schema_tokens,
        super::pictures::planned_tokens(llm.as_ref(), &pictures),
        chrono::Local::now().date_naive(),
    );

    let history_len = context.len();
    Ok(PreparedTurn {
        turn_id,
        conv_id,
        llm,
        model,
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
        pictures,
        history_len,
        _pin: pin,
        _guard: guard,
    })
}

/// What a resumed research turn needs of its model. A request planned with
/// tools goes on offering them, and its rounds may have called them, so the
/// model resuming it must call tools too. One planned on a model without
/// tools resumes as it started, without them.
pub(super) fn resume_requirements(planned_tools: &[ToolDefinition]) -> ModelRequirements {
    let tools = if planned_tools.is_empty() {
        ToolSupport::NotNeeded
    } else {
        ToolSupport::Required
    };
    ModelRequirements {
        tools,
        ..ModelRequirements::default()
    }
}

/// A research turn carrying on after a restart: the conversation, the model
/// and the turn's settings, without reading history or planning again. What
/// it had planned and found is in `saved`.
///
/// The chat's choice is resolved again: a model that cannot be used now
/// fails the turn with its cause rather than resuming on another one.
pub(super) async fn resume_turn(
    container: &dyn ChatRuntime,
    request: &TurnRequest,
    saved: &Saved,
    parent: &CancellationToken,
    emit: &ChatEventSink,
) -> Result<PreparedTurn> {
    let (Some(conv_id), Some(turn_id)) =
        (request.conversation_id.clone(), request.request_id.clone())
    else {
        return Err(AppError::InvalidInput(
            "A saved research turn names its conversation and request.".into(),
        ));
    };
    let turn_cancellation = container.turn_cancellation();
    let guard = TurnCancellationGuard::start(
        Arc::clone(&turn_cancellation),
        turn_id.clone(),
        &conv_id,
        Some(parent),
    )?;
    let (pin, model) = pin_model(
        container,
        &conv_id,
        &turn_id,
        request.model.as_ref(),
        resume_requirements(&saved.assembled.tools),
        emit,
    )
    .await?;
    let llm = Arc::clone(&pin.llm);
    if turn_cancellation.is_cancel_requested(&turn_id) {
        return Err(cancelled());
    }
    let preferences = request.tool_preferences.as_ref();
    let flags = saved.assembled.flags;
    let recorder = TurnRecorder::resumed(
        &conv_id,
        &turn_id,
        step_emitter(emit),
        saved.steps(),
        saved.elapsed_ms(),
    );
    let focus = FocusScope::resolve(
        container,
        &conv_id,
        preferences.and_then(|preferences| preferences.focus_document_ids.as_ref()),
        flags.closed_book,
    )
    .await;
    let explorer = container
        .explorer_turn(
            &conv_id,
            preferences.and_then(|preferences| preferences.explorer_focus.as_ref()),
        )
        .await;
    let settings = container.settings().await.unwrap_or_default();
    let prompt_settings = normalize_prompt_settings(settings.llm.prompts.clone());
    let highlight_terms = extract_highlight_terms(
        &request.message,
        settings.llm.tool_output.highlight_terms_max as usize,
    );
    // Nothing is retrieved again, so no evidence is sized against this.
    let budget = TurnBudget::plan(
        &llm,
        "",
        &request.message,
        &[],
        settings.llm.bounded_conversation_memory,
        0,
        0,
        chrono::Local::now().date_naive(),
    );
    Ok(PreparedTurn {
        turn_id,
        conv_id,
        llm,
        model,
        message: request.message.clone(),
        requested_flags: flags,
        recorder,
        focus,
        explorer,
        conv_service: container.conversation_service(),
        settings,
        prompt_settings,
        highlight_terms,
        context: Vec::new(),
        document_context: Vec::new(),
        linked_web_sources_context: None,
        system_prompt: String::new(),
        budget,
        // The saved request already carries the pictures, as captions.
        pictures: Vec::new(),
        history_len: saved.assembled.history_len,
        _pin: pin,
        _guard: guard,
    })
}

/// The turn's model, resolved once and pinned until the turn ends, and
/// announced on the turn's stream before the first token so the reader knows
/// what is answering.
pub(super) async fn pin_model(
    container: &dyn ChatRuntime,
    conv_id: &str,
    turn_id: &str,
    request_override: Option<&ModelRef>,
    requirements: ModelRequirements,
    emit: &ChatEventSink,
) -> Result<(PinnedModel, TurnModelDto)> {
    let pin = container
        .chat_llm_for(conv_id, request_override, requirements)
        .await?;
    let model = TurnModelDto::pinned(&pin);
    let announced = ChatStreamEventDto {
        model: Some(model.clone()),
        ..ChatStreamEventDto::new(conv_id, turn_id)
    };
    if let Err(error) = emit(announced) {
        warn!(%error, "Failed to announce the turn's model");
    }
    Ok((pin, model))
}

/// Turn steps go out on the turn's stream as they happen.
fn step_emitter(emit: &ChatEventSink) -> Box<dyn Fn(ChatStreamEventDto) + Send + Sync> {
    let emit = Arc::clone(emit);
    Box::new(move |payload| {
        if let Err(error) = emit(payload) {
            warn!(%error, "Failed to emit a turn step");
        }
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
        cancellation: Arc<
            crate::features::conversation::chat::cancellation::TurnCancellationRegistry,
        >,
    ) -> Result<Self> {
        let settings = SettingsDto::default();
        let pin = PinnedModel {
            llm: Arc::clone(&llm),
            resolved: lattice_core::application::ports::llm_port::ResolvedModel {
                model: ModelRef::Local {
                    model_id: llm.model_name().to_string(),
                },
                origin: lattice_core::application::ports::llm_port::ModelOrigin::Default,
            },
            instance_id: 0,
            degraded: None,
        };
        Ok(Self {
            budget: TurnBudget::plan(
                &llm,
                "",
                "question",
                &[],
                true,
                0,
                0,
                chrono::Local::now().date_naive(),
            ),
            turn_id: turn_id.to_string(),
            conv_id: conv_id.to_string(),
            model: TurnModelDto::pinned(&pin),
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
            pictures: Vec::new(),
            history_len: 0,
            _pin: pin,
            _guard: TurnCancellationGuard::start(cancellation, turn_id.to_string(), conv_id, None)?,
        })
    }
}
