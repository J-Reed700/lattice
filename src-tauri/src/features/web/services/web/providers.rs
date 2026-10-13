//! Querying search providers page by page and merging what they return.
use super::page::{decode_response_text, read_bounded_body};
use super::{
    ProviderPage, QueryResults, SearchPage, WebService, MAX_PROVIDER_PAGES, MAX_SEARCH_BODY_BYTES,
    MAX_WIKIPEDIA_BODY_BYTES, PROVIDER_PAGE_SIZE,
};
use crate::features::web::services::stealth;
use futures::future::join_all;
use reqwest::header::ACCEPT;
use reqwest::StatusCode;
use std::collections::HashSet;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, warn};

impl WebService {
    pub(super) fn normalize_provider_list(
        &self,
        raw_providers: &[String],
        include_wikipedia: bool,
    ) -> Vec<String> {
        let mut ordered = Vec::new();
        let mut seen = HashSet::new();

        for provider in raw_providers {
            let canonical = match provider.trim().to_ascii_lowercase().as_str() {
                "ddg" | "duckduckgo" | "duckduckgo_html" => "duckduckgo",
                "bing" => "bing",
                "wiki" | "wikipedia" => "wikipedia",
                _ => continue,
            };
            if seen.insert(canonical.to_string()) {
                ordered.push(canonical.to_string());
            }
        }

        if ordered.is_empty() {
            ordered.push("duckduckgo".to_string());
            ordered.push("bing".to_string());
        }

        if include_wikipedia && seen.insert("wikipedia".to_string()) {
            ordered.push("wikipedia".to_string());
        }

        ordered
    }

    fn build_provider_search_urls(
        &self,
        provider: &str,
        encoded_query: &str,
        provider_offset: usize,
    ) -> Vec<String> {
        match provider {
            "duckduckgo" => vec![
                format!(
                    "https://html.duckduckgo.com/html/?q={}&s={}",
                    encoded_query, provider_offset
                ),
                format!(
                    "https://duckduckgo.com/html/?q={}&s={}",
                    encoded_query, provider_offset
                ),
                format!(
                    "https://lite.duckduckgo.com/lite/?q={}&s={}",
                    encoded_query, provider_offset
                ),
            ],
            "bing" => vec![format!(
                "https://www.bing.com/search?q={}&count=10&first={}",
                encoded_query,
                provider_offset.saturating_add(1)
            )],
            _ => Vec::new(),
        }
    }

    fn wikipedia_search_url(&self, encoded_query: &str, limit: usize, offset: usize) -> String {
        format!(
            "https://en.wikipedia.org/w/api.php?action=query&format=json&utf8=1&list=search&srsearch={}&srlimit={}&sroffset={}&srprop=snippet",
            encoded_query, limit, offset
        )
    }

    /// Search one query across `providers`.
    ///
    /// Every provider's first page is always requested, so those run
    /// concurrently (the site pacer still serializes same-site requests).
    /// Whether a provider needs further pages depends on how many results the
    /// providers before it contributed, so those are fetched afterwards in
    /// provider order. Pages are merged in the same provider/page order as a
    /// fully serial search, which keeps dedup and ordering deterministic.
    pub(super) async fn search_single_query(
        &self,
        query: &str,
        providers: &[String],
        requested_total: usize,
    ) -> QueryResults {
        let encoded_query = urlencoding::encode(query).into_owned();
        let required_pages = requested_total
            .div_ceil(PROVIDER_PAGE_SIZE)
            .clamp(1, MAX_PROVIDER_PAGES);

        let first_pages = join_all(providers.iter().map(|provider| {
            self.fetch_provider_page(provider, &encoded_query, 0, requested_total)
        }))
        .await;

        let mut merged = QueryResults::default();
        for (provider, first_page) in providers.iter().zip(first_pages) {
            self.absorb_provider_page(&mut merged, provider, first_page);
            if provider == "wikipedia" {
                continue;
            }

            for page_idx in 1..required_pages {
                if merged.results.len() >= requested_total {
                    break;
                }
                let page = self
                    .fetch_provider_page(provider, &encoded_query, page_idx, requested_total)
                    .await;
                self.absorb_provider_page(&mut merged, provider, page);
            }
        }

        merged
    }

