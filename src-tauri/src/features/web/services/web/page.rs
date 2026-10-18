//! Reading one page: bounded download, charset decoding, parsing and the browser fallback.
use super::address_guard::validate_url_resolved;
use super::{
    FetchUrlContentOutput, PageRead, ReadPage, WebService, ARTICLE_SHARE_DENOMINATOR,
    HTML_PARSE_TIMEOUT, MAX_PAGE_BODY_BYTES, RESPONSE_BODY_TIMEOUT, THIN_PAGE_CHARS,
};
use crate::features::web::services::page_cache::{CachedFetch, CachedPage};
use crate::features::web::services::{browser_reader, identity};
use chrono::Utc;
use encoding_rs::{Encoding, UTF_8};
use lattice_core::shared::error::{AppError, Result};
use reqwest::StatusCode;
use scraper::{Html, Selector};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};
use url::Url;

/// Parse one bounded HTML document and derive both the title and readable
/// body from the same tree.
fn extract_page_parts(html: &str) -> (Option<String>, String) {
    let document = Html::parse_document(html);
    let title = Selector::parse("title")
        .ok()
        .and_then(|selector| document.select(&selector).next())
        .map(|element| element.text().collect::<String>().trim().to_string());

    let (Ok(article_selector), Ok(p_selector)) = (
        Selector::parse("article"),
        Selector::parse("p, h1, h2, h3, h4, h5, h6"),
    ) else {
        return (title, String::new());
    };
    let blocks = |root: scraper::ElementRef<'_>| -> Vec<String> {
        root.select(&p_selector)
            .map(|p| p.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .filter(|text| !text.is_empty())
            .collect()
    };
    let article_parts: Vec<String> = document
        .select(&article_selector)
        .flat_map(&blocks)
        .collect();
    let page_parts = blocks(document.root_element());
    let chars = |parts: &[String]| parts.iter().map(String::len).sum::<usize>();
    let parts = if !article_parts.is_empty()
        && chars(&article_parts) * ARTICLE_SHARE_DENOMINATOR >= chars(&page_parts)
    {
        article_parts
    } else {
        page_parts
    };
    (title, parts.join("\n\n"))
}

/// Read a transparently decoded response body under a byte and wall-clock
/// budget. Content-Length is only an early rejection hint; the stream limit
/// also covers chunked and compressed bodies.
pub(super) async fn read_bounded_body(
    response: reqwest::Response,
    max_bytes: usize,
) -> std::result::Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        return Err(format!("response declares more than {max_bytes} bytes"));
    }
    tokio::time::timeout(RESPONSE_BODY_TIMEOUT, async move {
        let mut response = response;
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
            if body.len().saturating_add(chunk.len()) > max_bytes {
                return Err(format!("response exceeded {max_bytes} decoded bytes"));
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    })
    .await
    .map_err(|_| {
        format!(
            "response body exceeded {:?} deadline",
            RESPONSE_BODY_TIMEOUT
        )
    })?
}

pub(super) fn decode_response_text(body: Vec<u8>, content_type: Option<&str>) -> String {
    let header_encoding = content_type
        .and_then(|value| {
            value.split(';').find_map(|parameter| {
                let (name, value) = parameter.trim().split_once('=')?;
                name.trim()
                    .eq_ignore_ascii_case("charset")
                    .then_some(value.trim().trim_matches(|ch| ch == '"' || ch == '\''))
            })
        })
        .and_then(|label| Encoding::for_label(label.as_bytes()))
        .unwrap_or(UTF_8);
    let (encoding, bom_len) = Encoding::for_bom(&body).unwrap_or((header_encoding, 0));
    encoding
        .decode(body.get(bom_len..).unwrap_or_default())
        .0
        .into_owned()
}

pub(super) async fn run_html_parser<T, F>(parsers: Arc<Semaphore>, operation: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    tokio::time::timeout(HTML_PARSE_TIMEOUT, async move {
        let permit = parsers
            .acquire_owned()
            .await
            .map_err(|_| AppError::InternalError("HTML parser admission is closed".into()))?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            operation()
        })
        .await
        .map_err(|error| AppError::InternalError(format!("HTML parser task failed: {error}")))
    })
    .await
    .map_err(|_| AppError::Network("HTML parsing exceeded its time budget".into()))?
}

