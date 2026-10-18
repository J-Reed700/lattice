//! Web service for web search and URL fetching
//!
//! Provides secure web integration with SSRF prevention.
//!
//! # Security
//!
//! - **SSRF Prevention (CWE-918)**: Blocks private IPs, localhost, metadata endpoints
//! - **Request Timeout**: 10-second default timeout
//! - **Content Length Limiting**: Prevents memory exhaustion
//! - **User Agent**: Identifies requests as coming from Lattice
//!
//! # Example
//! ```rust,no_run
//! use lattice::infrastructure::services::web_service::WebService;
//! use lattice_core::application::dtos::function_calling_dto::WebSearchInput;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let service = WebService::new(std::path::Path::new("/tmp/lattice"))?;
//!
//! // Search web
//! let request = WebSearchInput {
//!     query: "rust programming".to_string(),
//!     max_results: 5,
//!     page: 1,
//!     offset: 0,
//!     providers: vec![],
//!     include_wikipedia: false,
//!     depth: 1,
//!     branch_queries: 2,
//!     followup_queries: vec![],
//! };
//! let results = service.search_web(&request).await?;
//!
//! // Fetch URL
//! let content = service.fetch_url_content("https://www.rust-lang.org").await?;
//! # Ok(())
//! # }
//! ```

use crate::features::web::services::browser_reader::BrowserReader;
use crate::features::web::services::page_cache::PageCache;
use crate::features::web::services::stealth;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::future::join_all;
use lattice_core::application::contracts::web::*;
use lattice_core::application::ports::WebServiceTrait;
use lattice_core::shared::constants::WEB_REQUEST_TIMEOUT;
use lattice_core::shared::error::{AppError, Result};
use reqwest::{Client, StatusCode};
use std::collections::HashSet;
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tracing::{debug, info};

mod address_guard;
mod bounded;
mod pacing;
mod page;
mod providers;
mod results;

pub(crate) use address_guard::is_blocked_ip;
pub(crate) use address_guard::validate_url_resolved;
use address_guard::{
    check_url_host, redirect_policy, refuse_blocked_addresses, PublicOnlyResolver,
};
pub use bounded::{BoundedBody, BoundedFetchError};
use pacing::{SearchCache, SitePacer};
use page::parse_fetchable_url;

/// Extracted text shorter than this is a shell — a title, a menu, a cookie
/// notice — and the page is worth reading in the browser. Matches the chat
/// pipeline's floor for a page worth quoting.
const THIN_PAGE_CHARS: usize = 400;

/// `<article>` text is used only when it is at least 1/N of the page's
/// paragraph text; below that it is cards or teasers, not the body.
const ARTICLE_SHARE_DENOMINATOR: usize = 3;

/// Results requested per search-engine page.
const PROVIDER_PAGE_SIZE: usize = 10;
/// Most pages fetched from one provider for one query.
const MAX_PROVIDER_PAGES: usize = 5;
/// Least time between the starts of two requests to the same site.
const MIN_SITE_REQUEST_INTERVAL: Duration = Duration::from_millis(900);
/// Longest wait for a site's slot. A wedged holder must not queue the rest of
/// the search behind it, so past this the request goes out unpaced.
const SITE_SLOT_WAIT_TIMEOUT: Duration = Duration::from_secs(15);
/// Sites tracked at once before idle entries are dropped. `WebService` is a
/// container singleton, so the map would otherwise live for the whole process.
const MAX_TRACKED_SITES: usize = 64;
/// What a page is read for. Either way the whole extracted text is kept: the
/// decoded body is already bounded by [`MAX_PAGE_BODY_BYTES`], and a second
/// cap on the text would discard a page's tail after a successful, bounded
/// download. What the text is for decides how it is extracted and cached; how
/// much of it a prompt carries is decided by that prompt's own budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PageRead {
    /// A page read for a conversation: article extraction, the browser
    /// fallback for script-built pages, and the shared page cache.
    Chat,
    /// A reference capture: full-document markdown extraction with the
    /// page's provenance signals, never served from or written to the chat
    /// cache. Its browser fallback reads the rendered document, so the same
    /// extraction runs on it.
    Reference,
}
/// Maximum decoded bytes accepted from a page before HTML parsing.
pub(crate) const MAX_PAGE_BODY_BYTES: usize = 5 * 1024 * 1024;
/// Maximum decoded bytes accepted from a search provider response.
const MAX_SEARCH_BODY_BYTES: usize = 2 * 1024 * 1024;
/// Maximum decoded bytes accepted from the Wikipedia JSON endpoint.
const MAX_WIKIPEDIA_BODY_BYTES: usize = 1024 * 1024;
/// Number of HTML parses admitted at once across requests.
const MAX_HTML_PARSERS: usize = 2;
const RESPONSE_BODY_TIMEOUT: Duration = Duration::from_secs(30);
const HTML_PARSE_TIMEOUT: Duration = Duration::from_secs(15);
/// The page cache's folder inside the app data directory.
const PAGE_CACHE_DIR_NAME: &str = "web-cache";
/// How long a search's results are reused. Long enough to cover a regenerate
/// and the follow-ups around it, short enough that "what happened today" does
/// not answer from this morning.
const SEARCH_CACHE_TTL: Duration = Duration::from_secs(15 * 60);
/// Searches held at once. Each is a few kilobytes of snippets.
const MAX_CACHED_SEARCHES: usize = 64;
/// Hosts that queue with the site they mirror rather than on their own.
const SITE_ALIASES: &[(&str, &str)] = &[
    ("html.duckduckgo.com", "duckduckgo.com"),
    ("lite.duckduckgo.com", "duckduckgo.com"),
];