    /// Fold one provider page into a query's results. Earlier providers and
    /// pages win URL dedup; the latest error overwrites earlier ones.
    pub(super) fn absorb_provider_page(
        &self,
        merged: &mut QueryResults,
        provider: &str,
        page: ProviderPage,
    ) {
        for result in page.results {
            let canonical = Self::canonicalize_url_for_dedup(&result.url);
            if merged.seen_urls.insert(canonical) {
                merged.results.push(result);
            }
        }
        if page.used {
            merged.providers_used.insert(provider.to_string());
        }
        if page.last_error.is_some() {
            merged.last_error = page.last_error;
        }
    }

    /// Fetch one result page from `provider`.
    ///
    /// Wikipedia is a single lookup and ignores `page_idx`. HTML providers try
    /// their fallback URLs in order until one yields results; every attempt
    /// takes the site's pacing slot on its own, so backoff sleeps and bot
    /// challenges never hold the site against other queries.
    async fn fetch_provider_page(
        &self,
        provider: &str,
        encoded_query: &str,
        page_idx: usize,
        requested_total: usize,
    ) -> ProviderPage {
        if provider == "wikipedia" {
            return self
                .fetch_wikipedia_page(encoded_query, requested_total)
                .await;
        }

        const MAX_RETRIES_PER_PROVIDER: usize = 3;
        const BASE_RETRY_DELAY_MS: u64 = 250;

        let should_retry_status = |status: StatusCode| {
            status == StatusCode::FORBIDDEN
                || status == StatusCode::TOO_MANY_REQUESTS
                || status.is_server_error()
        };
        let backoff = |attempt: usize| {
            Duration::from_millis(BASE_RETRY_DELAY_MS.saturating_mul(1_u64 << (attempt - 1)))
        };

        let mut page = ProviderPage::default();
        let provider_offset = page_idx.saturating_mul(PROVIDER_PAGE_SIZE);
        let search_urls = self.build_provider_search_urls(provider, encoded_query, provider_offset);

        for search_url in &search_urls {
            for attempt in 1..=MAX_RETRIES_PER_PROVIDER {
                debug!(
                    "Web search attempt via {} provider={} (try {}/{})",
                    search_url, provider, attempt, MAX_RETRIES_PER_PROVIDER
                );

                let html = match self.fetch_search_page(search_url, attempt).await {
                    SearchPage::Html(html) => html,
                    SearchPage::Status(status) => {
                        warn!(
                            "Web search provider returned status {} for {} on attempt {}",
                            status, search_url, attempt
                        );
                        page.last_error = Some(format!(
                            "Web search failed with status {} from {} (attempt {})",
                            status, search_url, attempt
                        ));

                        if attempt < MAX_RETRIES_PER_PROVIDER && should_retry_status(status) {
                            sleep(backoff(attempt)).await;
                            continue;
                        }
                        break;
                    }
                    SearchPage::Failed(error) => {
                        page.last_error = Some(error);

                        if attempt < MAX_RETRIES_PER_PROVIDER {
                            sleep(backoff(attempt)).await;
                            continue;
                        }
                        break;
                    }
                };

                if self.is_bot_challenge_page(&html) {
                    warn!(
                        "Web search provider {} returned anti-bot challenge page on attempt {}",
                        search_url, attempt
                    );

                    if let Some(solver) = stealth::flaresolverr() {
                        info!("Attempting FlareSolverr bypass for {}", search_url);
                        match solver.solve(search_url).await {
                            Ok(solved_html) => {
                                if solved_html.len() > MAX_SEARCH_BODY_BYTES {
                                    warn!("FlareSolverr response exceeded the search body limit");
                                    page.last_error = Some(
                                        "FlareSolverr response exceeded its byte limit".into(),
                                    );
                                    break;
                                }
                                let mut results = match self
                                    .parse_search_results_bounded(solved_html, PROVIDER_PAGE_SIZE)
                                    .await
                                {
                                    Ok(results) => results,
                                    Err(error) => {
                                        page.last_error = Some(error.to_string());
                                        break;
                                    }
                                };
                                for result in &mut results {
                                    result.source = Some(provider.to_string());
                                }
                                if !results.is_empty() {
                                    page.used = true;
                                    page.results = results;
                                    break;
                                }
                            }
                            Err(e) => {
                                warn!("FlareSolverr failed for {}: {}", search_url, e);
                            }
                        }
                    }

                    page.last_error = Some(format!(
                        "Search provider challenge page from {} (attempt {})",
                        search_url, attempt
                    ));
                    break;
                }

                let mut results = match self
                    .parse_search_results_bounded(html, PROVIDER_PAGE_SIZE)
                    .await
                {
                    Ok(results) => results,
                    Err(error) => {
                        page.last_error = Some(error.to_string());
                        break;
                    }
                };
                for result in &mut results {
                    result.source = Some(provider.to_string());
                }

                if !results.is_empty() {
                    page.used = true;
                    page.results = results;
                    break;
                }

                if attempt < MAX_RETRIES_PER_PROVIDER {
                    sleep(backoff(attempt)).await;
                    continue;
                }
            }

            // A fallback URL that returned results completes the page.
            if page.used {
                break;
            }
        }

        page
    }

