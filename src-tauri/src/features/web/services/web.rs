//! Web service: search and URL fetching for the chat tools, the reader and
//! the learning research loop.
//!
//! Search delegates to source finding (`super::sourcing`): the router plans
//! waves of finders, the runner calls them under their breakers and a shared
//! page cache, and the result carries every finder's outcome, so a provider
//! that failed or was skipped is named rather than silently missing.
//!
//! # Security
//!
//! - **SSRF Prevention (CWE-918)**: Blocks private IPs, localhost, metadata endpoints
//! - **Request Timeout**: 10-second default timeout
//! - **Content Length Limiting**: Prevents memory exhaustion
//! - **Identity**: browser-like headers for pages and HTML engines, a declared
//!   `Lattice/<version>` agent for APIs (`super::identity`)

use crate::features::web::services::browser_reader::BrowserReader;
use crate::features::web::services::finders::{self, FinderHttp};
use crate::features::web::services::identity;
use crate::features::web::services::page_cache::PageCache;
use crate::features::web::services::sourcing::SourcingService;
use crate::features::web::services::wayback::Wayback;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lattice_core::application::contracts::settings::SourcingSettingsDto;
use lattice_core::application::contracts::web::*;
use lattice_core::application::ports::{SourceFinding, WebServiceTrait};
use lattice_core::shared::constants::WEB_REQUEST_TIMEOUT;
use lattice_core::shared::error::{AppError, Result};
use reqwest::Client;
use std::collections::BTreeSet;
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::Semaphore;

mod address_guard;
mod bounded;
mod pacing;
mod page;
mod search;

pub(crate) use address_guard::is_blocked_ip;
pub(crate) use address_guard::validate_url_resolved;
use address_guard::{
    check_url_host, redirect_policy, refuse_blocked_addresses, PublicOnlyResolver,
};
pub use bounded::{BoundedBody, BoundedFetchError};
pub(crate) use pacing::{SitePacer, MIN_SITE_REQUEST_INTERVAL};
use page::{parse_fetchable_url, Want};

/// Extracted text shorter than this is a shell — a title, a menu, a cookie
/// notice — and the page is worth reading in the browser. Matches the chat
/// pipeline's floor for a page worth quoting.
const THIN_PAGE_CHARS: usize = 400;

/// `<article>` text is used only when it is at least 1/N of the page's
/// paragraph text; below that it is cards or teasers, not the body.
const ARTICLE_SHARE_DENOMINATOR: usize = 3;

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
    /// A reference capture: full-document extraction, never served from or
    /// written to the chat cache, and no browser fallback.
    Reference,
}
/// Maximum decoded bytes accepted from a page before HTML parsing.
pub(crate) const MAX_PAGE_BODY_BYTES: usize = 5 * 1024 * 1024;
/// Number of HTML parses admitted at once across requests.
const MAX_HTML_PARSERS: usize = 2;
const RESPONSE_BODY_TIMEOUT: Duration = Duration::from_secs(30);
const HTML_PARSE_TIMEOUT: Duration = Duration::from_secs(15);
/// The page cache's folder inside the app data directory.
const PAGE_CACHE_DIR_NAME: &str = "web-cache";

/// Web service implementation: search through source finding, and page reads
/// with SSRF guards, per-site pacing, a page cache and the browser fallback.
pub struct WebService {
    /// The guarded public client (no cookie jar, no proxy).
    client: Client,
    /// Per-site request queue shared by page reads and every finder.
    pacer: Arc<SitePacer>,
    /// Pages and settled refusals kept across turns, on disk.
    page_cache: PageCache,
    /// Blocking HTML parsers admitted at once. The permit lives in the
    /// blocking closure, including when its async caller is cancelled.
    html_parsers: Arc<Semaphore>,
    /// The hidden browser a page that refuses the HTTP client is read in.
    /// The app attaches it once it is running; until then such a page keeps
    /// its HTTP result.
    browser: OnceLock<Arc<BrowserReader>>,
    /// Routed, fused source finding over the registered finders, with their
    /// health and the provider-page cache.
    sourcing: Arc<SourcingService>,
    /// The Internet Archive: the last resort for a page nothing else reads,
    /// and dated reads (design §3.8).
    wayback: Wayback,
}

/// A page read for display, which is a fetch plus the one thing the tool
/// output has no room for: when the text was actually read. A cache hit is
/// served instantly and could otherwise only claim to have been read "now".
#[derive(Debug)]
pub struct ReadPage {
    pub output: FetchUrlContentOutput,
    pub fetched_at: DateTime<Utc>,
}