/// What one paced request to a search URL produced.
enum SearchPage {
    /// A successful response's body.
    Html(String),
    /// A non-success status, which decides retrying on its own.
    Status(StatusCode),
    /// The request or the body read failed, with the message for `last_error`.
    Failed(String),
}

/// What one provider page (or the single Wikipedia lookup) produced for a query.
///
/// Results stay un-deduplicated so pages can be fetched concurrently and then
/// merged in the serial provider/page order.
#[derive(Default)]
struct ProviderPage {
    results: Vec<WebSearchResult>,
    /// The provider returned results (Wikipedia: a parseable payload).
    used: bool,
    /// Last failure seen while fetching this page.
    last_error: Option<String>,
}

/// One query's results across providers, merged in provider order.
#[derive(Default)]
struct QueryResults {
    results: Vec<WebSearchResult>,
    seen_urls: HashSet<String>,
    providers_used: HashSet<String>,
    last_error: Option<String>,
}

/// Web service implementation
///
/// Provides web search via DuckDuckGo and URL content fetching
/// with comprehensive security controls and stealth anti-bot measures.
pub struct WebService {
    /// HTTP client with cookie jar and optional proxy
    client: Client,
    /// Per-site request queue shared by every search on this service.
    pacer: SitePacer,
    /// Pages and settled refusals kept across turns, on disk.
    page_cache: PageCache,
    /// The last few searches, so a regenerate does not re-issue them.
    search_cache: SearchCache,
    /// Blocking HTML parsers admitted at once. The permit lives in the
    /// blocking closure, including when its async caller is cancelled.
    html_parsers: Arc<Semaphore>,
    /// The hidden browser a page that refuses the HTTP client is read in.
    /// The app attaches it once it is running; until then such a page keeps
    /// its HTTP result.
    browser: OnceLock<Arc<BrowserReader>>,
}

/// A page read for display, which is a fetch plus the one thing the tool
/// output has no room for: when the text was actually read. A cache hit is
/// served instantly and could otherwise only claim to have been read "now".
pub struct ReadPage {
    pub output: FetchUrlContentOutput,
    pub fetched_at: DateTime<Utc>,
    /// What the page's markup and HTTP exchange said about where it came
    /// from (source provenance §4), or why this read could not say.
    pub signals: ProvenanceSignals,
}

impl WebService {
    /// Create a new web service with stealth features (cookie jar, proxy rotation).
    ///
    /// `data_dir` is the app data directory; the page cache lives in a
    /// `web-cache/` folder under it and is created on first use.
    pub fn new(data_dir: &Path) -> Result<Self> {
        Self::with_timeout(WEB_REQUEST_TIMEOUT, data_dir)
    }

    /// Create with custom timeout
    pub fn with_timeout(timeout: Duration, data_dir: &Path) -> Result<Self> {
        let client = build_public_client(timeout)?;

        Ok(Self {
            client,
            pacer: SitePacer::default(),
            page_cache: PageCache::new(data_dir.join(PAGE_CACHE_DIR_NAME)),
            search_cache: SearchCache::default(),
            html_parsers: Arc::new(Semaphore::new(MAX_HTML_PARSERS)),
            browser: OnceLock::new(),
        })
    }

