//! The wiki/web phase of a turn: its interpretation, the web query it
//! searches with, and the wiki search.
use super::*;

fn overlap_ratio(a: &HashSet<String>, b: &HashSet<String>) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let overlap = a.intersection(b).count() as f32;
    overlap / (b.len() as f32)
}

impl ExternalLookup<'_> {
    pub(super) fn is_planned(&self) -> bool {
        self.wiki_planned || self.web_planned
    }

    /// Interprets the turn and rewrites the web query side by side, then runs the
    /// wiki and web searches side by side. `allow_lookup` false still runs the
    /// interpretation. `web_reason` is why the web is searched, for the record.
    pub(super) async fn run(
        &self,
        base_interpretation: &HyDEInterpretation,
        interpret: bool,
        allow_lookup: bool,
        web_reason: WebSearchReason,
    ) -> ExternalPhaseOutcome {
        let prep_start = Instant::now();
        let search_wiki = self.wiki_planned && allow_lookup;
        let search_web = self.web_planned && allow_lookup;
        // HyDE and web-query rewriting run on the utility LLM (small, fast,
        // local) when set, not the chat LLM. See utility_llm resolution above.
        let hyde_service = self.container.query_interpreter(
            Arc::clone(self.utility_llm),
            self.cancellation.turn_token(self.request_id),
        );
        let hyde_context = if interpret || search_web {
            let conversation = build_hyde_context_window_for_conversation(
                self.conv_service,
                self.conversation_id,
                self.validated_message,
                self.utility_llm.as_ref(),
                None,
            )
            .await;
            // The attachment leads: it is this turn's subject, and on a first
            // message there is no conversation window at all.
            match (self.attachment_digest, conversation) {
                (Some(digest), Some(window)) => Some(format!("{digest}\n{window}")),
                (Some(digest), None) => Some(digest.to_string()),
                (None, window) => window,
            }
        } else {
            None
        };
        let (interpretation, generated_web_query) = tokio::join!(
            async {
                if !interpret {
                    return None;
                }
                // The expansion is a full utility-model generation that only the wiki search
                // reads, so it is generated only when the wiki is searched.
                if search_wiki {
                    return hyde_service
                        .interpret_with_context(
                            self.validated_message,
                            hyde_context.as_deref(),
                        )
                        .await
                        .inspect_err(|error| {
                            tracing::warn!(%error, "Query expansion failed; searching with the message as written")
                        })
                        .ok();
                }
                hyde_service
                    .classify_with_context(self.validated_message, hyde_context.as_deref())
                    .await
                    .inspect_err(|error| {
                        tracing::warn!(%error, "Query classification failed; using the base interpretation")
                    })
                    .ok()
                    .map(|query_type| {
                        HyDEInterpretation::raw_only(self.validated_message, query_type)
                    })
            },
            async {
                if !search_web {
                    return None;
                }
                Some(
                    hyde_service
                        .web_search_query_with_context(
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
        // Deep research searches the subject from several angles after the
        // main query. The utility model writes them from the same reading of
        // the turn; a failure means one search, not a mined guess.
        let research_followups = match web_query.as_deref() {
            Some(root) if self.search_flags.deep_research_mode => {
                let count = (self.tuning.deep_research_depth.clamp(1, 4) as usize - 1)
                    * self.tuning.deep_research_branch_queries.clamp(1, 4) as usize;
                hyde_service
                    .research_followups_with_context(
                        root,
                        self.validated_message,
                        hyde_context.as_deref(),
                        count,
                    )
                    .await
                    .unwrap_or_else(|error| {
                        warn!(%error, "Deep research runs the main search alone");
                        Vec::new()
                    })
            }
            _ => Vec::new(),
        };
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
                Some(
                    self.web_search(query, &research_followups, web_reason)
                        .await,
                )
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
        let step = self.recorder.begin_guarded(
            TurnStepKind::Wiki,
            "Checking Wikipedia",
            Some(wiki_query.clone()),
        );

        let executor = self.container.tools();
        let call = lattice_core::application::contracts::tools::FunctionCall::new(
            uuid::Uuid::new_v4().to_string(),
            "wiki_search",
            serde_json::json!({
                "query": wiki_query,
                "max_results": tuning.wiki_search_max_results
            }),
        );

        let mut failure: Option<String> = None;
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

                                searched.context = wiki_context(&web_like_results);
                            }
                        }
                        Err(error) => {
                            warn!(error = %error, "Failed to parse wiki search output");
                            failure = Some(format!("unreadable result: {error}"));
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
                failure = Some(reason);
            }
            Err(error) => {
                warn!(error = %error, "Wiki search failed");
                failure = Some(error.to_string());
            }
        }
        match failure {
            Some(reason) => step.failed(Some(reason)),
            None => step.done(Some(result_count_line(searched.sources.len()))),
        }
        searched.elapsed_ms = elapsed_ms(wiki_search_start);
        searched
    }
}

/// The prompt items of a wiki search: every result, its snippet whole. The
/// turn's budget decides how many the prompt shows.
fn wiki_context(results: &[WebSearchResult]) -> Vec<WebContextItem> {
    results
        .iter()
        .map(|item| WebContextItem {
            url: item.url.clone(),
            title: item.title.clone(),
            detail: format!("URL: {}\nSnippet: {}", item.url, item.snippet.trim()),
        })
        .collect()
}

#[cfg(test)]
mod wiki_context_tests {
    use super::*;

    #[test]
    fn every_wiki_result_reaches_the_turn_with_its_whole_snippet() {
        let results: Vec<WebSearchResult> = (0..6)
            .map(|index| WebSearchResult {
                title: format!("Article {index}"),
                url: format!("https://en.wikipedia.org/wiki/{index}"),
                snippet: format!("{index} {}", "long snippet text ".repeat(100)),
                published_date: None,
                source: Some("wikipedia".to_string()),
            })
            .collect();
        let context = wiki_context(&results);
        assert_eq!(context.len(), 6);
        assert!(context[5].detail.ends_with(results[5].snippet.trim()));
    }
}