impl WebService {
    /// Create the web service with the default request timeout.
    ///
    /// `data_dir` is the app data directory; the page cache lives in a
    /// `web-cache/` folder under it and is created on first use.
    pub fn new(data_dir: &Path) -> Result<Self> {
        Self::with_timeout(WEB_REQUEST_TIMEOUT, data_dir)
    }

    /// Create with a custom request timeout. The keyless finders are
    /// registered under the default sourcing settings; keyed finders and the
    /// user's switches arrive with the sourcing settings (design §3.14).
    pub fn with_timeout(timeout: Duration, data_dir: &Path) -> Result<Self> {
        let client = build_public_client(timeout)?;
        let pacer = Arc::new(SitePacer::default());
        let html_parsers = Arc::new(Semaphore::new(MAX_HTML_PARSERS));
        let settings = SourcingSettingsDto::default();
        let http = FinderHttp {
            client: client.clone(),
            pacer: Arc::clone(&pacer),
            parsers: Arc::clone(&html_parsers),
            contact: settings.contact().map(str::to_owned),
        };
        let wayback = Wayback::new(http.clone(), settings.clone());
        let sourcing = Arc::new(SourcingService::new(
            finders::keyless_registry(&http),
            settings,
            BTreeSet::new(),
        ));
        Ok(Self {
            client,
            pacer,
            page_cache: PageCache::new(data_dir.join(PAGE_CACHE_DIR_NAME)),
            html_parsers,
            browser: OnceLock::new(),
            sourcing,
            wayback,
        })
    }

    /// The same service over a given source-finding service, for tests that
    /// script the finders.
    #[cfg(test)]
    pub(crate) fn with_sourcing(data_dir: &Path, sourcing: Arc<SourcingService>) -> Result<Self> {
        Ok(Self {
            sourcing,
            ..Self::new(data_dir)?
        })
    }

    /// Read refused pages through `browser` from now on. The first reader
    /// attached stays; the app has one.
    pub fn attach_browser(&self, browser: Arc<BrowserReader>) {
        let _ = self.browser.set(browser);
    }

    /// Source finding as callers hold it: the same finders, health and
    /// provider-page cache `search_web` uses.
    pub fn source_finding(&self) -> Arc<dyn SourceFinding> {
        Arc::clone(&self.sourcing) as Arc<dyn SourceFinding>
    }
}

/// The one client for reading public web pages, with the default request
/// timeout: no cookie jar, the private-address redirect policy and the
/// public-only resolver. The snapshot engine fetches through it.
pub(crate) fn public_web_client() -> Result<Client> {
    build_public_client(WEB_REQUEST_TIMEOUT)
}

/// The single builder behind [`WebService`] and [`public_web_client`].
fn build_public_client(timeout: Duration) -> Result<Client> {
    identity::client_builder()
        .timeout(timeout)
        .redirect(redirect_policy())
        .dns_resolver(Arc::new(PublicOnlyResolver))
        .build()
        .map_err(|e| AppError::InternalError(format!("Failed to create HTTP client: {}", e)))
}

/// Parse `url` and accept only `http`/`https`: the same check page reads use.
pub(crate) fn fetchable_url(url: &str) -> Result<url::Url> {
    parse_fetchable_url(url)
}

/// A response body read under `max_bytes` and this service's body deadline,
/// decoded by its declared charset. For the finders, which share the page
/// reader's limits.
pub(crate) async fn read_text_bounded(
    response: reqwest::Response,
    max_bytes: usize,
) -> std::result::Result<String, String> {
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body = page::read_bounded_body(response, max_bytes).await?;
    Ok(page::decode_response_text(body, content_type.as_deref()))
}

/// Run a blocking HTML parse under the shared parser admission and its time
/// budget. For the finders that parse result pages.
pub(crate) async fn parse_html<T, F>(parsers: Arc<Semaphore>, operation: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    page::run_html_parser(parsers, operation).await
}

#[async_trait]
impl WebServiceTrait for WebService {
    async fn search_web(&self, input: &WebSearchInput) -> Result<WebSearchOutput> {
        self.search_through_sourcing(input).await
    }

    async fn fetch_url_content(&self, url: &str) -> Result<FetchUrlContentOutput> {
        self.read_page(url).await.map(|read| read.output)
    }

    async fn fetch_reference_content(&self, url: &str) -> Result<FetchUrlContentOutput> {
        let page = self
            .read_page_as(url, PageRead::Reference, Want::Live)
            .await?;
        if page.output.content.trim().len() < THIN_PAGE_CHARS {
            return Err(AppError::InvalidInput("This page did not provide enough static reference text. Supply a readable chapter, document, or pasted text.".into()));
        }
        Ok(page.output)
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
