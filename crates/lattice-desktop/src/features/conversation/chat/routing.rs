use super::intent::{TriageDocumentAction, DEFAULT_INTENT_CONFIDENCE_THRESHOLD};
use super::retrieval::RecentDocumentMetadata;
use super::*;
use lattice_core::application::ports::turn_source::TurnSourcePolicy;
use lattice_core::domain::conversation::DocumentReference;

pub(super) const SKIP_KB_CONFIDENCE_THRESHOLD: f32 = 0.75;

#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct SearchFlags {
    pub(super) force_kb_search: bool,
    /// The user explicitly asked for the library ("Your documents" toggle,
    /// Query turn mode, or pinned focus documents), distinct from an
    /// intent-inferred vault search.
    #[serde(default)]
    pub(super) user_requested_kb: bool,
    /// Answer strictly from retrieved sources only. When false (hybrid mode,
    /// the default), cite retrieved passages first and allow uncited general
    /// knowledge for general or conceptual gaps.
    #[serde(default)]
    pub(super) strict_sources: bool,
    /// High-confidence intent signal (or trivial greeting fast path) in hybrid
    /// auto mode indicating no vault search is needed for this turn.
    #[serde(default)]
    pub(super) skip_auto_kb: bool,
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
                user_requested_kb: false,
                strict_sources: false,
                skip_auto_kb: false,
                force_web_search: false,
                web_fallback: false,
                force_wiki_search: false,
                deep_research_mode: false,
                force_followup_mode: tool_preferences.followup_mode || turn_mode_is_followup,
                closed_book: true,
            };
        }
        let user_requested_kb = tool_preferences.knowledge_base || turn_mode_is_query;
        Self {
            force_kb_search: user_requested_kb,
            user_requested_kb,
            strict_sources: tool_preferences.strict_sources,
            skip_auto_kb: false,
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
    if !should_infer_turn_intent(tool_preferences, search_flags) {
        return InferredIntent::as_requested(search_flags);
    }
    if is_trivial_conversational_message(validated_message) {
        return InferredIntent::as_requested(SearchFlags {
            skip_auto_kb: !search_flags.strict_sources,
            ..search_flags
        });
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
        skip_auto_kb = merged.skip_auto_kb,
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
    let skip_auto_kb = !search_flags.strict_sources
        && !search_flags.force_kb_search
        && !search_flags.user_requested_kb
        && !search_flags.force_web_search
        && !search_flags.force_wiki_search
        && !search_flags.force_followup_mode
        && !search_flags.deep_research_mode
        && intent.confidence >= SKIP_KB_CONFIDENCE_THRESHOLD
        && !intent.needs_knowledge_base
        && !intent.needs_web
        && !intent.is_followup;
    SearchFlags {
        force_kb_search: search_flags.force_kb_search || intent.needs_knowledge_base,
        skip_auto_kb,
        web_fallback: intent.needs_web && !search_flags.force_web_search,
        force_followup_mode: search_flags.force_followup_mode || intent.is_followup,
        ..search_flags
    }
}

pub(super) struct TurnTriageOutcome {
    pub(super) inferred: InferredIntent,
    pub(super) decision: RouterDecisionOutcome,
    pub(super) record: Option<TurnRouterDto>,
}

pub(super) struct TurnTriageInput<'a> {
    pub(super) chat_llm: &'a Arc<dyn lattice_core::application::ports::LLMPort>,
    pub(super) tool_preferences: Option<&'a ToolPreferences>,
    pub(super) requested_flags: SearchFlags,
    pub(super) validated_message: &'a str,
    pub(super) context: &'a [String],
    pub(super) document_context: &'a [DocumentReference],
    pub(super) scope: &'a ScopeSnapshot,
    pub(super) source: Option<TurnSourcePolicy>,
    pub(super) router_settings: &'a RouterSettingsDto,
    pub(super) recorder: &'a TurnRecorder,
    pub(super) cancel: tokio_util::sync::CancellationToken,
}

