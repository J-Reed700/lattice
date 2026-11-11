//! The web search, and the result pages it opens and reads.
use super::*;

fn dominant_result_terms(
    results: &[lattice_core::application::contracts::web::WebSearchResult],
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

/// The host a URL names, or the URL itself when it will not parse.
fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_owned))
        .unwrap_or_else(|| "a web page".to_string())
}

/// Pages a search opens: none when the user turned page reading off (the
/// search then answers from snippets), otherwise as many as the page budget
/// gives a share worth reading.
pub(super) fn pages_wanted(read_web_pages: bool, page_budget_chars: usize) -> usize {
    if read_web_pages {
        pages_the_budget_carries(page_budget_chars)
    } else {
        0
    }
}

impl ExternalLookup<'_> {
    /// How many readable pages this turn wants in its prompt: as many as the
    /// page budget can give a share worth reading, so a large window reads
    /// every result it was handed and a small one reads only what it can carry.
    fn pages_wanted(&self) -> usize {
        pages_wanted(self.tuning.read_web_pages, self.page_budget_chars)
    }

    /// Read one page, or say why not.
    async fn fetch_one_page(
        &self,
        url: String,
    ) -> std::result::Result<lattice_core::application::contracts::web::FetchUrlContentOutput, String>
    {
        // Named by host so progress and a blocked site are visible.
        let step = self.recorder.begin_guarded_with_links(
            TurnStepKind::ReadPage,
            format!("Reading {}", host_of(&url)),
            Some(url.clone()),
            vec![TurnStepLinkDto::new(url.clone(), None)],
        );
        // The conversation's own archive comes first: a page it already read is
        // answered from the citation's permanent snapshot, not the network.
        let outcome = match super::super::super::source_snapshots::archived_page(
            self.container,
            self.conversation_id,
            &url,
        )
        .await
        {
            Some(page) => Ok(page),
            None => {
                let outcome = self.fetch_one_page_inner(url.clone()).await;
                if let Ok(page) = &outcome {
                    super::super::super::source_snapshots::archive_page_for_url(
                        self.container,
                        self.conversation_id,
                        &url,
                        page,
                    )
                    .await;
                }
                outcome
            }
        };
        match &outcome {
            Ok(page) => web_steps::finish_page(step, &url, page),
            Err(reason) => step.failed(Some(reason.clone())),
        }
        outcome
    }

    async fn fetch_one_page_inner(
        &self,
        url: String,
    ) -> std::result::Result<lattice_core::application::contracts::web::FetchUrlContentOutput, String>
    {
        let timeout = Duration::from_secs(self.tuning.web_page_fetch_timeout_secs.max(1) as u64);
        let call = lattice_core::application::contracts::tools::FunctionCall::new(
            uuid::Uuid::new_v4().to_string(),
            "fetch_url_content",
            serde_json::json!({ "url": url }),
        );
        let executor = self.container.tools();
        let outcome = match tokio::time::timeout(timeout, executor.execute(call)).await {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(error)) => return Err(error.to_string()),
            Err(_) => return Err(format!("timed out after {}s", timeout.as_secs())),
        };
        if !outcome.success {
            return Err(outcome
                .error_message
                .unwrap_or_else(|| "the fetch did not succeed".to_string()));
        }
        let data = outcome
            .data
            .ok_or_else(|| "the fetch returned no content".to_string())?;
        let output = serde_json::from_value::<
            lattice_core::application::contracts::web::FetchUrlContentOutput,
        >(data)
        .map_err(|error| format!("the fetched page could not be parsed: {error}"))?;
        // Extraction can succeed on a page that is all navigation, a cookie
        // wall or a video player. The snippet says as much in less space.
        if output.content.trim().len() < MIN_USEFUL_PAGE_CHARS {
            return Err("the page had no readable article text".to_string());
        }
        Ok(output)
    }

    /// Opens the top web results and reads them, in waves of side-by-side
    /// fetches: a page that fails or has no article is replaced by the next
    /// result down in the following wave. The pages share the turn's page budget
    /// (see [`super::super::page_budget`]), and everything tried is recorded in
    /// the returned memory. The vector is index-aligned with `results`; `None`
    /// was never tried.
    async fn fetch_page_texts(
        &self,
        results: &[lattice_core::application::contracts::web::WebSearchResult],
    ) -> (Vec<Option<PageOutcome>>, FetchMemory) {
        let mut outcomes: Vec<Option<PageOutcome>> = (0..results.len()).map(|_| None).collect();
        let mut memory = FetchMemory::default();

        let budget = self.page_budget_chars;
        let useful = useful_page_share(budget);
        let wanted = self.pages_wanted().min(results.len());
        if wanted == 0 {
            return (outcomes, memory);
        }
        // Reading pages is the slowest thing the external phase does. A stop
        // press that landed during the search itself should not buy the user
        // another round of network fetches.
        if self.cancellation.is_cancel_requested(self.request_id) {
            debug!("Turn cancelled before page fetching — keeping snippets only");
            return (outcomes, memory);
        }

        let started = Instant::now();
        let mut read: Vec<(
            usize,
            lattice_core::application::contracts::web::FetchUrlContentOutput,
        )> = Vec::new();
        let mut next = 0usize;
        let mut attempted = 0usize;
        for wave in 0..MAX_PAGE_FETCH_WAVES {
            // Open more only while the pages read so far, given room by rank,
            // leave a useful share for another: a failed page, or short ones,
            // free room for the next results down.
            let short_by = if wave == 0 {
                wanted
            } else {
                read.sort_by_key(|(slot, _)| *slot);
                let lengths: Vec<usize> = read
                    .iter()
                    .map(|(_, output)| output.content.trim().chars().count())
                    .collect();
                let used: usize = super::super::page_budget::by_rank(&lengths, budget, useful)
                    .iter()
                    .sum();
                pages_the_budget_carries(budget.saturating_sub(used))
            };
            if short_by == 0 || next >= results.len() {
                break;
            }
            if wave > 0 && self.cancellation.is_cancel_requested(self.request_id) {
                break;
            }
            let batch: Vec<(usize, String)> = results
                .iter()
                .enumerate()
                .skip(next)
                .take(short_by)
                .map(|(slot, result)| (slot, result.url.clone()))
                .collect();
            next += batch.len();
            attempted += batch.len();

            let fetched = futures::future::join_all(
                batch
                    .iter()
                    .map(|(_, url)| self.fetch_one_page(url.clone())),
            )
            .await;
            for ((slot, url), fetched) in batch.into_iter().zip(fetched) {
                match fetched {
                    Ok(output) => read.push((slot, output)),
                    Err(reason) => {
                        debug!(
                            url = url.as_str(),
                            reason = reason.as_str(),
                            "Page could not be read — keeping its snippet"
                        );
                        memory.record_failure(&url, &reason);
                        if let Some(outcome) = outcomes.get_mut(slot) {
                            *outcome = Some(PageOutcome::Unreadable(reason));
                        }
                    }
                }
            }
        }

        read.sort_by_key(|(slot, _)| *slot);
        let lengths: Vec<usize> = read
            .iter()
            .map(|(_, output)| output.content.trim().chars().count())
            .collect();
        let budgets = super::super::page_budget::by_rank(&lengths, budget, useful);

        let mut carried = 0usize;
        let mut carried_whole = 0usize;
        let mut total_words = 0usize;
        for (((slot, output), length), budget) in read.into_iter().zip(lengths).zip(budgets) {
            let full = output.content.trim();
            let Some(result) = results.get(slot) else {
                continue;
            };
            // No room for it in this prompt. It is still remembered, so the
            // model can ask for it and be answered without a request.
            if budget == 0 {
                memory.record_page(&result.url, full, Delivery::Part { start: 0, end: 0 });
                continue;
            }
            let whole = budget >= length;
            let delivery = if whole {
                Delivery::Whole
            } else {
                Delivery::Part {
                    start: 0,
                    end: budget,
                }
            };
            memory.record_page(&result.url, full, delivery);
            carried += 1;
            carried_whole += usize::from(whole);
            total_words += output.word_count;
            if let Some(outcome) = outcomes.get_mut(slot) {
                *outcome = Some(PageOutcome::Read(FetchedPage {
                    text: safe_truncate(full, budget),
                    word_count: output.word_count,
                    clipped_at: (!whole).then_some(budget),
                }));
            }
        }

        info!(
            wanted = wanted,
            attempted = attempted,
            carried = carried,
            carried_whole = carried_whole,
            total_words = total_words,
            page_budget_chars = self.page_budget_chars,
            elapsed_ms = elapsed_ms(started),
            "Web page content fetched for prompt context"
        );
        (outcomes, memory)
    }

    pub(super) async fn web_search(
        &self,
        web_query: &str,
        followups: &[String],
        reason: WebSearchReason,
    ) -> ExternalSearchResult {
        let web_search_start = Instant::now();
        let tuning = self.tuning;
        let mut searched = ExternalSearchResult::default();
        // Held as an option because a search that succeeds is finished before
        // its pages are read, not after. The label says why the question left
        // the machine: the user asked, or the vault fell short.
        let mut step = Some(self.recorder.begin_guarded(
            TurnStepKind::WebSearch,
            reason.step_label(),
            Some(web_query.to_string()),
        ));

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
        // No engine is named, so the finder routes to every engine the user has on.
        // Wikipedia is left to the wiki branch whenever it is planned: it reaches
        // wikipedia.org through its own client, so asking here too would hit it twice.
        let include_wikipedia = deep_research_enabled && !self.wiki_planned;

        let executor = self.container.tools();
        let call = lattice_core::application::contracts::tools::FunctionCall::new(
            uuid::Uuid::new_v4().to_string(),
            "web_search",
            serde_json::json!({
                "query": web_query,
                "max_results": web_max_results,
                "page": 1,
                "offset": 0,
                "include_wikipedia": include_wikipedia,
                "depth": web_depth,
                "branch_queries": web_branch_queries,
                "followup_queries": followups
            }),
        );

        match executor.execute(call).await {
            Ok(result) if result.success => {
                if let Some(data) = result.data {
                    match serde_json::from_value::<
                        lattice_core::application::contracts::web::WebSearchOutput,
                    >(data)
                    {
                        Ok(output) => {
                            info!(
                                result_count = output.result_count,
                                "Forced web search completed"
                            );
                            // Said now, before the slow page reads, so a waiting reader sees the
                            // addresses.
                            if let Some(step) = step.take() {
                                web_steps::finish_search(step, self.recorder, &output);
                            }
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
                                searched.queries = std::iter::once(web_query.to_string())
                                    .chain(output.followup_queries.iter().cloned())
                                    .collect();

                                let (pages, page_memory) =
                                    self.fetch_page_texts(&output.results).await;
                                searched.pages = page_memory;
                                searched.context = output
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
                                        let snippet = result.snippet.trim();
                                        // A page read in full replaces the search engine's
                                        // blurb; the snippet stays for the rest.
                                        let body =
                                            render_page_body(pages.get(i).and_then(Option::as_ref));
                                        WebContextItem {
                                            url: result.url.clone(),
                                            title: result.title.clone(),
                                            detail: format!(
                                                "URL: {}\nSnippet: {}{}{}",
                                                result.url, snippet, published_line, body
                                            ),
                                        }
                                    })
                                    .collect();
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "Failed to parse web search output");
                            searched.error = Some(format!("unreadable results: {e}"));
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
        if let Some(step) = step {
            match &searched.error {
                Some(reason) => step.failed(Some(reason.clone())),
                None => step.done(Some(result_count_line(searched.sources.len()))),
            }
        }
        searched.elapsed_ms = elapsed_ms(web_search_start);
        searched
    }
}
