use super::*;
use crate::features::conversation::chat::cancellation::is_cancel_requested;
use std::time::Duration;

use crate::domain::qa::hyde::HyDEInterpretation;

fn overlap_ratio(a: &HashSet<String>, b: &HashSet<String>) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let overlap = a.intersection(b).count() as f32;
    overlap / (b.len() as f32)
}

fn dominant_result_terms(
    results: &[crate::features::function_calling::dto::WebSearchResult],
    limit: usize,
) -> (Vec<(String, usize)>, usize) {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut total = 0usize;

    for result in results {
        let combined = format!("{} {}", result.title, result.snippet);
        for token in tokenize_keyword_terms(&combined)
            .into_iter()
            .filter(|t| t.len() >= 4)
        {
            *counts.entry(token).or_insert(0) += 1;
            total += 1;
        }
    }

    let mut ranked: Vec<(String, usize)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    (ranked.into_iter().take(limit).collect(), total)
}

/// Retrieval for one turn.
///
/// Cancellation is checked at the phase boundaries rather than by racing the
/// phases themselves: an external search or a corrective retry dropped
/// mid-await would abandon work the pipeline cannot clean up. A cancelled turn
/// returns whatever the outcome holds so far, and the caller — which re-checks
/// the flag the moment this returns — turns that into the cancellation error
/// without ever reading it.
// This orchestration boundary exposes the complete per-turn retrieval configuration.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_retrieval_pipeline(
    container: &Container,
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conversation_id: &str,
    request_id: &str,
    validated_message: &str,
    llm: &Arc<dyn crate::application::ports::LLMPort>,
    router_settings: &RouterSettingsDto,
    router_decision: &RouterDecisionOutcome,
    search_flags: SearchFlags,
    conversation_document_context: &[crate::domain::conversation::DocumentReference],
    highlight_terms: &[String],
    context: &[String],
    max_tokens: usize,
    tool_output_settings: &ToolOutputSettingsDto,
    search_settings: &SearchSettingsDto,
) -> RetrievalPipelineOutcome {
    let retrieval_start = Instant::now();

    // Held until `outcome` exists below. This is the single honest source for
    // the "answered without your documents" line: the frontend must never
    // infer it.
    let mut embedding_unavailable_reason: Option<String> = None;
    if let Err(e) = container.get_or_load_embedding().await {
        debug!(error = %e, "Embedding model not available for RAG — search will be skipped");
        embedding_unavailable_reason = Some("the embedding model is not ready".to_string());
    }

    let utility_llm: Arc<dyn crate::application::ports::LLMPort> =
        match container.get_or_load_utility_llm().await {
            Ok(Some(util)) => {
                tracing::debug!("Using configured utility LLM for HyDE/query-rewrite");
                util
            }
            Ok(None) => {
                tracing::debug!(
                    "No utility LLM configured — external search rewriting may use the chat LLM \
                 (set one in Settings → Model Catalog to speed up retrieval)"
                );
                Arc::clone(llm)
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Utility LLM load failed — external search rewriting may use the chat LLM"
                );
                Arc::clone(llm)
            }
        };

    const RESPONSE_TOKEN_BUDGET_RATIO: f64 = 0.25;
    const PROMPT_OVERHEAD_TOKENS: usize = 200;

    let response_budget = (max_tokens as f64 * RESPONSE_TOKEN_BUDGET_RATIO) as usize;
    let question_tokens = llm.count_tokens(validated_message);
    let context_history_tokens: usize = context.iter().map(|c| llm.count_tokens(c)).sum();
    let available_for_rag = max_tokens
        .saturating_sub(response_budget)
        .saturating_sub(PROMPT_OVERHEAD_TOKENS)
        .saturating_sub(question_tokens)
        .saturating_sub(context_history_tokens);

    let mut outcome = RetrievalPipelineOutcome {
        short_circuit_response: None,
        interpretation: HyDEInterpretation::raw_only(
            validated_message.to_string(),
            QueryType::Question,
        ),
        search_response: crate::features::search::dto::SearchResponseDto {
            results: vec![],
            total: 0,
            query_time_ms: 0,
        },
        followup_context: None,
        web_context: None,
        web_search_error: None,
        kb_unavailable_reason: None,
        sources: Vec::new(),
        available_for_rag,
        sub_timings: RetrievalSubTimingMetrics::default(),
        searched_documents: 0,
        scope_is_linked: false,
        sufficiency: None,
    };
    let tuning = &search_settings.retrieval_tuning;
    // The embedding and utility models may both have been cold.
    if is_cancel_requested(request_id) {
        return outcome;
    }

    let mut retrieval_plan = RetrievalPlan::from_router(
        router_settings,
        router_decision,
        validated_message,
        search_flags,
    );
    let mut kb_attempted = false;
    let mut kb_has_results = false;
    let mut kb_low_confidence = true;
    outcome.short_circuit_response = retrieval_plan.short_circuit_response.take();

    if outcome.short_circuit_response.is_none()
        && retrieval_plan.route_action == RouterAction::UseLastDocument
    {
        outcome.followup_context = build_followup_context(
            container,
            conv_service,
            conversation_id,
            conversation_document_context,
            highlight_terms,
            available_for_rag,
            llm,
            tool_output_settings.excerpt_chars as usize,
        )
        .await;

        retrieval_plan.enable_kb_fallback(
            outcome.followup_context.is_none() && !search_flags.force_web_search,
        );
    }

    let external = ExternalLookup {
        container,
        conv_service,
        conversation_id,
        request_id,
        validated_message,
        utility_llm: &utility_llm,
        search_flags,
        highlight_terms,
        excerpt_chars: tool_output_settings.excerpt_chars as usize,
        tuning,
        wiki_planned: retrieval_plan.should_search_wiki,
        web_planned: retrieval_plan.should_search_web,
    };
    let kb_search = async {
        let kb_retrieval_start = Instant::now();
        let kb_search_limit = derive_kb_search_limit(tool_output_settings, tuning);
        let kb_outcome = run_kb_retrieval(
            container,
            conv_service,
            conversation_id,
            validated_message,
            llm,
            search_flags,
            highlight_terms,
            kb_search_limit,
            search_settings.enable_reranking,
            search_settings.similarity_threshold,
            tuning,
        )
        .await;
        (kb_outcome, elapsed_ms(kb_retrieval_start))
    };

    // The phase below is the long one — KB retrieval with its corrective
    // retries, and wiki/web lookups over the network.
    if is_cancel_requested(request_id) {
        return outcome;
    }

    let run_kb = outcome.short_circuit_response.is_none() && retrieval_plan.should_search_kb;
    // KB and the external phase overlap only when no KB result can close the
    // external-lookup gate. The low-confidence clarify below needs no wiki or
    // web search planned, so it never applies to a turn that takes this path.
    let kb_alongside_external =
        run_kb && external.is_planned() && kb_can_run_alongside_external(search_flags);
    let mut external_phase: Option<ExternalPhaseOutcome> = None;
    let kb_result = if kb_alongside_external {
        // The external phase interprets the turn itself instead of waiting on
        // KB's interpretation, so it starts from the raw message. `allow_lookup`
        // is true on the promise of `kb_can_run_alongside_external`: no KB
        // result can close the gate on a turn that takes this path. The merge
        // below re-checks the gate and complains if that promise ever breaks.
        let external_base = outcome.interpretation.clone();
        let interpret = external_base.hyde_text.is_none();
        let (kb_result, phase) =
            tokio::join!(kb_search, external.run(&external_base, interpret, true));
        external_phase = Some(phase);
        Some(kb_result)
    } else if run_kb {
        Some(kb_search.await)
    } else {
        None
    };

    if let Some((kb_outcome, kb_total_ms)) = kb_result {
        kb_attempted = true;
        outcome.interpretation = kb_outcome.interpretation;
        outcome.search_response = kb_outcome.search_response;
        outcome.sources = kb_outcome.sources;
        outcome.kb_unavailable_reason = kb_outcome.kb_unavailable_reason;
        outcome.sub_timings = kb_outcome.timings;
        outcome.searched_documents = kb_outcome.searched_documents;
        outcome.scope_is_linked = kb_outcome.scope_is_linked;
        outcome.sufficiency = kb_outcome.sufficiency;
        outcome.sub_timings.kb_total_ms = kb_total_ms;
        kb_has_results = !outcome.search_response.results.is_empty();
        kb_low_confidence = kb_outcome.low_confidence;

        if kb_outcome.low_confidence
            && !retrieval_plan.should_search_web
            && !retrieval_plan.should_search_wiki
            && outcome.short_circuit_response.is_none()
            && router_decision.recent_doc_meta.is_some()
        {
            outcome.short_circuit_response = Some(build_router_clarify_response(
                router_settings,
                &router_decision.recent_doc_meta,
                validated_message,
                None,
            ));
            outcome.search_response = empty_search_response();
            outcome.sources.clear();
            info!(
                conversation_id = conversation_id,
                "Low-confidence retrieval triggered clarify fallback"
            );
        }
    }

    // Only when the turn really did answer without the vault. Keyword search
    // can still return passages with the embedder down, and telling the reader
    // "answered without your documents" while citing three of them is worse
    // than saying nothing.
    if let Some(reason) = embedding_unavailable_reason {
        if outcome.sources.is_empty() {
            outcome.kb_unavailable_reason.get_or_insert(reason);
        }
    }

    let should_use_external_as_fallback =
        should_use_external_as_fallback(search_flags, &outcome.interpretation);
    let allow_external_lookup = should_execute_external_lookup(
        should_use_external_as_fallback,
        kb_attempted,
        kb_has_results,
        kb_low_confidence,
    );
    if (retrieval_plan.should_search_web || retrieval_plan.should_search_wiki)
        && !allow_external_lookup
    {
        info!(
            conversation_id = conversation_id,
            kb_attempted = kb_attempted,
            kb_has_results = kb_has_results,
            kb_low_confidence = kb_low_confidence,
            "Skipping external search because KB is sufficient for this turn"
        );
    }

    if is_cancel_requested(request_id) {
        return outcome;
    }

    if external_phase.is_none() && outcome.short_circuit_response.is_none() && external.is_planned()
    {
        let interpret = outcome.interpretation.hyde_text.is_none();
        external_phase = Some(
            external
                .run(&outcome.interpretation, interpret, allow_external_lookup)
                .await,
        );
    }

    if let Some(phase) = external_phase {
        outcome.sub_timings.external_hyde_interpretation_ms = phase.query_prep_ms;
        // The utility model's interpretation replaces KB's unless KB produced
        // HyDE text of its own, which is when the sequential order skipped the
        // external interpretation. KB retrieval never does today, so both
        // orders hand chat the external interpretation whenever it succeeded.
        if outcome.interpretation.hyde_text.is_none() {
            if let Some(interpretation) = phase.interpretation {
                outcome.interpretation = interpretation;
            }
        }
        // Rechecked because a concurrent phase started before KB's
        // interpretation existed. Wiki before web whatever finished first, so
        // the merged sources and context match the sequential order.
        let fetched_anything = phase.wiki.is_some() || phase.web.is_some();
        debug_assert!(
            allow_external_lookup || !fetched_anything,
            "a concurrent external phase fetched results the gate then closed on"
        );
        if !allow_external_lookup && fetched_anything {
            warn!(
                conversation_id = conversation_id,
                "Discarding external results the gate closed on after they were fetched"
            );
        }
        // The short-circuit answer is the whole turn: merging searches onto it
        // would cite sources the reply never used.
        if allow_external_lookup && outcome.short_circuit_response.is_none() {
            if let Some(wiki) = phase.wiki {
                attach_wiki_results(&mut outcome, wiki);
            }
            if let Some(web) = phase.web {
                attach_web_results(&mut outcome, web);
            }
        }
    }
    outcome.sub_timings.total_ms = elapsed_ms(retrieval_start);
    info!(
        kb_total_ms = outcome.sub_timings.kb_total_ms,
        kb_scope_load_ms = outcome.sub_timings.kb_scope_load_ms,
        kb_hyde_interpretation_ms = outcome.sub_timings.kb_hyde_interpretation_ms,
        kb_search_plan_ms = outcome.sub_timings.kb_search_plan_ms,
        kb_shortlist_planning_ms = outcome.sub_timings.kb_shortlist_planning_ms,
        kb_query_execution_ms = outcome.sub_timings.kb_query_execution_ms,
        kb_merge_shortlist_gate_ms = outcome.sub_timings.kb_merge_shortlist_gate_ms,
        kb_post_filters_ms = outcome.sub_timings.kb_post_filters_ms,
        kb_rerank_ms = outcome.sub_timings.kb_rerank_ms,
        kb_sufficiency_ms = outcome.sub_timings.kb_sufficiency_ms,
        kb_corrective_retry_ms = outcome.sub_timings.kb_corrective_retry_ms,
        kb_corrective_retries = outcome.sub_timings.kb_corrective_retries,
        kb_sufficient = ?outcome.sub_timings.kb_sufficient,
        kb_planner_skipped = outcome.sub_timings.kb_planner_skipped,
        kb_build_sources_ms = outcome.sub_timings.kb_build_sources_ms,
        kb_persist_references_ms = outcome.sub_timings.kb_persist_references_ms,
        external_hyde_interpretation_ms = outcome.sub_timings.external_hyde_interpretation_ms,
        wiki_search_ms = outcome.sub_timings.wiki_search_ms,
        web_search_ms = outcome.sub_timings.web_search_ms,
        kb_alongside_external = kb_alongside_external,
        total_ms = outcome.sub_timings.total_ms,
        "retrieval: sub timing metrics"
    );
    outcome
}