/// Runs turn classification and recent-document routing together, using a
/// single unified utility-LLM call when possible and zero LLM calls on
/// deterministic fast paths.
pub(super) async fn triage_turn_with_scope(
    container: &dyn ChatRuntime,
    input: TurnTriageInput<'_>,
    metrics: &mut ConversationFlowTimingMetrics,
) -> Result<TurnTriageOutcome> {
    let TurnTriageInput {
        chat_llm,
        tool_preferences,
        requested_flags,
        validated_message,
        context,
        document_context,
        scope,
        source,
        router_settings,
        recorder,
        cancel,
    } = input;
    let router_may_search = source.is_none_or(|s| s.routes_to_library);
    let infers_intent = source.is_none_or(|s| s.infers_intent);

    if requested_flags.closed_book {
        return Ok(TurnTriageOutcome {
            inferred: InferredIntent::as_requested(requested_flags),
            decision: RouterDecisionOutcome {
                action: RouterAction::NewSearch,
                recent_doc_meta: None,
                clarify_message: None,
                router_may_search,
            },
            record: None,
        });
    }

    let recent_doc = if scope.resolved {
        scope.recent_document.clone()
    } else if !document_context.is_empty() {
        load_recent_document_metadata(container, document_context).await
    } else {
        None
    };

    if !infers_intent || !should_infer_turn_intent(tool_preferences, requested_flags) {
        if requested_flags.force_followup_mode
            && !requested_flags.force_kb_search
            && !requested_flags.web_planned()
            && recent_doc.is_some()
        {
            return Ok(TurnTriageOutcome {
                inferred: InferredIntent::as_requested(requested_flags),
                decision: RouterDecisionOutcome {
                    action: RouterAction::UseLastDocument,
                    recent_doc_meta: recent_doc,
                    clarify_message: None,
                    router_may_search,
                },
                record: None,
            });
        }
        let router_start = Instant::now();
        let (decision, record) = resolve_router_decision_with_recent(
            container,
            document_context,
            recent_doc,
            validated_message,
            requested_flags,
            source,
            router_settings,
            recorder,
            cancel,
        )
        .await?;
        metrics.router_ms = elapsed_ms(router_start);
        return Ok(TurnTriageOutcome {
            inferred: InferredIntent::as_requested(requested_flags),
            decision,
            record,
        });
    }

    if is_trivial_conversational_message(validated_message) {
        let flags = SearchFlags {
            skip_auto_kb: !requested_flags.strict_sources,
            ..requested_flags
        };
        if flags.skip_auto_kb
            || !router_settings.enabled
            || !router_may_search
            || recent_doc.is_none()
        {
            return Ok(TurnTriageOutcome {
                inferred: InferredIntent::as_requested(flags),
                decision: RouterDecisionOutcome {
                    action: RouterAction::NewSearch,
                    recent_doc_meta: None,
                    clarify_message: None,
                    router_may_search,
                },
                record: None,
            });
        }
        let router_start = Instant::now();
        let (decision, record) = resolve_router_decision_with_recent(
            container,
            document_context,
            recent_doc,
            validated_message,
            flags,
            source,
            router_settings,
            recorder,
            cancel,
        )
        .await?;
        metrics.router_ms = elapsed_ms(router_start);
        return Ok(TurnTriageOutcome {
            inferred: InferredIntent::as_requested(flags),
            decision,
            record,
        });
    }

    if requested_flags.strict_sources && scope.is_known_empty_vault() {
        return Ok(TurnTriageOutcome {
            inferred: InferredIntent::as_requested(requested_flags),
            decision: RouterDecisionOutcome {
                action: RouterAction::NewSearch,
                recent_doc_meta: None,
                clarify_message: None,
                router_may_search,
            },
            record: None,
        });
    }

    let can_route_recent_doc = router_may_search
        && router_settings.enabled
        && !document_context.is_empty()
        && recent_doc.is_some();
    let using_default_router_template =
        router_settings.prompt_template == RouterSettingsDto::default().prompt_template;

    if can_route_recent_doc && using_default_router_template {
        let router_start = Instant::now();
        let classifier_llm = match (container.utility_llm_loader())().await {
            Ok(Some(utility)) => utility,
            Ok(None) => Arc::clone(chat_llm),
            Err(e) => {
                warn!(
                    error = %e,
                    "Utility LLM load failed — skipping unified turn triage"
                );
                metrics.router_ms = elapsed_ms(router_start);
                return Ok(TurnTriageOutcome {
                    inferred: InferredIntent::as_requested(requested_flags),
                    decision: RouterDecisionOutcome {
                        action: RouterAction::NewSearch,
                        recent_doc_meta: recent_doc,
                        clarify_message: None,
                        router_may_search,
                    },
                    record: None,
                });
            }
        };
        let recent_title = recent_doc
            .as_ref()
            .map(|meta| meta.title.as_str())
            .unwrap_or_default();
        let classifier = IntentClassifier::new(classifier_llm).with_cancellation(cancel);
        let triage_input =
            classifier.input_for_recent_document(validated_message, context, recent_title);
        let triage = classifier
            .triage_with_recent_document(&triage_input, recent_title)
            .await;

        if triage.used_fallback {
            metrics.router_ms = elapsed_ms(router_start);
            return Ok(TurnTriageOutcome {
                inferred: InferredIntent::as_requested(requested_flags),
                decision: RouterDecisionOutcome {
                    action: RouterAction::NewSearch,
                    recent_doc_meta: recent_doc,
                    clarify_message: None,
                    router_may_search,
                },
                record: None,
            });
        }

        let mut merged_flags = apply_turn_intent(requested_flags, &triage.intent);
        if matches!(
            triage.document_action,
            TriageDocumentAction::UseLastDocument | TriageDocumentAction::Clarify
        ) && triage.intent.confidence >= DEFAULT_INTENT_CONFIDENCE_THRESHOLD
        {
            merged_flags.skip_auto_kb = false;
        } else if triage.document_action == TriageDocumentAction::None
            && !requested_flags.strict_sources
            && triage.intent.confidence >= SKIP_KB_CONFIDENCE_THRESHOLD
            && !triage.intent.needs_web
            && !triage.intent.needs_knowledge_base
        {
            merged_flags.skip_auto_kb = true;
        }

        info!(
            needs_knowledge_base = triage.intent.needs_knowledge_base,
            needs_web = triage.intent.needs_web,
            is_followup = triage.intent.is_followup,
            document_action = ?triage.document_action,
            confidence = triage.intent.confidence,
            skip_auto_kb = merged_flags.skip_auto_kb,
            web_fallback = merged_flags.web_fallback,
            "chat_with_conversation: unified turn triage"
        );

        if merged_flags.skip_auto_kb || merged_flags.web_planned() {
            metrics.router_ms = elapsed_ms(router_start);
            return Ok(TurnTriageOutcome {
                inferred: InferredIntent {
                    flags: merged_flags,
                    standalone_question: triage.intent.standalone_question,
                },
                decision: RouterDecisionOutcome {
                    action: RouterAction::NewSearch,
                    recent_doc_meta: if merged_flags.skip_auto_kb {
                        None
                    } else {
                        recent_doc
                    },
                    clarify_message: None,
                    router_may_search,
                },
                record: None,
            });
        }

        let mut action = match triage.document_action {
            TriageDocumentAction::UseLastDocument => RouterAction::UseLastDocument,
            TriageDocumentAction::Clarify => RouterAction::Clarify,
            TriageDocumentAction::NewSearch | TriageDocumentAction::None => RouterAction::NewSearch,
        };
        if triage.intent.confidence < router_settings.ambiguity_threshold {
            action = RouterAction::Clarify;
        }
        if matches!(
            action,
            RouterAction::UseLastDocument | RouterAction::Clarify
        ) && !requested_flags.user_requested_kb
        {
            merged_flags.force_kb_search = false;
        }
        let clarify_message = if action == RouterAction::Clarify {
            Some(
                triage
                    .clarify_question
                    .filter(|q| !q.trim().is_empty())
                    .unwrap_or_else(|| {
                        router_settings
                            .clarify_prompt_template
                            .replace("{recent_document_title}", recent_title)
                    }),
            )
        } else {
            None
        };
        let step = recorder.begin(TurnStepKind::Route, "Deciding how to answer", None);
        let action_code = router_action_code(&action);
        recorder.end(
            &step,
            TurnStepState::Done,
            Some(format!(
                "{} · {:.0}% confident",
                router_action_label(&action),
                triage.intent.confidence * 100.0
            )),
        );
        metrics.router_ms = elapsed_ms(router_start);
        return Ok(TurnTriageOutcome {
            inferred: InferredIntent {
                flags: merged_flags,
                standalone_question: triage.intent.standalone_question,
            },
            decision: RouterDecisionOutcome {
                action,
                recent_doc_meta: recent_doc,
                clarify_message,
                router_may_search,
            },
            record: Some(TurnRouterDto {
                action: action_code,
                confidence: triage.intent.confidence,
                rationale: triage.rationale,
            }),
        });
    }

    let inferred = infer_turn_intent(
        container,
        chat_llm,
        tool_preferences,
        requested_flags,
        validated_message,
        context,
        cancel.clone(),
    )
    .await;

    if !can_route_recent_doc {
        let recent_doc_meta = if inferred.flags.skip_auto_kb || inferred.flags.web_planned() {
            None
        } else {
            recent_doc
        };
        return Ok(TurnTriageOutcome {
            inferred,
            decision: RouterDecisionOutcome {
                action: RouterAction::NewSearch,
                recent_doc_meta,
                clarify_message: None,
                router_may_search,
            },
            record: None,
        });
    }

    let router_start = Instant::now();
    let flags = inferred.flags;
    let (decision, record) = resolve_router_decision_with_recent(
        container,
        document_context,
        recent_doc,
        validated_message,
        flags,
        source,
        router_settings,
        recorder,
        cancel,
    )
    .await?;
    metrics.router_ms = elapsed_ms(router_start);
    Ok(TurnTriageOutcome {
        inferred,
        decision,
        record,
    })
}