/// Parse `url` and accept only what this service is willing to request.
///
/// Split out of [`WebService::validate_url`] because a page the cache already
/// holds still has to clear this bar — a `file://` URL must not become
/// readable just because something once wrote it into the cache — while the
/// DNS resolution behind it would be pointless work for a request that is
/// never sent.
pub(super) fn parse_fetchable_url(url: &str) -> Result<Url> {
    let parsed =
        Url::parse(url).map_err(|e| AppError::InvalidUrl(format!("Invalid URL: {}", e)))?;
    match parsed.scheme() {
        "http" | "https" => Ok(parsed),
        scheme => Err(AppError::InvalidUrl(format!(
            "Unsupported URL scheme: {}. Only http and https are allowed.",
            scheme
        ))),
    }
}

impl WebService {
    /// Read `url`, from the page cache when it holds it and from the network
    /// otherwise.
    ///
    /// A hit skips the pacer and the DNS check as well as the request: the
    /// text is already here, so there is no host to be polite to and no
    /// address to re-resolve. Only the scheme is still checked, because a
    /// `file://` URL must not be readable by any path.
    pub async fn read_page(&self, url: &str) -> Result<ReadPage> {
        self.read_page_as(url, PageRead::Chat).await
    }

    pub(super) async fn read_page_as(&self, url: &str, read: PageRead) -> Result<ReadPage> {
        debug!("Fetching URL: {}", url);
        parse_fetchable_url(url)?;

        let lookup_start = Instant::now();
        // Reference snapshots have their own durable storage. Never return a
        // short chat-cache entry as a complete book or overwrite that cache.
        let cached = if read == PageRead::Chat {
            self.page_cache.get(url).await
        } else {
            None
        };
        match cached {
            Some(CachedFetch::Page(page)) => {
                info!(url, words = page.word_count, "URL served from page cache");
                let fetched_at = page.fetched_at;
                return Ok(ReadPage {
                    output: FetchUrlContentOutput {
                        url: page.final_url,
                        title: page.title,
                        content: page.content,
                        content_truncated: page.content_truncated,
                        word_count: page.word_count,
                        fetch_time_ms: lookup_start.elapsed().as_secs_f64() * 1000.0,
                        content_type: page.content_type,
                        from_cache: true,
                        read_via: Default::default(),
                    },
                    fetched_at,
                });
            }
            Some(CachedFetch::Refusal { reason }) => {
                info!(url, %reason, "URL refusal served from page cache");
                return Err(AppError::Network(format!(
                    "{reason} (this site refused earlier; not retried)"
                )));
            }
            None => {}
        }

        // Validate URL for security. The client's resolver and redirect
        // policy hold every later hop to the same rule; this first check is
        // here for the clearer error.
        validate_url_resolved(url).await?;

        // Paced per site, like the searches: repeat requests to one host are
        // spaced and never overlap, and a host this process has not contacted
        // is contacted at once. This used to be a blind 0.5–2s sleep before
        // every fetch, which a host seeing us for the first time cannot even
        // observe — it only made the user wait, once per page per round.
        let _site_slot = self.pacer.acquire(url).await;

        let start = Instant::now();
        let profile = identity::random_profile();
        let headers = identity::browser_headers(profile, None);

        let mut response = self
            .client
            .get(url)
            .headers(headers.clone())
            .send()
            .await
            .map_err(|e| AppError::Network(format!("Failed to fetch URL: {}", e)))?;

        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            let retry_after_secs = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(2)
                .min(5);
            debug!(
                url,
                retry_after_secs, "Rate limited by host; retrying once after delay"
            );
            tokio::time::sleep(std::time::Duration::from_secs(retry_after_secs)).await;
            if let Ok(retry_resp) = self.client.get(url).headers(headers).send().await {
                response = retry_resp;
            }
        }

        if !response.status().is_success() {
            let status = response.status();
            warn!("URL fetch failed with status: {}", status);
            // 403, 429 and 503 are what a JavaScript or rate-limiting check answers a plain client
            // with. The page may open in a browser, so it is tried there
            // before the refusal is believed.
            if matches!(
                status,
                StatusCode::FORBIDDEN
                    | StatusCode::SERVICE_UNAVAILABLE
                    | StatusCode::TOO_MANY_REQUESTS
            ) {
                if let Some(page) = self.read_through_browser(url, start, read).await {
                    return Ok(page);
                }
            }
            // Remembered only when the status means the answer is settled, so
            // the next turn does not spend a request finding out again.
            self.page_cache
                .remember_refusal(url, status, &format!("HTTP {status}"))
                .await;
            return Err(AppError::Network(format!(
                "HTTP {}: {}",
                status,
                status.canonical_reason().unwrap_or("Unknown")
            )));
        }

