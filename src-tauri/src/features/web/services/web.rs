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
//! use lattice::application::dtos::function_calling_dto::WebSearchInput;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let service = WebService::new()?;
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
//! };
//! let results = service.search_web(&request).await?;
//!
//! // Fetch URL
//! let content = service.fetch_url_content("https://www.rust-lang.org").await?;
//! # Ok(())
//! # }
//! ```

use crate::features::function_calling::dto::*;
use crate::features::web::WebServiceTrait;
use crate::shared::constants::WEB_REQUEST_TIMEOUT;
use crate::shared::error::{AppError, Result};
use crate::shared::utils::stealth;
use async_trait::async_trait;
use futures::future::join_all;
use reqwest::header::ACCEPT;
use reqwest::{Client, StatusCode};
use scraper::{Html, Selector};
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, ToSocketAddrs};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};
use tokio::time::sleep;
use tracing::{debug, info, warn};
use url::{form_urlencoded, Url};

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
/// Hosts that queue with the site they mirror rather than on their own.
const SITE_ALIASES: &[(&str, &str)] = &[
    ("html.duckduckgo.com", "duckduckgo.com"),
    ("lite.duckduckgo.com", "duckduckgo.com"),
];

/// Paces requests per site.
///
/// Guarantees two things per site: one request in flight at a time, and at
/// least [`MIN_SITE_REQUEST_INTERVAL`] between the starts of consecutive
/// requests. A slot covers a single request, never a retry sequence or a
/// FlareSolverr round trip, so a slow site delays its own next request instead
/// of every request queued behind it. Waiting is capped by
/// [`SITE_SLOT_WAIT_TIMEOUT`]; past that the request is sent unpaced. Different
/// sites never wait on each other.
struct SitePacer {
    /// Per-site slot, holding when that site's last request started.
    sites: parking_lot::Mutex<HashMap<String, Arc<AsyncMutex<Option<Instant>>>>>,
    min_interval: Duration,
}

impl Default for SitePacer {
    fn default() -> Self {
        Self {
            sites: parking_lot::Mutex::new(HashMap::new()),
            min_interval: MIN_SITE_REQUEST_INTERVAL,
        }
    }
}

impl SitePacer {
    #[cfg(test)]
    fn with_interval(min_interval: Duration) -> Self {
        Self {
            min_interval,
            ..Self::default()
        }
    }

    /// Queue key for `url`: its full host, with the mirror hosts above folded
    /// onto the site they mirror. Keying on a public suffix instead would put
    /// unrelated sites in one queue. `None` when there is no host to pace,
    /// which leaves the request unpaced rather than inventing a per-URL queue.
    fn site_key(url: &str) -> Option<String> {
        let host = Url::parse(url).ok()?.host_str()?.to_ascii_lowercase();
        let host = host.trim_start_matches("www.");
        Some(
            SITE_ALIASES
                .iter()
                .find(|(mirror, _)| *mirror == host)
                .map_or_else(|| host.to_string(), |(_, site)| (*site).to_string()),
        )
    }

    /// Take `url`'s site slot, then wait out whatever is left of that site's
    /// minimum interval. Dropping the guard releases the slot; `None` means the
    /// request is not paced at all — it has no host, or the wait timed out.
    async fn acquire(&self, url: &str) -> Option<OwnedMutexGuard<Option<Instant>>> {
        let key = Self::site_key(url)?;
        let min_interval = self.min_interval;
        let slot = {
            let mut sites = self.sites.lock();
            if sites.len() >= MAX_TRACKED_SITES {
                // Drop only entries nobody holds whose interval has already
                // elapsed: their next request has nothing left to wait for.
                sites.retain(|_, slot| {
                    Arc::strong_count(slot) > 1
                        || slot.try_lock().map_or(true, |last| {
                            last.is_some_and(|started| started.elapsed() < min_interval)
                        })
                });
            }
            Arc::clone(sites.entry(key).or_default())
        };

        let mut last_request =
            match tokio::time::timeout(SITE_SLOT_WAIT_TIMEOUT, slot.lock_owned()).await {
                Ok(guard) => guard,
                Err(_) => {
                    warn!(url, "Site slot wait timed out; sending the request unpaced");
                    return None;
                }
            };
        if let Some(remaining) =
            last_request.and_then(|started| min_interval.checked_sub(started.elapsed()))
        {
            sleep(remaining).await;
        }
        *last_request = Some(Instant::now());
        Some(last_request)
    }
}

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
}