/// Routes the turn and keeps the router's decision, with its confidence and
/// rationale, for the turn record.
#[cfg(test)]
pub(super) async fn resolve_router_decision(
    container: &dyn ChatRuntime,
    conversation_document_context: &[DocumentReference],
    validated_message: &str,
    search_flags: SearchFlags,
    source: Option<TurnSourcePolicy>,
    router_settings: &RouterSettingsDto,
    recorder: &TurnRecorder,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<(RouterDecisionOutcome, Option<TurnRouterDto>)> {
    let router_may_search = source.is_none_or(|source| source.routes_to_library);
    if search_flags.web_planned()
        || search_flags.closed_book
        || search_flags.skip_auto_kb
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
    resolve_router_decision_with_recent(
        container,
        conversation_document_context,
        recent_doc_meta,
        validated_message,
        search_flags,
        source,
        router_settings,
        recorder,
        cancel,
    )
    .await
}

async fn resolve_router_decision_with_recent(
    container: &dyn ChatRuntime,
    conversation_document_context: &[DocumentReference],
    recent_doc_meta: Option<RecentDocumentMetadata>,
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
    // an Explorer turn the user did not point at the library, or a turn with
    // `skip_auto_kb` (e.g. trivial greetings or high-confidence general-knowledge
    // turns in Hybrid Auto) — skipping routing here neither loads the router
    // model nor overrides `skip_auto_kb` with `UseLastDocument`/`Clarify`. A
    // turn with the web in its plan, asked for or as a fallback, searches the
    // vault afresh so the fallback has a verdict on the vault to go by.
    if search_flags.web_planned()
        || search_flags.closed_book
        || search_flags.skip_auto_kb
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::conversation::chat::test_runtime::{FakeRuntime, Scripted};
    use async_trait::async_trait;
    use lattice_core::application::ports::document_repository_port::DocumentRepositoryPort;
    use lattice_core::application::ports::llm_port::CompletionResponse;
    use lattice_core::application::ports::repository_port::RepositoryPort;
    use lattice_core::domain::conversation::DocumentReference;
    use lattice_core::domain::entities::document::{
        Category, DocumentStatus, Language, StoredDocument,
    };
    use lattice_core::domain::entities::Document;
    use lattice_core::domain::value_objects::checksum::Checksum;
    use lattice_core::shared::types::{DocumentId, ValidatedFilePath};

    struct OneDocRepo(Document);

    #[async_trait]
    impl RepositoryPort<Document> for OneDocRepo {
        async fn find_by_id(&self, id: &str) -> Result<Option<Document>> {
            Ok((self.0.id().to_string() == id).then(|| self.0.clone()))
        }
        async fn find_all(&self) -> Result<Vec<Document>> {
            Ok(vec![self.0.clone()])
        }
        async fn save(&self, _entity: &Document) -> Result<()> {
            Ok(())
        }
        async fn save_batch(&self, _entities: &[Document]) -> Result<()> {
            Ok(())
        }
        async fn delete(&self, _id: &str) -> Result<()> {
            Ok(())
        }
        async fn delete_batch(&self, _ids: &[&str]) -> Result<()> {
            Ok(())
        }
        async fn count(&self) -> Result<usize> {
            Ok(1)
        }
        async fn exists(&self, id: &str) -> Result<bool> {
            Ok(self.0.id().to_string() == id)
        }
    }

    #[async_trait]
    impl DocumentRepositoryPort for OneDocRepo {
        async fn find_file_path_by_id(&self, _document_id: &str) -> Result<String> {
            Ok(self.0.file_path().to_string_lossy().into_owned())
        }
        async fn document_exists(&self, document_id: &str) -> Result<bool> {
            Ok(self.0.id().to_string() == document_id)
        }
        async fn find_id_by_path(&self, _file_path: &str) -> Result<Option<String>> {
            Ok(None)
        }
        async fn delete(&self, _document_id: &str) -> Result<()> {
            Ok(())
        }
        async fn delete_and_release_blob(&self, _document_id: &str) -> Result<()> {
            Ok(())
        }
        async fn find_by_checksum(&self, _checksum: &Checksum) -> Result<Option<Document>> {
            Ok(None)
        }
        async fn count_documents(&self) -> Result<i64> {
            Ok(1)
        }
        async fn count_chunks(&self) -> Result<i64> {
            Ok(0)
        }
        async fn find_all_paginated(
            &self,
            _page: lattice_core::shared::persistence::Page,
        ) -> Result<Vec<Document>> {
            Ok(vec![self.0.clone()])
        }
    }

    impl lattice_core::application::ports::DocumentRepository for OneDocRepo {}

    fn recent_doc(id: &str, title: &str) -> Document {
        let now = chrono::Utc::now();
        Document::with_id(StoredDocument {
            id: DocumentId::from_string(id.to_string()).unwrap(),
            file_path: ValidatedFilePath::new(std::path::PathBuf::from(format!("/vault/{title}")))
                .unwrap(),
            file_name: title.to_string(),
            file_type: Some("md".to_string()),
            mime_type: "text/markdown".to_string(),
            size_bytes: 128,
            modified_at: now,
            indexed_at: now,
            checksum: Checksum::new("a".repeat(64)).unwrap(),
            status: DocumentStatus::Indexed,
            error_message: None,
            language: Language::English,
            category: Category::Notes,
            quality_score: 1.0,
            access_count: 0,
            last_accessed_at: None,
            word_count: 20,
            content: "Notes".to_string(),
        })
    }

    #[tokio::test]
    async fn resolve_router_decision_skips_router_model_when_skip_auto_kb_is_set() {
        let doc = recent_doc("doc-1", "ProjectNotes.md");
        let refs = vec![DocumentReference {
            document_id: "doc-1".to_string(),
            chunk_id: None,
            relevance_score: Some(0.9),
            added_at: chrono::Utc::now(),
        }];
        let router_settings = RouterSettingsDto {
            enabled: true,
            ..RouterSettingsDto::default()
        };
        let cancel = tokio_util::sync::CancellationToken::new();

        // Control: with skip_auto_kb = false and a recent document, the router model is loaded.
        let control_runtime = FakeRuntime {
            documents: Some(Arc::new(OneDocRepo(doc.clone()))),
            router_llm: Some(Scripted::new([CompletionResponse::from_text(
                r#"{"action":"use_last_document","confidence":0.95,"rationale":"follow-up"}"#,
            )])),
            ..FakeRuntime::default()
        };
        let control_recorder = TurnRecorder::silent("conv-1", "req-control");
        let (control_outcome, control_record) = resolve_router_decision(
            &control_runtime,
            &refs,
            "tell me more about that",
            SearchFlags::default(),
            None,
            &router_settings,
            &control_recorder,
            cancel.clone(),
        )
        .await
        .unwrap();
        assert_eq!(control_runtime.loader_counts.router_llm_calls(), 1);
        assert_eq!(control_outcome.action, RouterAction::UseLastDocument);
        assert!(control_record.is_some());

        // Under test: with skip_auto_kb = true, the router model is never loaded and
        // the decision stays NewSearch with no TurnRouterDto record.
        let skipped_runtime = FakeRuntime {
            documents: Some(Arc::new(OneDocRepo(doc))),
            router_llm: Some(Scripted::new([CompletionResponse::from_text(
                r#"{"action":"use_last_document","confidence":0.95,"rationale":"follow-up"}"#,
            )])),
            ..FakeRuntime::default()
        };
        let skipped_recorder = TurnRecorder::silent("conv-1", "req-skip");
        let (skipped_outcome, skipped_record) = resolve_router_decision(
            &skipped_runtime,
            &refs,
            "hello!",
            SearchFlags {
                skip_auto_kb: true,
                ..SearchFlags::default()
            },
            None,
            &router_settings,
            &skipped_recorder,
            cancel,
        )
        .await
        .unwrap();
        assert_eq!(skipped_runtime.loader_counts.router_llm_calls(), 0);
        assert_eq!(skipped_outcome.action, RouterAction::NewSearch);
        assert!(skipped_outcome.recent_doc_meta.is_none());
        assert!(skipped_record.is_none());
    }
}
