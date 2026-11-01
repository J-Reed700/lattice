use super::*;
use lattice_core::application::ports::turn_source::TurnSourcePolicy;

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub(super) struct SearchFlags {
    pub(super) force_kb_search: bool,
    /// The user asked for the web: the web-search toggle, or the query turn
    /// mode. Searched whatever the vault holds, and beside it.
    pub(super) force_web_search: bool,
    /// The intent step guessed the turn may need the web. Not a search of its
    /// own: the vault is searched first, and the web only when the vault came
    /// back empty or the sufficiency check judged its evidence too weak. Only
    /// `apply_turn_intent` sets it, so it lives for one turn and no later turn
    /// inherits it.
    pub(super) web_fallback: bool,
    pub(super) force_wiki_search: bool,
    pub(super) deep_research_mode: bool,
    pub(super) force_followup_mode: bool,
    pub(super) closed_book: bool,
}

impl SearchFlags {
    pub(super) fn from_preferences(tool_preferences: Option<&ToolPreferences>) -> Self {
        let tool_preferences = tool_preferences.cloned().unwrap_or_default();
        let normalized_turn_mode = tool_preferences
            .turn_mode
            .as_deref()
            .map(str::trim)
            .filter(|mode| !mode.is_empty())
            .map(|mode| mode.to_ascii_lowercase());
        let turn_mode_is_followup = normalized_turn_mode.as_deref() == Some("followup");
        let turn_mode_is_query = normalized_turn_mode.as_deref() == Some("query");
        let force_wiki_search = tool_preferences
            .enabled_tools
            .as_ref()
            .is_some_and(|tools| {
                tools
                    .iter()
                    .any(|name| WIKI_OPTIONAL_TOOL_NAMES.contains(&name.as_str()))
            });
        if tool_preferences.closed_book {
            // Closed wins over every other preference, so no combination of
            // flags can reopen the vault or the network for such a turn.
            return Self {
                force_kb_search: false,
                force_web_search: false,
                web_fallback: false,
                force_wiki_search: false,
                deep_research_mode: false,
                force_followup_mode: tool_preferences.followup_mode || turn_mode_is_followup,
                closed_book: true,
            };
        }
        Self {
            force_kb_search: tool_preferences.knowledge_base || turn_mode_is_query,
            force_web_search: tool_preferences.web_search || turn_mode_is_query,
            web_fallback: false,
            force_wiki_search,
            deep_research_mode: tool_preferences.deep_research_mode,
            force_followup_mode: tool_preferences.followup_mode || turn_mode_is_followup,
            closed_book: false,
        }
    }

    /// Whether the web is in this turn's plan at all, asked for or as the
    /// intent step's fallback. Routing reads this; the gate after the vault
    /// search decides whether a fallback search actually runs.
    pub(super) fn web_planned(&self) -> bool {
        self.force_web_search || self.web_fallback
    }
}

/// What the intent step made of a turn.
pub(super) struct InferredIntent {
    pub(super) flags: SearchFlags,
    /// A follow-up as a question that stands alone, for the search.
    pub(super) standalone_question: Option<String>,
}

impl InferredIntent {
    /// The turn as requested, with nothing inferred.
    pub(super) fn as_requested(flags: SearchFlags) -> Self {
        Self {
            flags,
            standalone_question: None,
        }
    }
}

/// Ask the utility model what an unflagged turn needs, and fold the answer
/// into the search flags. Skipped the moment the user said anything explicit —
/// a toggle, a turn mode, a closed book — because inference exists to fill in
/// defaults, not to second-guess a decision.
pub(super) async fn infer_turn_intent(
    container: &dyn ChatRuntime,
    chat_llm: &Arc<dyn lattice_core::application::ports::LLMPort>,
    tool_preferences: Option<&ToolPreferences>,
    search_flags: SearchFlags,
    validated_message: &str,
    context: &[String],
    cancel: tokio_util::sync::CancellationToken,
) -> InferredIntent {
    if !should_infer_turn_intent(tool_preferences, search_flags)
        || is_trivial_conversational_message(validated_message)
    {
        return InferredIntent::as_requested(search_flags);
    }

    // Resolved the way the grounding judge below resolves its model: the
    // utility LLM when one is configured, the chat LLM otherwise, and no
    // classification at all when loading one fails.
    let classifier_llm = match (container.utility_llm_loader())().await {
        Ok(Some(utility)) => utility,
        Ok(None) => Arc::clone(chat_llm),
        Err(e) => {
            warn!(
                error = %e,
                "Utility LLM load failed — skipping turn-intent classification"
            );
            return InferredIntent::as_requested(search_flags);
        }
    };

    // As much recent history as the classifier's window gives it, newest
    // first, rather than a fixed number of turns.
    let classifier = IntentClassifier::new(classifier_llm).with_cancellation(cancel);
    let input = classifier.input_for(validated_message, context);
    let intent = classifier.classify(&input).await;

    let merged = apply_turn_intent(search_flags, &intent);
    // `web_fallback` is what the turn does with `needs_web`: the web is asked
    // only if the vault falls short, and the turn record says when it was.
    info!(
        needs_knowledge_base = intent.needs_knowledge_base,
        needs_web = intent.needs_web,
        is_followup = intent.is_followup,
        confidence = intent.confidence,
        web_fallback = merged.web_fallback,
        "chat_with_conversation: inferred turn intent"
    );
    InferredIntent {
        flags: merged,
        standalone_question: intent.standalone_question,
    }
}

