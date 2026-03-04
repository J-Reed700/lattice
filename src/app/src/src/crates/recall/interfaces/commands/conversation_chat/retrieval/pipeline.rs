use super::*;

fn overlap_ratio(a: &HashSet<String>, b: &HashSet<String>) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let overlap = a.intersection(b).count() as f32;
    overlap / (b.len() as f32)
}

fn dominant_result_terms(
    results: &[crate::application::dtos::function_calling_dto::WebSearchResult],
    limit: usize,
) -> (Vec<(String, usize)>, usize) {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut total = 0usize;

    for result in results {
        let combined = format!("{} {}", result.title, result.snippet);
        for token in tokenize_keyword_terms(&combined).into_iter().filter(|t| t.len() >= 4) {
            *counts.entry(token).or_insert(0) += 1;
            total += 1;
        }
    }

    let mut ranked: Vec<(String, usize)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    (ranked.into_iter().take(limit).collect(), total)
}

pub(super) async fn run_retrieval_pipeline(
    container: &Container,
    conv_service: &Arc<dyn crate::infrastructure::services::traits::ConversationServiceTrait>,
    conversation_id: &str,
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

    if let Err(e) = container.get_or_load_embedding().await {
        debug!(error = %e, "Embedding model not available for RAG — search will be skipped");
    }

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
        interpretation: crate::domain::qa::hyde::HyDEInterpretation::raw_only(
            validated_message.to_string(),
            QueryType::Question,
        ),
        search_response: crate::application::dtos::search_dto::SearchResponseDto {
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
    };
    let tuning = &search_settings.retrieval_tuning;

    let mut retrieval_plan = RetrievalPlan::from_router(
        router_settings,
        router_decision,
        validated_message,
        search_flags,
    );
    let mut external_hyde_context: Option<String> = None;
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

    if outcome.short_circuit_response.is_none() && retrieval_plan.should_search_kb {
        kb_attempted = true;
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
        outcome.interpretation = kb_outcome.interpretation;
        outcome.search_response = kb_outcome.search_response;
        outcome.sources = kb_outcome.sources;
        outcome.kb_unavailable_reason = kb_outcome.kb_unavailable_reason;
        outcome.sub_timings = kb_outcome.timings;
        outcome.sub_timings.kb_total_ms = elapsed_ms(kb_retrieval_start);
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

    if outcome.short_circuit_response.is_none()
        && (retrieval_plan.should_search_web || retrieval_plan.should_search_wiki)
        && outcome.interpretation.hyde_text.is_none()
    {
        let external_hyde_start = Instant::now();
        let hyde_service = crate::infrastructure::services::hyde::HyDEService::new(Arc::clone(llm));
        let hyde_context =
            build_hyde_context_window_for_conversation(conv_service, conversation_id).await;
        external_hyde_context = hyde_context.clone();
        if let Ok(interpretation) = hyde_service
            .interpret_query_with_context(validated_message, hyde_context.as_deref())
            .await
        {
            outcome.interpretation = interpretation;
        }
        outcome.sub_timings.external_hyde_interpretation_ms = elapsed_ms(external_hyde_start);
    }

    let external_followup_anchor_terms = if search_flags.force_followup_mode
        || matches!(outcome.interpretation.query_type, QueryType::Followup)
    {
        let terms = load_followup_turn_anchor_terms(conv_service, conversation_id).await;
        if terms.is_empty() {
            None
        } else {
            Some(terms)
        }
    } else {
        None
    };

    if outcome.short_circuit_response.is_none()
        && retrieval_plan.should_search_wiki
        && allow_external_lookup
    {
        let wiki_search_start = Instant::now();
        let wiki_query = select_wiki_search_query_with_tuning(
            validated_message,
            &outcome.interpretation,
            external_followup_anchor_terms.as_ref(),
            tuning,
        );

        let executor = container.function_executor();
        let call = crate::domain::function_call::FunctionCall::new(
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

                                let wiki_sources = build_web_source_citations(
                                    &web_like_results,
                                    highlight_terms,
                                    tool_output_settings.excerpt_chars as usize,
                                );
                                if !wiki_sources.is_empty() {
                                    let wiki_source_count = wiki_sources.len();
                                    outcome.sources.extend(wiki_sources);
                                    outcome.sources = deduplicate_sources(outcome.sources);
                                    info!(
                                        wiki_source_count = wiki_source_count,
                                        merged_source_count = outcome.sources.len(),
                                        "Forced wiki citations attached"
                                    );
                                }

                                if outcome.web_context.is_none() {
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
                                        outcome.web_context = Some(wiki_context);
                                    }
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
        outcome.sub_timings.wiki_search_ms = elapsed_ms(wiki_search_start);
    }

    if outcome.short_circuit_response.is_none()
        && retrieval_plan.should_search_web
        && allow_external_lookup
    {
        let web_search_start = Instant::now();
        let web_query_generation_start = Instant::now();
        if external_hyde_context.is_none() {
            external_hyde_context =
                build_hyde_context_window_for_conversation(conv_service, conversation_id).await;
        }
        let lexical_web_query = select_web_search_query_with_tuning(
            validated_message,
            &outcome.interpretation,
            external_followup_anchor_terms.as_ref(),
            tuning,
        );
        let mut generated_web_query: Option<String> = None;
        let mut web_query_source = "hyde_generated";
        let hyde_service = crate::infrastructure::services::hyde::HyDEService::new(Arc::clone(llm));
        let web_query = match hyde_service
            .generate_web_search_query_with_context(
                validated_message,
                external_hyde_context.as_deref(),
            )
            .await
        {
            Ok(query) if !query.trim().is_empty() => {
                generated_web_query = Some(query.trim().to_string());
                safe_truncate(query.trim(), tuning.external_search_query_max_chars as usize)
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
        let hyde_terms = outcome
            .interpretation
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
                outcome
                    .interpretation
                    .hyde_text
                    .as_deref()
                    .unwrap_or(""),
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
        outcome.sub_timings.external_hyde_interpretation_ms +=
            elapsed_ms(web_query_generation_start);

        let deep_research_enabled = search_flags.deep_research_mode;
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
        let include_wikipedia = retrieval_plan.should_search_wiki || deep_research_enabled;
        let providers = if include_wikipedia {
            serde_json::json!(["duckduckgo", "bing", "wikipedia"])
        } else {
            serde_json::json!(["duckduckgo", "bing"])
        };

        let executor = container.function_executor();
        let call = crate::domain::function_call::FunctionCall::new(
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
                        crate::application::dtos::function_calling_dto::WebSearchOutput,
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
                                            query_preview = %safe_truncate(&web_query, 180),
                                            "Web results appear topically concentrated; consider query diagnostics above"
                                        );
                                    }
                                }
                                let web_sources = build_web_source_citations(
                                    &output.results,
                                    highlight_terms,
                                    tool_output_settings.excerpt_chars as usize,
                                );
                                if !web_sources.is_empty() {
                                    let web_source_count = web_sources.len();
                                    outcome.sources.extend(web_sources);
                                    outcome.sources = deduplicate_sources(outcome.sources);
                                    info!(
                                        web_source_count = web_source_count,
                                        merged_source_count = outcome.sources.len(),
                                        "Forced web search citations attached"
                                    );
                                }

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
                                        format!(
                                            "[{}] {}\nURL: {}\nSnippet: {}{}",
                                            i + 1,
                                            result.title,
                                            result.url,
                                            snippet,
                                            published_line
                                        )
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n\n");
                                if !context_text.trim().is_empty() {
                                    outcome.web_context = match outcome.web_context.take() {
                                        Some(existing) if !existing.trim().is_empty() => {
                                            Some(format!("{}\n\n{}", existing, context_text))
                                        }
                                        _ => Some(context_text),
                                    };
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
                outcome.web_search_error = Some(reason.clone());
                warn!(error = reason.as_str(), "Web search failed");
            }
            Err(e) => {
                outcome.web_search_error = Some(e.to_string());
                warn!(error = %e, "Web search failed");
            }
        }
        outcome.sub_timings.web_search_ms = elapsed_ms(web_search_start);
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
        kb_build_sources_ms = outcome.sub_timings.kb_build_sources_ms,
        kb_persist_references_ms = outcome.sub_timings.kb_persist_references_ms,
        external_hyde_interpretation_ms = outcome.sub_timings.external_hyde_interpretation_ms,
        wiki_search_ms = outcome.sub_timings.wiki_search_ms,
        web_search_ms = outcome.sub_timings.web_search_ms,
        total_ms = outcome.sub_timings.total_ms,
        "retrieval: sub timing metrics"
    );
    outcome
}