/// Borrowed inputs of the wiki/web phase, shared so the phase can run beside
/// KB retrieval without copying turn state.
/// Below this many characters of extracted text, a page is not carrying an
/// article — it is a cookie wall, a paywall stub or pure navigation — and the
/// search snippet says as much in less space.
const MIN_USEFUL_PAGE_CHARS: usize = 400;

/// One web result's page, read in full.
struct FetchedPage {
    text: String,
    word_count: usize,
    truncated: bool,
}

struct ExternalLookup<'a> {
    container: &'a Container,
    conv_service: &'a Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conversation_id: &'a str,
    request_id: &'a str,
    validated_message: &'a str,
    utility_llm: &'a Arc<dyn crate::application::ports::LLMPort>,
    search_flags: SearchFlags,
    highlight_terms: &'a [String],
    excerpt_chars: usize,
    tuning: &'a RetrievalTuningSettingsDto,
    wiki_planned: bool,
    web_planned: bool,
}

/// What the wiki/web phase produced, merged into the outcome by the caller.
struct ExternalPhaseOutcome {
    /// The utility model's reading of the turn. `None` when it was not
    /// requested or failed.
    interpretation: Option<HyDEInterpretation>,
    /// Wall-clock of query preparation, during which the interpretation and
    /// the web-query rewrite run concurrently.
    query_prep_ms: u64,
    wiki: Option<ExternalSearchResult>,
    web: Option<ExternalSearchResult>,
}