/// Inference runs only when every routing control is on its default. Any
/// forced flag — including the ones `from_preferences` derives from an
/// explicit turn mode — means the user already decided this turn.
pub(super) fn should_infer_turn_intent(
    tool_preferences: Option<&ToolPreferences>,
    search_flags: SearchFlags,
) -> bool {
    if search_flags.closed_book
        || search_flags.force_kb_search
        || search_flags.force_web_search
        || search_flags.force_wiki_search
        || search_flags.force_followup_mode
        || search_flags.deep_research_mode
    {
        return false;
    }
    let turn_mode = tool_preferences
        .and_then(|preferences| preferences.turn_mode.as_deref())
        .map(str::trim)
        .filter(|mode| !mode.is_empty())
        .map(|mode| mode.to_ascii_lowercase());
    matches!(turn_mode.as_deref(), None | Some("auto"))
}

/// Pure greetings and polite acknowledgements with no subject matter do not
/// need a 570-token LLM round-trip before the chat turn can start.
fn is_trivial_conversational_message(message: &str) -> bool {
    let normalized: String = message
        .trim()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "hi" | "hello"
            | "hey"
            | "hi there"
            | "hello there"
            | "hey there"
            | "good morning"
            | "good afternoon"
            | "good evening"
            | "thanks"
            | "thank you"
            | "ty"
            | "ok"
            | "okay"
    )
}

/// Inference may add a vault search and follow-up mode, and never takes away
/// what the user or the focus scope asked for. It never forces the web:
/// `needs_web` becomes `web_fallback`, from this turn's intent alone, so a
/// question the user's files answer stays on the machine. A closed book stays
/// closed.
pub(super) fn apply_turn_intent(search_flags: SearchFlags, intent: &TurnIntent) -> SearchFlags {
    if search_flags.closed_book {
        return search_flags;
    }
    SearchFlags {
        force_kb_search: search_flags.force_kb_search || intent.needs_knowledge_base,
        web_fallback: intent.needs_web && !search_flags.force_web_search,
        force_followup_mode: search_flags.force_followup_mode || intent.is_followup,
        ..search_flags
    }
}

/// Routes the turn and keeps the router's decision, with its confidence and
/// rationale, for the turn record.
pub(super) async fn resolve_router_decision(
    container: &dyn ChatRuntime,
    conversation_document_context: &[lattice_core::domain::conversation::DocumentReference],
    validated_message: &str,
    search_flags: SearchFlags,
    source: Option<TurnSourcePolicy>,
    router_settings: &RouterSettingsDto,
    recorder: &TurnRecorder,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<(RouterDecisionOutcome, Option<TurnRouterDto>)> {
    // In an Explorer turn the question is about the folder on screen, so the
    // router's default "search the library" is not enough to start one.
    let router_may_search = source.is_none_or(|source| source.routes_to_library);
    // A closed-book turn has nothing to route: the pipeline returns before it
    // reads this, and asking the router model would only spend time. Nor does
    // an Explorer turn the user did not point at the library. A turn with the
    // web in its plan, asked for or as a fallback, searches the vault afresh
    // so the fallback has a verdict on the vault to go by.
    if search_flags.web_planned()
        || search_flags.closed_book
        || (!router_may_search && !search_flags.force_kb_search)
    {
        return Ok((
            RouterDecisionOutcome {
                action: RouterAction::NewSearch,
                recent_doc_meta: None,
                clarify_message: None,
                router_may_search,
            },
            None,
        ));
    }

    let recent_doc_meta =
        load_recent_document_metadata(container, conversation_document_context).await;
    let has_recent_document = recent_doc_meta.is_some();
    if conversation_document_context.is_empty() || !router_settings.enabled || !has_recent_document
    {
        return Ok((
            RouterDecisionOutcome {
                action: RouterAction::NewSearch,
                recent_doc_meta,
                clarify_message: None,
                router_may_search,
            },
            None,
        ));
    }

    let router_input = RouterInput {
        query: validated_message.to_string(),
        has_recent_document,
        recent_document_title: recent_doc_meta.as_ref().map(|m| m.title.clone()),
        recent_document_id: recent_doc_meta.as_ref().map(|m| m.document_id.clone()),
    };
    let step = recorder.begin(TurnStepKind::Route, "Deciding how to answer", None);
    let router_llm = container.get_or_load_router_llm().await?;
    let router = RouterService::new(router_llm, router_settings.clone()).with_cancellation(cancel);
    let decision = router.route(router_input).await;
    let action = router_action_code(&decision.action);
    recorder.end(
        &step,
        TurnStepState::Done,
        Some(format!(
            "{} · {:.0}% confident",
            router_action_label(&decision.action),
            decision.confidence * 100.0
        )),
    );

    Ok((
        RouterDecisionOutcome {
            action: decision.action,
            recent_doc_meta,
            clarify_message: decision.clarify_question,
            router_may_search,
        },
        Some(TurnRouterDto {
            action,
            confidence: decision.confidence,
            rationale: decision.rationale,
        }),
    ))
}

/// The stable code the UI and tests match on.
pub(super) fn router_action_code(action: &RouterAction) -> String {
    match action {
        RouterAction::UseLastDocument => "use_last_document",
        RouterAction::NewSearch => "new_search",
        RouterAction::Clarify => "clarify",
    }
    .to_string()
}

/// The same decision as a sentence, for the step's one line.
pub(super) fn router_action_label(action: &RouterAction) -> &'static str {
    match action {
        RouterAction::UseLastDocument => "Continuing with the last document",
        RouterAction::NewSearch => "Searching afresh",
        RouterAction::Clarify => "Asking what you mean",
    }
}