    /// Read refused pages through `browser` from now on. The first reader
    /// attached stays; the app has one.
    pub fn attach_browser(&self, browser: Arc<BrowserReader>) {
        let _ = self.browser.set(browser);
    }
}

/// The one client for reading public web pages, with the default request
/// timeout: stealth headers and cookies, the private-address redirect policy
/// and the public-only resolver. The snapshot engine fetches through it.
pub(crate) fn public_web_client() -> Result<Client> {
    build_public_client(WEB_REQUEST_TIMEOUT)
}

/// The single builder behind [`WebService`] and [`public_web_client`].
fn build_public_client(timeout: Duration) -> Result<Client> {
    stealth::stealth_client_builder()
        .timeout(timeout)
        .redirect(redirect_policy())
        .dns_resolver(Arc::new(PublicOnlyResolver::new(stealth::proxy_hosts())))
        .build()
        .map_err(|e| AppError::InternalError(format!("Failed to create HTTP client: {}", e)))
}

/// Parse `url` and accept only `http`/`https`: the same check page reads use.
pub(crate) fn fetchable_url(url: &str) -> Result<url::Url> {
    parse_fetchable_url(url)
}

#[async_trait]
impl WebServiceTrait for WebService {
    async fn search_web(&self, input: &WebSearchInput) -> Result<WebSearchOutput> {
        let query = input.query.trim();
        if query.is_empty() {
            return Err(AppError::InvalidInput(
                "Web search query cannot be empty".to_string(),
            ));
        }

        if let Some(cached) = self.search_cache.get(input) {
            info!(
                query,
                results = cached.results.len(),
                "Search served from the recent-search cache"
            );
            return Ok(cached);
        }

        let max_results = input.max_results.clamp(1, 50);
        let page = input.page.max(1);
        let effective_offset = input
            .offset
            .saturating_add(page.saturating_sub(1).saturating_mul(max_results));
        let requested_total = effective_offset.saturating_add(max_results);
        let providers = self.normalize_provider_list(&input.providers, input.include_wikipedia);

        debug!(
            "Web search: query='{}', max_results={}, page={}, offset={}, providers={:?}",
            query, max_results, page, effective_offset, providers
        );

        let depth = input.depth.clamp(1, 4);
        let branch_queries = input.branch_queries.clamp(1, 4);
        let start = Instant::now();
        let mut merged_results: Vec<WebSearchResult> = Vec::new();
        let mut seen_urls = HashSet::new();
        let mut providers_used = HashSet::new();
        let mut seen_queries = HashSet::new();
        let mut executed_queries: Vec<String> = Vec::new();
        let mut seen_domains = HashSet::new();
        let mut last_error: Option<String> = None;
        // The root on its own, then the caller's follow-ups `branch_queries`
        // at a time, one level per step of depth.
        //
        // Follow-ups are written by whoever asked, never mined from the
        // results here. Mining ran for months and never stopped producing
        // noise: the words several snippets share are a page's furniture as
        // often as its subject, and each fix (stopwords, address filtering,
        // anchoring on title words) moved the noise rather than removing it —
        // "steps greens garden step learn", "steps greens garden beginner
        // right". Choosing another angle on a subject takes reading it, which
        // is the model's job, not a word count's.
        let followup_levels = depth.saturating_sub(1);
        let mut followups = input
            .followup_queries
            .iter()
            .map(|followup| followup.trim())
            .filter(|followup| !followup.is_empty())
            .take(followup_levels * branch_queries)
            .map(str::to_string)
            .collect::<Vec<_>>()
            .into_iter();
        let mut frontier = vec![query.to_string()];

        while !frontier.is_empty() {
            let level_queries: Vec<String> = frontier
                .into_iter()
                .filter(|frontier_query| {
                    let normalized_query = frontier_query.trim().to_ascii_lowercase();
                    !normalized_query.is_empty() && seen_queries.insert(normalized_query)
                })
                .collect();
            executed_queries.extend(level_queries.iter().cloned());

            // Search the level's queries concurrently (the site pacer keeps
            // same-site requests spaced), then merge in frontier order so dedup
            // and telemetry match a serial run. No extra concurrency
            // limit: `branch_queries` already bounds a level to four queries.
            // The futures are collected up front because holding the borrowing
            // `map` closure across the await fails this `async_trait` future's
            // `Send` check.
            let searches: Vec<_> = level_queries
                .iter()
                .map(|frontier_query| {
                    self.search_single_query(frontier_query, &providers, requested_total)
                })
                .collect();
            let level_results: Vec<QueryResults> = join_all(searches).await;

            for query_results in level_results {
                let QueryResults {
                    results,
                    providers_used: used_by_query,
                    last_error: maybe_error,
                    ..
                } = query_results;
                if let Some(err) = maybe_error {
                    last_error = Some(err);
                }
                for provider in used_by_query {
                    providers_used.insert(provider);
                }
                for result in &results {
                    let canonical_url = Self::canonicalize_url_for_dedup(&result.url);
                    if seen_urls.insert(canonical_url) {
                        if let Some(domain) = self.normalized_domain_key(&result.url) {
                            seen_domains.insert(domain);
                        }
                        merged_results.push(result.clone());
                    }
                }
            }
            frontier = followups.by_ref().take(branch_queries).collect();
        }

        if merged_results.is_empty() {
            return Err(AppError::Network(last_error.unwrap_or_else(|| {
                "All web search providers failed or returned zero results".to_string()
            })));
        }

        let merged_results = self.reorder_results_for_domain_diversity(merged_results);
        let total_results = merged_results.len();
        let start_idx = effective_offset.min(total_results);
        let end_idx = (start_idx + max_results).min(total_results);
        let page_results = merged_results
            .into_iter()
            .skip(start_idx)
            .take(end_idx.saturating_sub(start_idx))
            .collect::<Vec<_>>();

        let elapsed = start.elapsed();
        info!(
            "Web search completed in {:.2}ms (results={}, total_results={}, providers={:?})",
            elapsed.as_secs_f64() * 1000.0,
            page_results.len(),
            total_results,
            providers_used
        );
        info!(
            depth,
            branch_queries,
            unique_queries = seen_queries.len(),
            unique_urls = seen_urls.len(),
            unique_domains = seen_domains.len(),
            executed_query_preview = ?executed_queries.iter().take(8).collect::<Vec<_>>(),
            "Deep-research telemetry"
        );

        let mut providers_used_vec = providers_used.into_iter().collect::<Vec<_>>();
        providers_used_vec.sort();

        let output = WebSearchOutput {
            results: page_results,
            query: query.to_string(),
            result_count: end_idx.saturating_sub(start_idx),
            page,
            offset: effective_offset,
            total_results,
            has_more: end_idx < total_results,
            providers_used: providers_used_vec,
            unique_query_count: seen_queries.len(),
            unique_url_count: seen_urls.len(),
            unique_domain_count: seen_domains.len(),
            // The first executed query is the one that was asked for.
            followup_queries: executed_queries.into_iter().skip(1).collect(),
        };
        if !output.results.is_empty() {
            self.search_cache.store(input, &output);
        }
        Ok(output)
    }