/// One external search, ready to merge into the outcome.
#[derive(Debug, Default)]
pub(super) struct ExternalSearchResult {
    pub(super) sources: Vec<SourceDto>,
    /// Prompt context. `None` when the search returned nothing usable.
    pub(super) context: Option<String>,
    /// Surfaced to the prompt. Web search only: wiki failures are just logged.
    pub(super) error: Option<String>,
    pub(super) elapsed_ms: u64,
}

impl ExternalLookup<'_> {
    fn is_planned(&self) -> bool {
        self.wiki_planned || self.web_planned
    }

    /// Interpret the turn and rewrite the web query side by side, then run the
    /// wiki and web searches side by side. `allow_lookup` false still runs the
    /// interpretation, as the sequential pipeline always did.
    async fn run(
        &self,
        base_interpretation: &HyDEInterpretation,
        interpret: bool,
        allow_lookup: bool,
    ) -> ExternalPhaseOutcome {
        let prep_start = Instant::now();
        let search_wiki = self.wiki_planned && allow_lookup;
        let search_web = self.web_planned && allow_lookup;
        // HyDE and web-query rewriting run on the utility LLM (small, fast,
        // local) when set, not the chat LLM. See utility_llm resolution above.
        let hyde_service =
            crate::features::qa::hyde::HyDEService::new(Arc::clone(self.utility_llm));
        let hyde_context = if interpret || search_web {
            build_hyde_context_window_for_conversation(self.conv_service, self.conversation_id)
                .await
        } else {
            None
        };
        let (interpretation, generated_web_query) = tokio::join!(
            async {
                if !interpret {
                    return None;
                }
                hyde_service
                    .interpret_query_with_context(self.validated_message, hyde_context.as_deref())
                    .await
                    .ok()
            },
            async {
                if !search_web {
                    return None;
                }
                Some(
                    hyde_service
                        .generate_web_search_query_with_context(
                            self.validated_message,
                            hyde_context.as_deref(),
                        )
                        .await,
                )
            },
        );
        let effective_interpretation = interpretation.as_ref().unwrap_or(base_interpretation);

        let followup_anchor_terms = if (search_wiki || search_web)
            && (self.search_flags.force_followup_mode
                || matches!(effective_interpretation.query_type, QueryType::Followup))
        {
            let terms =
                load_followup_turn_anchor_terms(self.conv_service, self.conversation_id).await;
            if terms.is_empty() {
                None
            } else {
                Some(terms)
            }
        } else {
            None
        };
        let web_query = generated_web_query.map(|generated| {
            self.choose_web_query(
                generated,
                effective_interpretation,
                followup_anchor_terms.as_ref(),
            )
        });
        let query_prep_ms = elapsed_ms(prep_start);

        let (wiki, web) = tokio::join!(
            async {
                if !search_wiki {
                    return None;
                }
                Some(
                    self.wiki_search(effective_interpretation, followup_anchor_terms.as_ref())
                        .await,
                )
            },
            async {
                let query = web_query.as_deref()?;
                Some(self.web_search(query).await)
            },
        );

        ExternalPhaseOutcome {
            interpretation,
            query_prep_ms,
            wiki,
            web,
        }
    }

    /// The rewritten web query, or the lexical query when the rewrite came
    /// back empty or failed.
    fn choose_web_query(
        &self,
        generated: Result<String>,
        interpretation: &HyDEInterpretation,
        followup_anchor_terms: Option<&HashSet<String>>,
    ) -> String {
        let validated_message = self.validated_message;
        let tuning = self.tuning;
        let lexical_web_query = select_web_search_query_with_tuning(
            validated_message,
            interpretation,
            followup_anchor_terms,
            tuning,
        );
        let mut generated_web_query: Option<String> = None;
        let mut web_query_source = "hyde_generated";
        let web_query = match generated {
            Ok(query) if !query.trim().is_empty() => {
                generated_web_query = Some(query.trim().to_string());
                safe_truncate(
                    query.trim(),
                    tuning.external_search_query_max_chars as usize,
                )
            }
            Ok(_) => {
                web_query_source = "lexical_fallback_empty_hyde";
                lexical_web_query.clone()
            }
            Err(error) => {
                warn!(
                    error = %error,
                    "Web-specific HyDE query generation failed; falling back to lexical web query builder"
                );
                web_query_source = "lexical_fallback_hyde_error";
                lexical_web_query.clone()
            }
        };
        let raw_terms = tokenize_keyword_terms(validated_message)
            .into_iter()
            .collect::<HashSet<_>>();
        let hyde_terms = interpretation
            .hyde_text
            .as_deref()
            .map(tokenize_keyword_terms)
            .unwrap_or_default()
            .into_iter()
            .collect::<HashSet<_>>();
        let generated_terms = generated_web_query
            .as_deref()
            .map(tokenize_keyword_terms)
            .unwrap_or_default()
            .into_iter()
            .collect::<HashSet<_>>();
        let lexical_terms = tokenize_keyword_terms(&lexical_web_query)
            .into_iter()
            .collect::<HashSet<_>>();
        let final_terms = tokenize_keyword_terms(&web_query)
            .into_iter()
            .collect::<HashSet<_>>();

        debug!(
            source = web_query_source,
            raw_query_preview = %safe_truncate(validated_message, 160),
            hyde_expansion_preview = %safe_truncate(
                interpretation.hyde_text.as_deref().unwrap_or(""),
                180
            ),
            generated_query_preview = %safe_truncate(generated_web_query.as_deref().unwrap_or(""), 180),
            lexical_query_preview = %safe_truncate(&lexical_web_query, 180),
            final_query_preview = %safe_truncate(&web_query, 180),
            generated_term_count = generated_terms.len(),
            lexical_term_count = lexical_terms.len(),
            final_term_count = final_terms.len(),
            generated_overlap_with_hyde = overlap_ratio(&generated_terms, &hyde_terms),
            lexical_overlap_with_hyde = overlap_ratio(&lexical_terms, &hyde_terms),
            final_overlap_with_hyde = overlap_ratio(&final_terms, &hyde_terms),
            final_overlap_with_raw = overlap_ratio(&final_terms, &raw_terms),
            "Web query selection diagnostics"
        );
        web_query
    }

    async fn wiki_search(
        &self,
        interpretation: &HyDEInterpretation,
        followup_anchor_terms: Option<&HashSet<String>>,
    ) -> ExternalSearchResult {
        let wiki_search_start = Instant::now();
        let tuning = self.tuning;
        let mut searched = ExternalSearchResult::default();
        let wiki_query = select_wiki_search_query_with_tuning(
            self.validated_message,
            interpretation,
            followup_anchor_terms,
            tuning,
        );

        let executor = self.container.function_executor();
        let call = crate::features::function_calling::domain::FunctionCall::new(
            uuid::Uuid::new_v4().to_string(),
            "wiki_search",
            serde_json::json!({
                "query": wiki_query,
                "max_results": tuning.wiki_search_max_results
            }),
        );

        match executor.execute(call).await {
            Ok(result) if result.success => {
                if let Some(data) = result.data {
                    match serde_json::from_value::<WikiSearchOutput>(data) {
                        Ok(output) => {
                            info!(
                                result_count = output.result_count,
                                "Forced wiki search completed"
                            );
                            if !output.results.is_empty() {
                                let web_like_results = output
                                    .results
                                    .iter()
                                    .map(|item| WebSearchResult {
                                        title: item.title.clone(),
                                        url: item.url.clone(),
                                        snippet: item.snippet.clone(),
                                        published_date: None,
                                        source: Some("wikipedia".to_string()),
                                    })
                                    .collect::<Vec<_>>();

                                searched.sources = build_web_source_citations(
                                    &web_like_results,
                                    self.highlight_terms,
                                    self.excerpt_chars,
                                );

                                let wiki_context = output
                                    .results
                                    .iter()
                                    .take(tuning.wiki_context_limit as usize)
                                    .enumerate()
                                    .map(|(idx, item)| {
                                        let snippet = safe_truncate(
                                            &item.snippet,
                                            tuning.wiki_snippet_max_chars as usize,
                                        );
                                        format!(
                                            "[{}] {}\nURL: {}\nSnippet: {}",
                                            idx + 1,
                                            item.title,
                                            item.url,
                                            snippet
                                        )
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n\n");
                                if !wiki_context.trim().is_empty() {
                                    searched.context = Some(wiki_context);
                                }
                            }
                        }
                        Err(error) => {
                            warn!(error = %error, "Failed to parse wiki search output");
                        }
                    }
                }
            }
            Ok(result) => {
                let reason = result
                    .error_message
                    .clone()
                    .unwrap_or_else(|| "Wiki search tool execution failed".to_string());
                warn!(error = reason.as_str(), "Wiki search failed");
            }
            Err(error) => {
                warn!(error = %error, "Wiki search failed");
            }
        }
        searched.elapsed_ms = elapsed_ms(wiki_search_start);
        searched
    }

    /// Open the top web results and read them.
    ///
    /// A search provider returns a headline and about a sentence of context.
    /// Answering "what does this page say" from that is not possible, so the
    /// highest-ranked results are fetched and their article text is what the
    /// prompt carries. Fetches run side by side — each one sleeps a randomised
    /// moment before its request, so running them in sequence would add that
    /// delay per page — and every one is individually fallible: a timeout, a
    /// paywall or a page with no extractable article yields `None` for that
    /// slot and the caller falls back to the snippet. The returned vector is
    /// index-aligned with `results`.
    async fn fetch_page_texts(
        &self,
        results: &[crate::features::function_calling::dto::WebSearchResult],
    ) -> Vec<Option<FetchedPage>> {
        let mut pages: Vec<Option<FetchedPage>> = (0..results.len()).map(|_| None).collect();

        let count = (self.tuning.web_fetch_page_count as usize).min(results.len());
        if count == 0 {
            return pages;
        }
        // Reading pages is the slowest thing the external phase does. A stop
        // press that landed during the search itself should not buy the user
        // another round of network fetches.
        if is_cancel_requested(self.request_id) {
            debug!("Turn cancelled before page fetching — keeping snippets only");
            return pages;
        }

        let started = Instant::now();
        let executor = self.container.function_executor();
        let timeout = Duration::from_secs(self.tuning.web_page_fetch_timeout_secs.max(1) as u64);
        let max_chars = self.tuning.web_page_max_chars as usize;

        let fetched = futures::future::join_all(results.iter().take(count).map(|result| {
            let url = result.url.clone();
            let executor = executor.clone();
            async move {
                let call = crate::features::function_calling::domain::FunctionCall::new(
                    uuid::Uuid::new_v4().to_string(),
                    "fetch_url_content",
                    serde_json::json!({ "url": url }),
                );
                match tokio::time::timeout(timeout, executor.execute(call)).await {
                    Ok(Ok(outcome)) if outcome.success => outcome.data,
                    Ok(Ok(outcome)) => {
                        debug!(
                            url = url.as_str(),
                            error = outcome.error_message.unwrap_or_default().as_str(),
                            "Page fetch returned no content — falling back to snippet"
                        );
                        None
                    }
                    Ok(Err(e)) => {
                        debug!(url = url.as_str(), error = %e, "Page fetch failed — falling back to snippet");
                        None
                    }
                    Err(_) => {
                        debug!(
                            url = url.as_str(),
                            timeout_secs = timeout.as_secs(),
                            "Page fetch timed out — falling back to snippet"
                        );
                        None
                    }
                }
            }
        }))
        .await;

        let mut fetched_count = 0usize;
        let mut total_words = 0usize;
        for (slot, data) in fetched.into_iter().enumerate() {
            let Some(data) = data else { continue };
            let Ok(output) = serde_json::from_value::<
                crate::features::function_calling::dto::FetchUrlContentOutput,
            >(data) else {
                continue;
            };
            // Extraction can succeed on a page that is all navigation. Below
            // this the snippet is as informative and shorter.
            if output.content.trim().len() < MIN_USEFUL_PAGE_CHARS {
                debug!(
                    url = output.url.as_str(),
                    chars = output.content.trim().len(),
                    "Extracted page text too thin to be worth carrying — keeping snippet"
                );
                continue;
            }
            let text = safe_truncate(output.content.trim(), max_chars);
            let truncated = output.content_truncated || text.len() < output.content.trim().len();
            let Some(page_slot) = pages.get_mut(slot) else {
                continue;
            };
            fetched_count += 1;
            total_words += output.word_count;
            *page_slot = Some(FetchedPage {
                text,
                word_count: output.word_count,
                truncated,
            });
        }

        info!(
            requested = count,
            fetched = fetched_count,
            total_words = total_words,
            elapsed_ms = elapsed_ms(started),
            "Web page content fetched for prompt context"
        );
        pages
    }

    async fn web_search(&self, web_query: &str) -> ExternalSearchResult {
        let web_search_start = Instant::now();
        let tuning = self.tuning;
        let mut searched = ExternalSearchResult::default();

        let deep_research_enabled = self.search_flags.deep_research_mode;
        let web_depth = if deep_research_enabled {
            tuning.deep_research_depth.clamp(1, 4)
        } else {
            1
        };
        let web_branch_queries = if deep_research_enabled {
            tuning.deep_research_branch_queries.clamp(1, 4)
        } else {
            1
        };
        let web_max_results = if deep_research_enabled {
            tuning.web_search_max_results.max(10)
        } else {
            tuning.web_search_max_results
        };
        // Deep research wants Wikipedia in the provider mix, but the wiki branch
        // runs beside this one whenever it is planned and reaches wikipedia.org
        // through its own client, which this service's pacer never sees. Asking
        // for it here too would hit the same host twice at the same moment.
        let include_wikipedia = deep_research_enabled && !self.wiki_planned;
        let providers = if include_wikipedia {
            serde_json::json!(["duckduckgo", "bing", "wikipedia"])
        } else {
            serde_json::json!(["duckduckgo", "bing"])
        };

        let executor = self.container.function_executor();
        let call = crate::features::function_calling::domain::FunctionCall::new(
            uuid::Uuid::new_v4().to_string(),
            "web_search",
            serde_json::json!({
                "query": web_query,
                "max_results": web_max_results,
                "page": 1,
                "offset": 0,
                "providers": providers,
                "include_wikipedia": include_wikipedia,
                "depth": web_depth,
                "branch_queries": web_branch_queries
            }),
        );

        match executor.execute(call).await {
            Ok(result) if result.success => {
                if let Some(data) = result.data {
                    match serde_json::from_value::<
                        crate::features::function_calling::dto::WebSearchOutput,
                    >(data)
                    {
                        Ok(output) => {
                            info!(
                                result_count = output.result_count,
                                "Forced web search completed"
                            );
                            if deep_research_enabled {
                                let llm_context_domains = output
                                    .results
                                    .iter()
                                    .filter_map(|result| {
                                        result
                                            .url
                                            .split("://")
                                            .nth(1)
                                            .and_then(|rest| rest.split('/').next())
                                            .map(|host| {
                                                host.trim_start_matches("www.").to_ascii_lowercase()
                                            })
                                    })
                                    .collect::<HashSet<_>>();

                                info!(
                                    unique_queries = output.unique_query_count,
                                    unique_discovered_urls = output.unique_url_count,
                                    unique_discovered_domains = output.unique_domain_count,
                                    urls_passed_to_llm = output.results.len(),
                                    domains_passed_to_llm = llm_context_domains.len(),
                                    total_discovered_results = output.total_results,
                                    returned_results = output.result_count,
                                    "Deep research per-turn telemetry"
                                );
                            }
                            if !output.results.is_empty() {
                                let (top_terms, total_term_count) =
                                    dominant_result_terms(&output.results, 8);
                                if let Some((top_term, top_count)) = top_terms.first() {
                                    let concentration =
                                        (*top_count as f32) / (total_term_count.max(1) as f32);
                                    debug!(
                                        top_terms = ?top_terms,
                                        total_term_count = total_term_count,
                                        concentration = concentration,
                                        "Web result topical diagnostics"
                                    );
                                    if concentration >= 0.20 {
                                        warn!(
                                            top_term = top_term.as_str(),
                                            concentration = concentration,
                                            query_preview = %safe_truncate(web_query, 180),
                                            "Web results appear topically concentrated; consider query diagnostics above"
                                        );
                                    }
                                }
                                searched.sources = build_web_source_citations(
                                    &output.results,
                                    self.highlight_terms,
                                    self.excerpt_chars,
                                );

                                let pages = self.fetch_page_texts(&output.results).await;
                                let context_text = output
                                    .results
                                    .iter()
                                    .enumerate()
                                    .map(|(i, result)| {
                                        let published_line = result
                                            .published_date
                                            .map(|date| {
                                                format!("\nPublished: {}", date.to_rfc3339())
                                            })
                                            .unwrap_or_default();
                                        let snippet = safe_truncate(
                                            &result.snippet,
                                            tuning.web_snippet_max_chars as usize,
                                        );
                                        // A page that was read in full replaces the
                                        // search engine's one-line blurb. The snippet
                                        // stays for the rest, so a fetch that failed
                                        // degrades to what the old behaviour gave.
                                        let body = match pages.get(i).and_then(Option::as_ref) {
                                            Some(page) => format!(
                                                "\nPage content ({} words{}):\n{}",
                                                page.word_count,
                                                if page.truncated { ", truncated" } else { "" },
                                                page.text
                                            ),
                                            None => String::new(),
                                        };
                                        format!(
                                            "[{}] {}\nURL: {}\nSnippet: {}{}{}",
                                            i + 1,
                                            result.title,
                                            result.url,
                                            snippet,
                                            published_line,
                                            body
                                        )
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n\n");
                                if !context_text.trim().is_empty() {
                                    searched.context = Some(context_text);
                                }
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "Failed to parse web search output");
                        }
                    }
                }
            }
            Ok(result) => {
                let reason = result
                    .error_message
                    .clone()
                    .unwrap_or_else(|| "Web search tool execution failed".to_string());
                warn!(error = reason.as_str(), "Web search failed");
                searched.error = Some(reason);
            }
            Err(e) => {
                warn!(error = %e, "Web search failed");
                searched.error = Some(e.to_string());
            }
        }
        searched.elapsed_ms = elapsed_ms(web_search_start);
        searched
    }
}

/// Merge wiki results first: citations are appended, and the wiki context is
/// used only while no web context exists.
pub(super) fn attach_wiki_results(
    outcome: &mut RetrievalPipelineOutcome,
    wiki: ExternalSearchResult,
) {
    outcome.sub_timings.wiki_search_ms = wiki.elapsed_ms;
    if !wiki.sources.is_empty() {
        let wiki_source_count = wiki.sources.len();
        outcome.sources.extend(wiki.sources);
        outcome.sources = deduplicate_sources(std::mem::take(&mut outcome.sources));
        info!(
            wiki_source_count = wiki_source_count,
            merged_source_count = outcome.sources.len(),
            "Forced wiki citations attached"
        );
    }
    if outcome.web_context.is_none() {
        outcome.web_context = wiki.context;
    }
}

/// Merge web results after any wiki results: citations are appended, and the
/// web context follows whatever context is already there.
pub(super) fn attach_web_results(
    outcome: &mut RetrievalPipelineOutcome,
    web: ExternalSearchResult,
) {
    outcome.sub_timings.web_search_ms = web.elapsed_ms;
    if web.error.is_some() {
        outcome.web_search_error = web.error;
    }
    if !web.sources.is_empty() {
        let web_source_count = web.sources.len();
        outcome.sources.extend(web.sources);
        outcome.sources = deduplicate_sources(std::mem::take(&mut outcome.sources));
        info!(
            web_source_count = web_source_count,
            merged_source_count = outcome.sources.len(),
            "Forced web search citations attached"
        );
    }
    if let Some(context_text) = web.context {
        outcome.web_context = match outcome.web_context.take() {
            Some(existing) if !existing.trim().is_empty() => {
                Some(format!("{}\n\n{}", existing, context_text))
            }
            _ => Some(context_text),
        };
    }
}