impl WebService {
    /// Create a new web service with stealth features (cookie jar, proxy rotation).
    pub fn new() -> Result<Self> {
        Self::with_timeout(WEB_REQUEST_TIMEOUT)
    }

    /// Create with custom timeout
    pub fn with_timeout(timeout: Duration) -> Result<Self> {
        let client = stealth::stealth_client_builder()
            .timeout(timeout)
            .build()
            .map_err(|e| AppError::InternalError(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            client,
            pacer: SitePacer::default(),
        })
    }

    /// Extract main text content from HTML
    ///
    /// Attempts to extract readable article text by:
    /// 1. Finding <article> elements
    /// 2. Extracting text from <p>, <h1>-<h6> tags
    /// 3. Cleaning whitespace
    fn extract_article_text(&self, html: &str) -> String {
        let document = Html::parse_document(html);

        // Try to find article content
        let article_selector = Selector::parse("article").ok();
        let p_selector = Selector::parse("p, h1, h2, h3, h4, h5, h6").ok();

        let mut text_parts = Vec::new();

        // First try to find article element
        if let Some(selector) = article_selector {
            for element in document.select(&selector) {
                if let Some(p_sel) = &p_selector {
                    for p in element.select(p_sel) {
                        let text = p.text().collect::<Vec<_>>().join(" ");
                        if !text.trim().is_empty() {
                            text_parts.push(text.trim().to_string());
                        }
                    }
                }
            }
        }

        // If no article found, extract all paragraphs
        if text_parts.is_empty() {
            if let Some(p_sel) = &p_selector {
                for p in document.select(p_sel) {
                    let text = p.text().collect::<Vec<_>>().join(" ");
                    if !text.trim().is_empty() {
                        text_parts.push(text.trim().to_string());
                    }
                }
            }
        }

        text_parts.join("\n\n")
    }

    /// Extract title from HTML
    fn extract_title(&self, html: &str) -> Option<String> {
        let document = Html::parse_document(html);
        let title_selector = Selector::parse("title").ok()?;

        document
            .select(&title_selector)
            .next()
            .map(|element| element.text().collect::<String>().trim().to_string())
    }