        // Every hop to here was cleared by the redirect policy and the
        // resolver before it was requested.
        let final_url = response.url().to_string();
        if final_url != url {
            debug!("URL redirected to: {}", final_url);
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let html = read_bounded_body(response, MAX_PAGE_BODY_BYTES)
            .await
            .map_err(|e| AppError::Network(format!("Failed to read response body: {e}")))?;
        let html = decode_response_text(html, content_type.as_deref());

        let (title, content) = if read == PageRead::Chat {
            self.parse_page(html).await?
        } else {
            let extraction_url = final_url.clone();
            run_html_parser(self.html_parsers.clone(), move || {
                super::super::reference_text::extract_with_url(&html, Some(&extraction_url))
            })
            .await??
        };

        // A page whose text is written by its scripts arrives as a shell: a
        // title and a menu. The browser runs the scripts; its text is kept
        // only when it is actually more.
        if content.trim().len() < THIN_PAGE_CHARS {
            if let Some(page) = self.read_through_browser(url, start, read).await {
                if page.output.content.trim().len() > content.trim().len() {
                    return Ok(page);
                }
            }
        }

        Ok(self
            .keep_page(url, final_url, title, content, content_type, (start, read))
            .await)
    }

    /// Read `url` in a hidden browser window, and cache and return it if that
    /// produced text. `None` leaves the HTTP result standing.
    async fn read_through_browser(
        &self,
        url: &str,
        start: Instant,
        page_read: PageRead,
    ) -> Option<ReadPage> {
        let read_result = match self.browser.get() {
            Some(browser) => {
                browser
                    .read(url, browser_reader::DEFAULT_READ_TIMEOUT)
                    .await
            }
            None => Err(browser_reader::unavailable()),
        };
        match read_result {
            Ok(page) if !page.text.trim().is_empty() => {
                // The browser follows redirects the HTTP client never saw.
                if page.final_url != url && validate_url_resolved(&page.final_url).await.is_err() {
                    warn!(
                        url,
                        final_url = page.final_url.as_str(),
                        "Browser read ended somewhere not allowed; discarded"
                    );
                    return None;
                }
                Some(
                    self.keep_page(
                        url,
                        page.final_url,
                        page.title,
                        page.text,
                        Some("text/html".to_string()),
                        (start, page_read),
                    )
                    .await,
                )
            }
            Ok(_) => {
                info!(url, "Browser read found no text either");
                None
            }
            Err(reason) => {
                info!(url, %reason, "Browser read did not help");
                None
            }
        }
    }

    /// Count, cache and wrap a page's extracted text, all of it.
    pub(super) async fn keep_page(
        &self,
        url: &str,
        final_url: String,
        title: Option<String>,
        content: String,
        content_type: Option<String>,
        budget: (Instant, PageRead),
    ) -> ReadPage {
        let (start, read) = budget;
        let fetch_time_ms = start.elapsed().as_secs_f64() * 1000.0;

        let word_count = content.split_whitespace().count();

        info!(
            "URL fetched: {} words in {:.2}ms",
            word_count, fetch_time_ms
        );

        let fetched_at = Utc::now();
        if read == PageRead::Chat {
            self.page_cache
                .remember_page(
                    url,
                    &CachedPage {
                        final_url: final_url.clone(),
                        title: title.clone(),
                        content: content.clone(),
                        content_truncated: false,
                        word_count,
                        content_type: content_type.clone(),
                        fetched_at,
                    },
                )
                .await;
        }

        ReadPage {
            output: FetchUrlContentOutput {
                url: final_url,
                title,
                content,
                content_truncated: false,
                word_count,
                fetch_time_ms,
                content_type,
                from_cache: false,
                read_via: Default::default(),
            },
            fetched_at,
        }
    }

    /// Extract main text content from HTML
    ///
    /// Attempts to extract readable article text by:
    /// 1. Finding <article> elements
    /// 2. Extracting text from <p>, <h1>-<h6> tags
    /// 3. Cleaning whitespace
    #[cfg(test)]
    pub(super) fn extract_article_text(&self, html: &str) -> String {
        extract_page_parts(html).1
    }

    async fn parse_page(&self, html: String) -> Result<(Option<String>, String)> {
        run_html_parser(self.html_parsers.clone(), move || extract_page_parts(&html)).await
    }

    /// Extract title from HTML. Kept for the focused parser tests.
    #[cfg(test)]
    pub(super) fn extract_title(&self, html: &str) -> Option<String> {
        extract_page_parts(html).0
    }
}