    /// One paced request to `search_url`.
    ///
    /// The site's slot is held for the request and its body read, and released
    /// before the caller's backoff sleep or FlareSolverr bypass.
    async fn fetch_search_page(&self, search_url: &str, attempt: usize) -> SearchPage {
        let _site_slot = self.pacer.acquire(search_url).await;

        let response = match self.build_search_request(search_url).send().await {
            Ok(response) => response,
            Err(e) => {
                warn!(
                    "Web search request failed for {} on attempt {}: {}",
                    search_url, attempt, e
                );
                return SearchPage::Failed(format!(
                    "Request failed for {} (attempt {}): {}",
                    search_url, attempt, e
                ));
            }
        };

        let status = response.status();
        if !status.is_success() {
            return SearchPage::Status(status);
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        match read_bounded_body(response, MAX_SEARCH_BODY_BYTES).await {
            Ok(body) => SearchPage::Html(decode_response_text(body, content_type.as_deref())),
            Err(e) => {
                warn!(
                    "Failed to read search response from {} on attempt {}: {}",
                    search_url, attempt, e
                );
                SearchPage::Failed(format!(
                    "Failed reading search response from {} (attempt {}): {}",
                    search_url, attempt, e
                ))
            }
        }
    }

    /// Run the Wikipedia search lookup for one query.
    async fn fetch_wikipedia_page(
        &self,
        encoded_query: &str,
        requested_total: usize,
    ) -> ProviderPage {
        let mut page = ProviderPage::default();
        let wiki_limit = requested_total.clamp(1, 20);
        let wiki_url = self.wikipedia_search_url(encoded_query, wiki_limit, 0);

        let _site_slot = self.pacer.acquire(&wiki_url).await;
        let response = self
            .build_search_request(&wiki_url)
            .header(ACCEPT, "application/json")
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => {
                match read_bounded_body(resp, MAX_WIKIPEDIA_BODY_BYTES).await {
                    Ok(body) => match serde_json::from_slice::<serde_json::Value>(&body) {
                        Ok(payload) => {
                            page.results = self.parse_wikipedia_results(&payload, wiki_limit);
                            page.used = true;
                        }
                        Err(e) => {
                            warn!("Failed to parse Wikipedia search response: {}", e);
                            page.last_error = Some(format!("Wikipedia parse failed: {}", e));
                        }
                    },
                    Err(e) => {
                        warn!("Failed to read bounded Wikipedia response: {}", e);
                        page.last_error = Some(format!("Wikipedia body failed: {}", e));
                    }
                }
            }
            Ok(resp) => {
                let status = resp.status();
                warn!("Wikipedia search returned status {}", status);
                page.last_error = Some(format!("Wikipedia search HTTP {}", status));
            }
            Err(e) => {
                warn!("Wikipedia search request failed: {}", e);
                page.last_error = Some(format!("Wikipedia search request failed: {}", e));
            }
        }

        page
    }

    fn build_search_request(&self, url: &str) -> reqwest::RequestBuilder {
        let profile = stealth::random_profile();
        let referer = stealth::search_referer_for_url(url);
        let headers = stealth::search_headers(profile, referer);
        self.client.get(url).headers(headers)
    }
}