    /// Resolve DuckDuckGo redirect links and normalize extracted URLs.
    fn normalize_search_url(&self, raw: &str) -> Option<String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }

        if trimmed.starts_with("/ck/a?") {
            return Some(format!("https://www.bing.com{}", trimmed));
        }

        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            if trimmed.contains("bing.com/ck/a?") {
                return Some(trimmed.to_string());
            }
            if trimmed.contains("duckduckgo.com/l/?") || trimmed.contains("html.duckduckgo.com/l/?")
            {
                if let Ok(parsed) = Url::parse(trimmed) {
                    for (k, v) in parsed.query_pairs() {
                        if k == "uddg" && (v.starts_with("http://") || v.starts_with("https://")) {
                            return Some(v.into_owned());
                        }
                    }
                }
            }
            return Some(trimmed.to_string());
        }

        if trimmed.starts_with("//") {
            return Some(format!("https:{}", trimmed));
        }

        if trimmed.starts_with("/l/?") {
            let redirect = format!("https://duckduckgo.com{}", trimmed);
            if let Ok(parsed) = Url::parse(&redirect) {
                for (k, v) in parsed.query_pairs() {
                    if k == "uddg" && (v.starts_with("http://") || v.starts_with("https://")) {
                        return Some(v.into_owned());
                    }
                }
            }
            return None;
        }

        // Fallback for host/path-like values.
        if trimmed.contains('.') && !trimmed.contains(' ') {
            return Some(format!(
                "https://{}",
                trimmed.trim_start_matches('/').trim_end_matches('/')
            ));
        }

        None
    }

    fn is_bot_challenge_page(&self, html: &str) -> bool {
        let lower = html.to_ascii_lowercase();
        (lower.contains("anomaly-modal") && lower.contains("bots use duckduckgo"))
            || (lower.contains("403 - forbidden") && lower.contains("automated queries"))
    }

    fn is_search_result_candidate_url(&self, url: &str) -> bool {
        let Ok(parsed) = Url::parse(url) else {
            return false;
        };

        if !matches!(parsed.scheme(), "http" | "https") {
            return false;
        }

        let Some(host) = parsed.host_str() else {
            return false;
        };

        let host = host.to_ascii_lowercase();
        if host.contains("duckduckgo.com") {
            return false;
        }

        if host.contains("bing.com") {
            return parsed.path().starts_with("/ck/a");
        }

        true
    }

    fn is_tracking_query_param(key: &str) -> bool {
        let lower = key.to_ascii_lowercase();
        lower.starts_with("utm_")
            || matches!(
                lower.as_str(),
                "gclid"
                    | "fbclid"
                    | "msclkid"
                    | "_hsenc"
                    | "_hsmi"
                    | "mc_cid"
                    | "mc_eid"
                    | "ref"
                    | "source"
            )
    }

    fn canonicalize_url_for_dedup(&self, raw: &str) -> String {
        let trimmed = raw.trim();
        let Ok(mut parsed) = Url::parse(trimmed) else {
            return trimmed.to_ascii_lowercase();
        };

        if !matches!(parsed.scheme(), "http" | "https") {
            return trimmed.to_ascii_lowercase();
        }

        parsed.set_fragment(None);
        let _ = parsed.set_username("");
        let _ = parsed.set_password(None);

        if let Some(host) = parsed.host_str() {
            let normalized_host = host.to_ascii_lowercase();
            let _ = parsed.set_host(Some(&normalized_host));
        }

        if let Some(port) = parsed.port() {
            let is_default = (parsed.scheme() == "http" && port == 80)
                || (parsed.scheme() == "https" && port == 443);
            if is_default {
                let _ = parsed.set_port(None);
            }
        }

        let path = {
            let raw_path = parsed.path().trim();
            if raw_path.is_empty() || raw_path == "/" {
                "/".to_string()
            } else {
                format!(
                    "/{}",
                    raw_path.trim_start_matches('/').trim_end_matches('/')
                )
            }
        };
        parsed.set_path(&path);

        if let Some(query) = parsed.query() {
            let mut pairs = form_urlencoded::parse(query.as_bytes())
                .filter(|(key, _)| !Self::is_tracking_query_param(key))
                .map(|(key, value)| (key.into_owned(), value.into_owned()))
                .collect::<Vec<_>>();
            pairs.sort();

            if pairs.is_empty() {
                parsed.set_query(None);
            } else {
                let mut serializer = form_urlencoded::Serializer::new(String::new());
                for (key, value) in pairs {
                    serializer.append_pair(&key, &value);
                }
                parsed.set_query(Some(&serializer.finish()));
            }
        }

        let mut canonical = parsed.to_string();
        if canonical.ends_with('/') && parsed.query().is_none() && parsed.path() == "/" {
            canonical.pop();
        }
        canonical
    }

    fn normalized_domain_key(&self, raw_url: &str) -> Option<String> {
        let parsed = Url::parse(raw_url).ok()?;
        let host = parsed.host_str()?.to_ascii_lowercase();
        Some(host.trim_start_matches("www.").to_string())
    }

    fn reorder_results_for_domain_diversity(
        &self,
        results: Vec<WebSearchResult>,
    ) -> Vec<WebSearchResult> {
        let mut seen_domains = HashSet::new();
        let mut domain_first = Vec::with_capacity(results.len());
        let mut overflow = Vec::new();

        for result in results {
            let domain = self.normalized_domain_key(&result.url);
            let is_new_domain = domain
                .as_ref()
                .is_some_and(|domain_key| seen_domains.insert(domain_key.clone()));

            if is_new_domain {
                domain_first.push(result);
            } else {
                overflow.push(result);
            }
        }

        domain_first.extend(overflow);
        domain_first
    }

    fn parse_search_results(&self, html: &str, max_results: usize) -> Vec<WebSearchResult> {
        let document = Html::parse_document(html);
        let container_selector =
            Selector::parse(".result, .results_links, article, .b_algo, li.b_algo").ok();
        let link_selector =
            Selector::parse("a.result__a, .result__title a, a.result-link, a[data-testid='result-title-a'], .b_algo h2 a").ok();
        let snippet_selector = Selector::parse(
            ".result__snippet, .result-snippet, .result__body, .b_caption p, .b_snippet",
        )
        .ok();

        let mut results = Vec::new();
        let mut seen = HashSet::new();

        if let (Some(cont_sel), Some(link_sel)) =
            (container_selector.as_ref(), link_selector.as_ref())
        {
            for element in document.select(cont_sel) {
                if results.len() >= max_results {
                    break;
                }

                let Some(link) = element.select(link_sel).next() else {
                    continue;
                };
                let Some(href) = link.value().attr("href") else {
                    continue;
                };
                let Some(url) = self.normalize_search_url(href) else {
                    continue;
                };
                let canonical = self.canonicalize_url_for_dedup(&url);
                if !seen.insert(canonical) {
                    continue;
                }

                let title = link
                    .text()
                    .collect::<String>()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .trim()
                    .to_string();
                if title.is_empty() {
                    continue;
                }

                let snippet = snippet_selector
                    .as_ref()
                    .and_then(|sel| element.select(sel).next())
                    .map(|e| {
                        e.text()
                            .collect::<String>()
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ")
                            .trim()
                            .to_string()
                    })
                    .unwrap_or_default();

                results.push(WebSearchResult {
                    title,
                    url,
                    snippet,
                    published_date: None,
                    source: None,
                });
            }
        }

        if results.len() >= max_results {
            return results;
        }

        // Fallback parsing if container-based extraction fails.
        if let Some(link_sel) = link_selector {
            for link in document.select(&link_sel) {
                if results.len() >= max_results {
                    break;
                }
                let Some(href) = link.value().attr("href") else {
                    continue;
                };
                let Some(url) = self.normalize_search_url(href) else {
                    continue;
                };
                let canonical = self.canonicalize_url_for_dedup(&url);
                if !seen.insert(canonical) {
                    continue;
                }
                let title = link
                    .text()
                    .collect::<String>()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .trim()
                    .to_string();
                if title.is_empty() {
                    continue;
                }
                results.push(WebSearchResult {
                    title,
                    url,
                    snippet: String::new(),
                    published_date: None,
                    source: None,
                });
            }
        }

        if results.len() >= max_results {
            return results;
        }

        if let Ok(any_link_selector) = Selector::parse("a[href]") {
            for link in document.select(&any_link_selector) {
                if results.len() >= max_results {
                    break;
                }
                let Some(href) = link.value().attr("href") else {
                    continue;
                };
                let Some(url) = self.normalize_search_url(href) else {
                    continue;
                };
                let canonical = self.canonicalize_url_for_dedup(&url);
                if !self.is_search_result_candidate_url(&url) || !seen.insert(canonical) {
                    continue;
                }

                let title = link
                    .text()
                    .collect::<String>()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .trim()
                    .to_string();
                if title.len() < 6 {
                    continue;
                }

                results.push(WebSearchResult {
                    title,
                    url,
                    snippet: String::new(),
                    published_date: None,
                    source: None,
                });
            }
        }

        results
    }

    fn normalize_provider_list(
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

    fn parse_wikipedia_results(
        &self,
        payload: &serde_json::Value,
        max_results: usize,
    ) -> Vec<WebSearchResult> {
        let mut results = Vec::new();
        let Some(items) = payload
            .get("query")
            .and_then(|v| v.get("search"))
            .and_then(|v| v.as_array())
        else {
            return results;
        };

        for item in items.iter().take(max_results) {
            let title = item
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if title.is_empty() {
                continue;
            }
            let snippet_raw = item
                .get("snippet")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            let snippet = snippet_raw
                .replace("<span class=\"searchmatch\">", "")
                .replace("</span>", "")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let url = format!("https://en.wikipedia.org/wiki/{}", title.replace(' ', "_"));

            results.push(WebSearchResult {
                title,
                url,
                snippet,
                published_date: None,
                source: Some("wikipedia".to_string()),
            });
        }

        results
    }

    fn derive_followup_queries(
        &self,
        seed_query: &str,
        results: &[WebSearchResult],
        branch_queries: usize,
    ) -> Vec<String> {
        let mut terms: Vec<String> = Vec::new();
        let mut seen = HashSet::new();

        for result in results.iter().take(8) {
            for token in format!("{} {}", result.title, result.snippet)
                .split(|ch: char| !ch.is_ascii_alphanumeric())
                .filter(|token| token.len() >= 4)
            {
                let normalized = token.to_ascii_lowercase();
                if seen.insert(normalized.clone()) {
                    terms.push(normalized);
                }
                if terms.len() >= branch_queries.saturating_mul(6) {
                    break;
                }
            }
            if terms.len() >= branch_queries.saturating_mul(6) {
                break;
            }
        }

        let mut queries = Vec::new();
        for chunk in terms.chunks(3) {
            if queries.len() >= branch_queries {
                break;
            }
            let suffix = chunk.join(" ");
            let followup = format!("{seed_query} {suffix}").trim().to_string();
            if followup.len() > seed_query.len() {
                queries.push(followup);
            }
        }
        queries
    }

    /// Search one query across `providers`.
    ///
    /// Every provider's first page is always requested, so those run
    /// concurrently (the site pacer still serializes same-site requests).
    /// Whether a provider needs further pages depends on how many results the
    /// providers before it contributed, so those are fetched afterwards in
    /// provider order. Pages are merged in the same provider/page order as a
    /// fully serial search, which keeps dedup and ordering deterministic.
    async fn search_single_query(
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
    fn absorb_provider_page(&self, merged: &mut QueryResults, provider: &str, page: ProviderPage) {
        for result in page.results {
            let canonical = self.canonicalize_url_for_dedup(&result.url);
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
                                let mut results =
                                    self.parse_search_results(&solved_html, PROVIDER_PAGE_SIZE);
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

                let mut results = self.parse_search_results(&html, PROVIDER_PAGE_SIZE);
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

        match response.text().await {
            Ok(body) => SearchPage::Html(body),
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
                match resp.json::<serde_json::Value>().await {
                    Ok(payload) => {
                        page.results = self.parse_wikipedia_results(&payload, wiki_limit);
                        page.used = true;
                    }
                    Err(e) => {
                        warn!("Failed to parse Wikipedia search response: {}", e);
                        page.last_error = Some(format!("Wikipedia parse failed: {}", e));
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

    /// Check if IP is private or reserved (comprehensive check)
    fn is_private_ip(&self, ip: IpAddr) -> bool {
        match ip {
            IpAddr::V4(ipv4) => {
                ipv4.is_private()
                    || ipv4.is_loopback()
                    || ipv4.is_link_local()
                    || ipv4.is_broadcast()
                    || ipv4.is_documentation()
                    || ipv4.is_unspecified()
                    || ipv4 == Ipv4Addr::new(169, 254, 169, 254) // AWS metadata
            }
            IpAddr::V6(ipv6) => {
                ipv6.is_loopback()
                    || ipv6.is_multicast()
                    || ipv6.is_unspecified()
                    || ipv6.segments()[0] & 0xfe00 == 0xfc00 // fc00::/7 (Unique Local Addresses)
                    || ipv6.segments()[0] & 0xffc0 == 0xfe80 // fe80::/10 (Link-Local)
                    // AWS IPv6 metadata
                    || ipv6.segments() == [0xfd00, 0x0ec2, 0, 0, 0, 0, 0, 0x0254]
            }
        }
    }

    /// Resolve hostname to IPs and validate (prevents DNS rebinding - CWE-918)
    ///
    /// This prevents DNS rebinding attacks by:
    /// 1. Resolving DNS before making HTTP request
    /// 2. Validating all resolved IP addresses
    /// 3. Blocking if any IP is private/reserved
    ///
    /// # Arguments
    /// * `hostname` - Hostname to resolve and validate
    /// * `port` - Port number for socket address resolution
    ///
    /// # Errors
    /// Returns error if hostname resolves to blocked IP or DNS resolution fails
    fn resolve_and_validate_host(&self, hostname: &str, port: u16) -> Result<()> {
        // Block localhost variants immediately
        let lowercase_host = hostname.to_lowercase();
        if lowercase_host == "localhost" || lowercase_host == "0.0.0.0" || lowercase_host == "[::]"
        {
            return Err(AppError::InvalidUrl(format!(
                "Access to localhost blocked: {}",
                hostname
            )));
        }

        // Try to parse as IP address first
        if let Ok(ip) = hostname.parse::<IpAddr>() {
            if self.is_private_ip(ip) {
                return Err(AppError::InvalidUrl(format!(
                    "Access to private/reserved IP blocked: {}",
                    hostname
                )));
            }
            return Ok(());
        }

        // Resolve hostname to IP addresses
        let socket_addr = format!("{}:{}", hostname, port);
        let resolved: Vec<IpAddr> = match socket_addr.to_socket_addrs() {
            Ok(addrs) => addrs.map(|addr| addr.ip()).collect(),
            Err(e) => {
                return Err(AppError::InvalidUrl(format!(
                    "Failed to resolve hostname {}: {}",
                    hostname, e
                )));
            }
        };

        if resolved.is_empty() {
            return Err(AppError::InvalidUrl(format!(
                "No IP addresses resolved for hostname: {}",
                hostname
            )));
        }

        let blocked_ips: Vec<String> = resolved
            .iter()
            .filter(|ip| self.is_private_ip(**ip))
            .map(|ip| ip.to_string())
            .collect();

        if !blocked_ips.is_empty() {
            return Err(AppError::InvalidUrl(format!(
                "Hostname {} resolves to blocked IP(s): {}. Access to private networks is not allowed.",
                hostname,
                blocked_ips.join(", ")
            )));
        }

        debug!(
            "DNS validation passed for {}: resolved to {:?}",
            hostname, resolved
        );
        Ok(())
    }
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
        let mut frontier = vec![query.to_string()];

        for _ in 0..depth {
            if frontier.is_empty() {
                break;
            }
            let level_queries: Vec<String> = frontier
                .into_iter()
                .take(branch_queries)
                .filter(|frontier_query| {
                    let normalized_query = frontier_query.trim().to_ascii_lowercase();
                    !normalized_query.is_empty() && seen_queries.insert(normalized_query)
                })
                .collect();
            executed_queries.extend(level_queries.iter().cloned());

            // Search the level's queries concurrently (the site pacer keeps
            // same-site requests spaced), then merge in frontier order so dedup,
            // telemetry and follow-ups match a serial run. No extra concurrency
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

            let mut next_frontier = Vec::new();
            for (frontier_query, query_results) in level_queries.iter().zip(level_results) {
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
                if results.is_empty() {
                    continue;
                }

                for result in &results {
                    let canonical_url = self.canonicalize_url_for_dedup(&result.url);
                    if seen_urls.insert(canonical_url) {
                        if let Some(domain) = self.normalized_domain_key(&result.url) {
                            seen_domains.insert(domain);
                        }
                        merged_results.push(result.clone());
                    }
                }

                if depth > 1 {
                    for followup in self
                        .derive_followup_queries(frontier_query, &results, branch_queries)
                        .into_iter()
                        .take(branch_queries)
                    {
                        next_frontier.push(followup);
                    }
                }
            }
            frontier = next_frontier;
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

        Ok(WebSearchOutput {
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
        })
    }

    async fn fetch_url_content(&self, url: &str) -> Result<FetchUrlContentOutput> {
        debug!("Fetching URL: {}", url);

        // Validate URL for security
        self.validate_url(url)?;

        stealth::random_delay(500, 2000).await;

        let start = Instant::now();
        let profile = stealth::random_profile();
        let headers = stealth::browser_headers(profile, None);

        let response = self
            .client
            .get(url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| AppError::Network(format!("Failed to fetch URL: {}", e)))?;

        if !response.status().is_success() {
            warn!("URL fetch failed with status: {}", response.status());
            return Err(AppError::Network(format!(
                "HTTP {}: {}",
                response.status(),
                response.status().canonical_reason().unwrap_or("Unknown")
            )));
        }

        // Get final URL (after redirects)
        let final_url = response.url().to_string();

        // Re-validate final URL after redirects (prevents redirect-based SSRF)
        if final_url != url {
            debug!("URL redirected to: {}", final_url);
            self.validate_url(&final_url)?;
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let html = response
            .text()
            .await
            .map_err(|e| AppError::Network(format!("Failed to read response body: {}", e)))?;

        let fetch_time_ms = start.elapsed().as_secs_f64() * 1000.0;

        let title = self.extract_title(&html);
        let content = self.extract_article_text(&html);

        // Truncate if needed (50k chars default)
        let max_len = 50000;
        let (final_content, truncated) = if content.len() > max_len {
            (content[..max_len].to_string(), true)
        } else {
            (content, false)
        };

        let word_count = final_content.split_whitespace().count();

        info!(
            "URL fetched: {} words in {:.2}ms",
            word_count, fetch_time_ms
        );

        Ok(FetchUrlContentOutput {
            url: final_url,
            title,
            content: final_content,
            content_truncated: truncated,
            word_count,
            fetch_time_ms,
            content_type,
        })
    }

    fn validate_url(&self, url: &str) -> Result<()> {
        let parsed =
            Url::parse(url).map_err(|e| AppError::InvalidUrl(format!("Invalid URL: {}", e)))?;

        // Check scheme (only allow http/https)
        match parsed.scheme() {
            "http" | "https" => {}
            _ => {
                return Err(AppError::InvalidUrl(format!(
                    "Unsupported URL scheme: {}. Only http and https are allowed.",
                    parsed.scheme()
                )));
            }
        }

        let host = parsed
            .host_str()
            .ok_or_else(|| AppError::InvalidUrl("URL must have a host".to_string()))?;

        let port = parsed
            .port()
            .unwrap_or_else(|| if parsed.scheme() == "https" { 443 } else { 80 });

        // Resolve DNS and validate IPs (prevents DNS rebinding - CWE-918)
        self.resolve_and_validate_host(host, port)?;

        debug!("URL validation passed: {}", url);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_url() {
        let service = WebService::new().unwrap();

        // Public IP literals exercise URL validation without making this unit
        // test depend on external DNS availability.
        assert!(service.validate_url("https://1.1.1.1").is_ok());
        assert!(service.validate_url("http://8.8.8.8").is_ok());

        // Invalid: localhost
        assert!(service.validate_url("http://localhost:8000").is_err());
        assert!(service.validate_url("http://127.0.0.1").is_err());

        // Invalid: private IP
        assert!(service.validate_url("http://192.168.1.1").is_err());
        assert!(service.validate_url("http://10.0.0.1").is_err());

        // Invalid: metadata endpoint
        assert!(service.validate_url("http://169.254.169.254").is_err());

        // Invalid: unsupported scheme
        assert!(service.validate_url("ftp://example.com").is_err());
        assert!(service.validate_url("file:///etc/passwd").is_err());
    }

    #[test]
    fn test_is_private_ip() {
        let service = WebService::new().unwrap();

        // Private IPs (should be blocked)
        assert!(service.is_private_ip("192.168.1.1".parse().unwrap()));
        assert!(service.is_private_ip("10.0.0.1".parse().unwrap()));
        assert!(service.is_private_ip("172.16.0.1".parse().unwrap()));

        // Loopback
        assert!(service.is_private_ip("127.0.0.1".parse().unwrap()));

        // Link-local
        assert!(service.is_private_ip("169.254.1.1".parse().unwrap()));

        // AWS metadata
        assert!(service.is_private_ip("169.254.169.254".parse().unwrap()));

        // Public IPs (should be allowed)
        assert!(!service.is_private_ip("8.8.8.8".parse().unwrap()));
        assert!(!service.is_private_ip("1.1.1.1".parse().unwrap()));
    }

    #[test]
    fn test_extract_title() {
        let service = WebService::new().unwrap();

        let html = r#"
            <html>
                <head><title>Test Page</title></head>
                <body></body>
            </html>
        "#;

        let title = service.extract_title(html);
        assert_eq!(title, Some("Test Page".to_string()));
    }

    #[test]
    fn test_extract_article_text() {
        let service = WebService::new().unwrap();

        let html = r#"
            <html>
                <body>
                    <article>
                        <h1>Main Title</h1>
                        <p>First paragraph.</p>
                        <p>Second paragraph.</p>
                    </article>
                </body>
            </html>
        "#;

        let text = service.extract_article_text(html);
        assert!(text.contains("Main Title"));
        assert!(text.contains("First paragraph"));
        assert!(text.contains("Second paragraph"));
    }

    #[test]
    fn test_normalize_search_url_accepts_bing_redirect_links() {
        let service = WebService::new().unwrap();
        let raw = "/ck/a?!&&p=abc123";
        let normalized = service.normalize_search_url(raw);
        assert_eq!(
            normalized.as_deref(),
            Some("https://www.bing.com/ck/a?!&&p=abc123")
        );
    }

    #[test]
    fn test_detect_duckduckgo_challenge_page() {
        let service = WebService::new().unwrap();
        let challenge_html = r#"
            <html><body>
                <div class="anomaly-modal__title">Unfortunately, bots use DuckDuckGo too.</div>
            </body></html>
        "#;
        assert!(service.is_bot_challenge_page(challenge_html));
    }

    #[test]
    fn test_parse_search_results_generic_anchor_fallback() {
        let service = WebService::new().unwrap();
        let html = r#"
            <html>
                <body>
                    <a href="https://example.com/articles/arugula-health-benefits">
                        Arugula compounds and health effects
                    </a>
                </body>
            </html>
        "#;

        let results = service.parse_search_results(html, 5);
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].url,
            "https://example.com/articles/arugula-health-benefits"
        );
    }

    #[test]
    fn test_parse_search_results_generic_fallback_skips_search_hosts() {
        let service = WebService::new().unwrap();
        let html = r#"
            <html>
                <body>
                    <a href="https://www.bing.com/search?q=arugula">Bing navigation</a>
                    <a href="/ck/a?!&&p=abc123">Result via redirect</a>
                </body>
            </html>
        "#;

        let results = service.parse_search_results(html, 5);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].url, "https://www.bing.com/ck/a?!&&p=abc123");
    }

    fn search_result(url: &str, source: &str) -> WebSearchResult {
        WebSearchResult {
            title: url.to_string(),
            url: url.to_string(),
            snippet: String::new(),
            published_date: None,
            source: Some(source.to_string()),
        }
    }

    #[test]
    fn test_site_key_aliases_mirrors_without_collapsing_public_suffixes() {
        for url in [
            "https://html.duckduckgo.com/html/?q=a&s=0",
            "https://duckduckgo.com/html/?q=a&s=0",
            "https://lite.duckduckgo.com/lite/?q=a&s=0",
        ] {
            assert_eq!(SitePacer::site_key(url).as_deref(), Some("duckduckgo.com"));
        }
        assert_eq!(
            SitePacer::site_key("https://www.bing.com/search?q=a").as_deref(),
            Some("bing.com")
        );
        assert_eq!(
            SitePacer::site_key("https://en.wikipedia.org/w/api.php?action=query").as_deref(),
            Some("en.wikipedia.org")
        );
        // Unrelated sites sharing a public suffix must not share a queue.
        assert_ne!(
            SitePacer::site_key("https://www.bbc.co.uk/news"),
            SitePacer::site_key("https://www.theguardian.co.uk/news")
        );
        assert_eq!(SitePacer::site_key("not a url"), None);
    }

    #[tokio::test]
    async fn test_site_pacer_spaces_same_site_requests() {
        let interval = Duration::from_millis(120);
        let pacer = SitePacer::with_interval(interval);

        // The first request to a site has nothing to pace against.
        let start = Instant::now();
        drop(pacer.acquire("https://html.duckduckgo.com/html/?q=a").await);
        assert!(
            start.elapsed() < interval,
            "first request to a site must not wait"
        );

        // A mirror of that site waits out the rest of the interval.
        let start = Instant::now();
        drop(pacer.acquire("https://lite.duckduckgo.com/lite/?q=b").await);
        let waited = start.elapsed();
        assert!(
            waited >= interval / 2,
            "same site must wait out the interval, waited {waited:?}"
        );

        // Another site is paced independently.
        let start = Instant::now();
        drop(pacer.acquire("https://www.bing.com/search?q=a").await);
        assert!(
            start.elapsed() < interval,
            "another site must not inherit the wait"
        );
    }

    #[tokio::test]
    async fn test_site_pacer_serializes_same_site_only() {
        // A short interval keeps the assertions about the slot itself, not
        // about the pacing wait, which the test above covers.
        let pacer = SitePacer::with_interval(Duration::from_millis(1));
        let wait = Duration::from_millis(50);
        let ddg_slot = pacer.acquire("https://html.duckduckgo.com/html/?q=a").await;

        let bing_slot =
            tokio::time::timeout(wait, pacer.acquire("https://www.bing.com/search?q=a")).await;
        assert!(bing_slot.is_ok(), "another site must not wait");

        let mirror_slot =
            tokio::time::timeout(wait, pacer.acquire("https://lite.duckduckgo.com/lite/?q=b"))
                .await;
        assert!(
            mirror_slot.is_err(),
            "same site must wait for the held slot"
        );

        drop(ddg_slot);
        let mirror_slot =
            tokio::time::timeout(wait, pacer.acquire("https://lite.duckduckgo.com/lite/?q=b"))
                .await;
        assert!(mirror_slot.is_ok(), "released slot must be handed on");
    }

    #[test]
    fn test_absorb_provider_page_merges_in_provider_order() {
        let service = WebService::new().unwrap();
        let mut merged = QueryResults::default();

        service.absorb_provider_page(
            &mut merged,
            "duckduckgo",
            ProviderPage {
                results: vec![
                    search_result("https://a.example/x", "duckduckgo"),
                    search_result("https://b.example/", "duckduckgo"),
                ],
                used: true,
                last_error: Some("duckduckgo retry".to_string()),
            },
        );
        service.absorb_provider_page(
            &mut merged,
            "bing",
            ProviderPage {
                results: vec![
                    search_result("https://b.example", "bing"),
                    search_result("https://c.example/", "bing"),
                ],
                used: true,
                last_error: None,
            },
        );
        assert_eq!(merged.last_error.as_deref(), Some("duckduckgo retry"));

        service.absorb_provider_page(
            &mut merged,
            "wikipedia",
            ProviderPage {
                results: Vec::new(),
                used: false,
                last_error: Some("Wikipedia search HTTP 503".to_string()),
            },
        );

        let merged_urls: Vec<_> = merged
            .results
            .iter()
            .map(|result| (result.url.as_str(), result.source.as_deref()))
            .collect();
        assert_eq!(
            merged_urls,
            vec![
                ("https://a.example/x", Some("duckduckgo")),
                ("https://b.example/", Some("duckduckgo")),
                ("https://c.example/", Some("bing")),
            ]
        );
        assert_eq!(
            merged.providers_used,
            HashSet::from(["duckduckgo".to_string(), "bing".to_string()])
        );
        assert_eq!(
            merged.last_error.as_deref(),
            Some("Wikipedia search HTTP 503")
        );
    }
}