    async fn fetch_url_content(&self, url: &str) -> Result<FetchUrlContentOutput> {
        self.read_page(url).await.map(|read| read.output)
    }

    async fn fetch_reference_content(&self, url: &str) -> Result<FetchUrlContentOutput> {
        self.fetch_reference_page(url).await.map(|page| page.output)
    }

    async fn fetch_reference_page(&self, url: &str) -> Result<ReferencePage> {
        let page = self.read_page_as(url, PageRead::Reference).await?;
        if page.output.content.trim().len() < THIN_PAGE_CHARS {
            return Err(AppError::InvalidInput("This page did not provide enough static reference text. Supply a readable chapter, document, or pasted text.".into()));
        }
        Ok(ReferencePage {
            output: page.output,
            signals: page.signals,
            fetched_at: page.fetched_at.timestamp(),
        })
    }

    /// The synchronous check the tool executors run before a request on their
    /// own clients. It resolves with the blocking resolver because the trait
    /// method is synchronous; this service's own fetches use
    /// [`validate_url_resolved`] and a resolver that pins what it checked.
    fn validate_url(&self, url: &str) -> Result<()> {
        let parsed = parse_fetchable_url(url)?;
        let Some(domain) = check_url_host(&parsed)? else {
            return Ok(());
        };
        let port = parsed.port_or_known_default().unwrap_or(443);
        let resolved: Vec<SocketAddr> = (domain.as_str(), port)
            .to_socket_addrs()
            .map_err(|e| AppError::InvalidUrl(format!("Failed to resolve hostname {domain}: {e}")))?
            .collect();
        refuse_blocked_addresses(&domain, &resolved)
    }
}

#[cfg(test)]
mod tests;
