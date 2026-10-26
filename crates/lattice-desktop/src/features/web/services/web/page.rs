//! Reading one page: bounded download, charset decoding, parsing, and the
//! fetch chain's fallbacks: the browser reader, then the Internet Archive.
use super::address_guard::validate_url_resolved;
use super::{
    FetchUrlContentOutput, PageRead, ReadPage, WebService, ARTICLE_SHARE_DENOMINATOR,
    HTML_PARSE_TIMEOUT, MAX_PAGE_BODY_BYTES, RESPONSE_BODY_TIMEOUT, THIN_PAGE_CHARS,
};
use crate::features::web::services::finders::retry_after;
use crate::features::web::services::page_cache::{CachedFetch, CachedPage};
use crate::features::web::services::provenance::{
    self, PageMetadata, PageProvenance, PageReading, TextForm,
};
use crate::features::web::services::{browser_reader, identity, reference_text};
use chrono::{NaiveDate, Utc};
use encoding_rs::{Encoding, UTF_8};
use lattice_core::application::contracts::web::{HttpFacts, ProvenanceSignals, ReadVia};
use lattice_core::domain::sourcing::unwrap_archive;
use lattice_core::shared::error::{AppError, Result};
use reqwest::StatusCode;
use scraper::{Html, Selector};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};
use url::Url;

/// The content type a browser read is recorded with: the browser rendered
/// an HTML document and does not expose the response's own header.
const BROWSER_CONTENT_TYPE: &str = "text/html";

/// A page's title, readable body and metadata, from one parse of its HTML.
fn read_chat_html(html: &str, page_url: &Url) -> (Option<String>, String, PageMetadata) {
    let document = Html::parse_document(html);
    let (title, body) = page_parts(&document);
    (title, body, provenance::read_metadata(&document, page_url))
}

/// A page's reference markdown and metadata.
fn read_reference_html(
    html: &str,
    page_url: &Url,
) -> Result<(Option<String>, String, PageMetadata)> {
    let (title, content) = reference_text::extract_with_url(html, Some(page_url.as_str()))?;
    let metadata = provenance::read_metadata(&Html::parse_document(html), page_url);
    Ok((title, content, metadata))
}

/// The elements a chat page read takes its text from. Paragraphs and headings
/// alone dropped a page's lists, tables, code and quotes without a word, and
/// the read was still reported whole: a changelog lost its versions, an
/// answer its code.
const READ_BLOCKS: &str = "p, h1, h2, h3, h4, h5, h6, li, pre, blockquote, figcaption, dt, dd, tr";

fn is_read_block(name: &str) -> bool {
    matches!(
        name,
        "p" | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "li"
            | "pre"
            | "blockquote"
            | "figcaption"
            | "dt"
            | "dd"
            | "tr"
    )
}

/// Whether a block sits inside another, whose text already holds its own, or
/// is a list or table row of the page's chrome: a menu is a `nav` of list
/// items, and a footer's links are one too. Paragraphs and headings there
/// were always read, and still are.
fn has_block_ancestor(element: scraper::ElementRef<'_>) -> bool {
    let structural = !matches!(
        element.value().name(),
        "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
    );
    element
        .ancestors()
        .filter_map(scraper::ElementRef::wrap)
        .any(|ancestor| {
            let name = ancestor.value().name();
            is_read_block(name) || (structural && matches!(name, "nav" | "footer" | "aside"))
        })
}

/// A block's text: code as written, a table row's cells side by side, and
/// anything else as one line of words.
fn block_text(element: scraper::ElementRef<'_>) -> String {
    match element.value().name() {
        "pre" => element.text().collect::<String>().trim().to_string(),
        "tr" => element
            .children()
            .filter_map(scraper::ElementRef::wrap)
            .map(|cell| cell.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .filter(|cell| !cell.is_empty())
            .collect::<Vec<_>>()
            .join(" | "),
        _ => element
            .text()
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string(),
    }
}

/// Derive both the title and readable body from one parsed document.
fn page_parts(document: &Html) -> (Option<String>, String) {
    let title = Selector::parse("title")
        .ok()
        .and_then(|selector| document.select(&selector).next())
        .map(|element| element.text().collect::<String>().trim().to_string());

    let (Ok(article_selector), Ok(block_selector)) =
        (Selector::parse("article"), Selector::parse(READ_BLOCKS))
    else {
        return (title, String::new());
    };
    let blocks = |root: scraper::ElementRef<'_>| -> Vec<String> {
        root.select(&block_selector)
            .filter(|block| !has_block_ancestor(*block))
            .map(block_text)
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

/// Which copy of a page a read wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Want {
    /// The live page; the archive only when nothing else reads it.
    Live,
    /// The archived copy nearest this day, instead of the live page.
    AsOf(NaiveDate),
}

/// Why the live page gave nothing, as the error names it.
enum LiveFailure {
    Status(StatusCode),
    Transport(String),
}

impl LiveFailure {
    fn describe(&self) -> String {
        match self {
            LiveFailure::Status(status) => format!(
                "HTTP {}: {}",
                status,
                status.canonical_reason().unwrap_or("Unknown")
            ),
            LiveFailure::Transport(detail) => format!("Failed to fetch URL: {detail}"),
        }
    }
}

/// What reading the live page came to.
enum Live {
    /// A page worth returning.
    Read(ReadPage),
    /// Only a shell (kept and cached): the archive may hold more text.
    Thin(ReadPage),
    Failed(LiveFailure),
}

/// A page's extracted text and how it was read, before it is counted and
/// cached.
pub(super) struct PageText {
    pub(super) final_url: String,
    pub(super) title: Option<String>,
    pub(super) content: String,
    pub(super) content_type: Option<String>,
    pub(super) via: ReadVia,
}

/// A direct read of an archive URL is an archived read, and says so.
fn direct_read_via(final_url: &str) -> ReadVia {
    Url::parse(final_url)
        .ok()
        .and_then(|url| unwrap_archive(&url))
        .and_then(|copy| {
            copy.captured_at.map(|captured_at| ReadVia::Archive {
                captured_at,
                original: copy.original.to_string(),
            })
        })
        .unwrap_or(ReadVia::Direct)
}

fn served_from_cache(page: CachedPage, lookup_start: Instant) -> ReadPage {
    ReadPage {
        output: FetchUrlContentOutput {
            url: page.final_url,
            title: page.title,
            content: page.content,
            content_truncated: page.content_truncated,
            word_count: page.word_count,
            fetch_time_ms: lookup_start.elapsed().as_secs_f64() * 1000.0,
            content_type: page.content_type,
            from_cache: true,
            read_via: page.read_via,
        },
        fetched_at: page.fetched_at,
        signals: ProvenanceSignals::NotExtracted {
            reason: provenance::FROM_PAGE_CACHE.to_string(),
        },
    }
}

fn cached_page(page: &ReadPage) -> CachedPage {
    CachedPage {
        final_url: page.output.url.clone(),
        title: page.output.title.clone(),
        content: page.output.content.clone(),
        content_truncated: page.output.content_truncated,
        word_count: page.output.word_count,
        content_type: page.output.content_type.clone(),
        fetched_at: page.fetched_at,
        read_via: page.output.read_via.clone(),
    }
}

fn text_len(page: &ReadPage) -> usize {
    page.output.content.trim().len()
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
        self.read_page_as(url, PageRead::Chat, Want::Live).await
    }

    /// Read the Internet Archive's copy of `url` nearest `as_of`, never the
    /// live page. `read_via` names the capture and its date.
    pub async fn read_page_as_of(&self, url: &str, as_of: NaiveDate) -> Result<ReadPage> {
        self.read_page_as(url, PageRead::Chat, Want::AsOf(as_of))
            .await
    }

    /// The fetch chain (design §3.8): cache → guard → pacer → direct GET →
    /// the browser reader for refusals and shells → the Internet Archive when
    /// still nothing. A dated read goes to the archive alone.
    pub(super) async fn read_page_as(
        &self,
        url: &str,
        read: PageRead,
        want: Want,
    ) -> Result<ReadPage> {
        debug!("Fetching URL: {}", url);
        let parsed = parse_fetchable_url(url)?;
        if let Want::AsOf(day) = want {
            return self.read_dated(&parsed, day, read).await;
        }

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
                return Ok(served_from_cache(page, lookup_start));
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
        // here for the clearer error. A URL that fails it is never sent to
        // the archive either.
        validate_url_resolved(url).await?;

        let start = Instant::now();
        // The live read holds the site's pacer slot; the archive steps run
        // after it is released (archive.org has a slot of its own, and a
        // page on archive.org must not wait on itself).
        match self.read_live(url, start, read).await? {
            Live::Read(page) => Ok(page),
            Live::Thin(page) => Ok(self
                .thicker_from_archive(url, &parsed, page, start, read)
                .await),
            Live::Failed(failure) => {
                self.rescue_or_refuse(url, &parsed, failure, start, read)
                    .await
            }
        }
    }

    /// One paced direct GET, and the browser reader for refusals and shells.
    async fn read_live(&self, url: &str, start: Instant, read: PageRead) -> Result<Live> {
        // Paced per site, like the searches: repeat requests to one host are
        // spaced and never overlap, and a host this process has not contacted
        // is contacted at once.
        let _site_slot = self.pacer.acquire(url).await;

        let profile = identity::random_profile();
        let headers = identity::browser_headers(profile, None);
        let mut response = match self.client.get(url).headers(headers.clone()).send().await {
            Ok(response) => response,
            Err(error) => return Ok(Live::Failed(LiveFailure::Transport(error.to_string()))),
        };

        // A host that says when to come back is asked once more after that
        // wait, in full (§3.6); a 429 without one goes on to the browser and
        // the archive. The caller's own deadline drops the read if the wait
        // outlasts it.
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            if let Some(wait) = retry_after(response.headers(), Utc::now()) {
                debug!(
                    url,
                    ?wait,
                    "Rate limited by host; retrying once after its wait"
                );
                tokio::time::sleep(wait).await;
                if let Ok(retried) = self.client.get(url).headers(headers).send().await {
                    response = retried;
                }
            }
        }

        let status = response.status();
        if !status.is_success() {
            warn!("URL fetch failed with status: {}", status);
            // 403, 429 and 503 are what a JavaScript or rate-limiting check
            // answers a plain client with. The page may open in a browser, so
            // it is tried there before the refusal is believed.
            if matches!(
                status,
                StatusCode::FORBIDDEN
                    | StatusCode::SERVICE_UNAVAILABLE
                    | StatusCode::TOO_MANY_REQUESTS
            ) {
                if let Some(page) = self
                    .read_through_browser(url, start, read, status.as_u16())
                    .await
                {
                    return Ok(Live::Read(page));
                }
            }
            return Ok(Live::Failed(LiveFailure::Status(status)));
        }

        // Every hop to here was cleared by the redirect policy and the
        // resolver before it was requested.
        let page_url = response.url().clone();
        let final_url = page_url.to_string();
        if final_url != url {
            debug!("URL redirected to: {}", final_url);
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let http = HttpFacts {
            status: response.status().as_u16(),
            content_type: content_type.clone(),
            last_modified: response
                .headers()
                .get(reqwest::header::LAST_MODIFIED)
                .and_then(|value| value.to_str().ok())
                .and_then(provenance::parse_http_date),
            via_browser_reader: false,
        };

        let body = read_bounded_body(response, MAX_PAGE_BODY_BYTES)
            .await
            .map_err(|e| AppError::Network(format!("Failed to read response body: {e}")))?;
        let kind = body::body_kind(content_type.as_deref(), &body);
        let (title, content, metadata, text_form) = if kind == body::BodyKind::Html {
            let html = decode_response_text(body, content_type.as_deref());
            self.extract(html, page_url, read).await?
        } else {
            let text = body::body_text(kind, body, content_type.as_deref()).await?;
            (None, text, PageMetadata::default(), TextForm::Plain)
        };
        let thin = content.trim().len() < THIN_PAGE_CHARS;

        // A page whose text is written by its scripts arrives as a shell: a
        // title and a menu. The browser runs the scripts; its text is kept
        // only when it is actually more. A PDF or a text file has no scripts.
        if thin && kind == body::BodyKind::Html {
            if let Some(page) = self
                .read_through_browser(url, start, read, http.status)
                .await
            {
                if page.output.content.trim().len() > content.trim().len() {
                    return Ok(if text_len(&page) < THIN_PAGE_CHARS {
                        Live::Thin(page)
                    } else {
                        Live::Read(page)
                    });
                }
            }
        }

        let via = direct_read_via(&final_url);
        let text = PageText {
            final_url,
            title,
            content,
            content_type,
            via,
        };
        let provenance = PageProvenance::Read(Box::new(PageReading {
            metadata,
            http,
            text: text_form,
        }));
        let page = self.keep_page(url, text, (start, read), provenance).await;
        Ok(if thin {
            Live::Thin(page)
        } else {
            Live::Read(page)
        })
    }

    /// Read `url` in a hidden browser window, and cache and return it if that
    /// produced text. `None` leaves the HTTP result standing. `http_status` is
    /// what the HTTP client was answered, recorded with the page's signals.
    async fn read_through_browser(
        &self,
        url: &str,
        start: Instant,
        page_read: PageRead,
        http_status: u16,
    ) -> Option<ReadPage> {
        match self.browser_read(url, page_read, http_status).await {
            Ok(page) if !page.content.trim().is_empty() => {
                // The browser follows redirects the HTTP client never saw.
                if page.final_url != url && validate_url_resolved(&page.final_url).await.is_err() {
                    warn!(
                        url,
                        final_url = page.final_url.as_str(),
                        "Browser read ended somewhere not allowed; discarded"
                    );
                    return None;
                }
                let text = PageText {
                    final_url: page.final_url,
                    title: page.title,
                    content: page.content,
                    content_type: Some(BROWSER_CONTENT_TYPE.to_string()),
                    via: ReadVia::Browser,
                };
                Some(
                    self.keep_page(url, text, (start, page_read), page.provenance)
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

    /// A conversation's browser read takes the browser's paragraph text; a
    /// reference read takes the rendered document and runs the same
    /// extraction and metadata reader as an HTTP read on it.
    async fn browser_read(
        &self,
        url: &str,
        page_read: PageRead,
        http_status: u16,
    ) -> Result<BrowserRead> {
        let browser = self.browser.get().ok_or_else(browser_reader::unavailable)?;
        if page_read == PageRead::Chat {
            let page = browser
                .read(url, browser_reader::DEFAULT_READ_TIMEOUT)
                .await?;
            return Ok(BrowserRead {
                final_url: page.final_url,
                title: page.title,
                content: page.text,
                provenance: PageProvenance::NotRead(provenance::BROWSER_TEXT_ONLY),
            });
        }
        let page = browser
            .read_document(url, browser_reader::DEFAULT_READ_TIMEOUT)
            .await?;
        let page_url = Url::parse(&page.final_url)
            .map_err(|e| AppError::InvalidUrl(format!("browser page address: {e}")))?;
        let html = page.html;
        let (title, content, metadata) = run_html_parser(self.html_parsers.clone(), move || {
            read_reference_html(&html, &page_url)
        })
        .await??;
        let http = HttpFacts {
            status: http_status,
            content_type: Some(BROWSER_CONTENT_TYPE.to_string()),
            last_modified: None,
            via_browser_reader: true,
        };
        Ok(BrowserRead {
            final_url: page.final_url,
            title: page.title.or(title),
            content,
            provenance: PageProvenance::Read(Box::new(PageReading {
                metadata,
                http,
                text: TextForm::Markdown,
            })),
        })
    }

    /// Title, text and metadata, by what the page is read for: article
    /// extraction for a conversation, the whole document for a reference.
    async fn extract(
        &self,
        html: String,
        page_url: Url,
        read: PageRead,
    ) -> Result<(Option<String>, String, PageMetadata, TextForm)> {
        if read == PageRead::Chat {
            let (title, content, metadata) =
                run_html_parser(self.html_parsers.clone(), move || {
                    read_chat_html(&html, &page_url)
                })
                .await?;
            return Ok((title, content, metadata, TextForm::Plain));
        }
        let (title, content, metadata) = run_html_parser(self.html_parsers.clone(), move || {
            read_reference_html(&html, &page_url)
        })
        .await??;
        Ok((title, content, metadata, TextForm::Markdown))
    }

    /// Count, cache and wrap a page's extracted text, all of it, with the
    /// signals `provenance` yields for it.
    pub(super) async fn keep_page(
        &self,
        url: &str,
        text: PageText,
        budget: (Instant, PageRead),
        provenance: PageProvenance,
    ) -> ReadPage {
        let (start, read) = budget;
        let fetch_time_ms = start.elapsed().as_secs_f64() * 1000.0;
        let PageText {
            final_url,
            title,
            content,
            content_type,
            via,
        } = text;

        let word_count = content.split_whitespace().count();

        info!(
            "URL fetched: {} words in {:.2}ms",
            word_count, fetch_time_ms
        );

        let fetched_at = Utc::now();
        let signals = provenance.into_signals(&final_url, &content, fetched_at.timestamp());
        let page = ReadPage {
            output: FetchUrlContentOutput {
                url: final_url,
                title,
                content,
                content_truncated: false,
                word_count,
                fetch_time_ms,
                content_type,
                from_cache: false,
                read_via: via,
            },
            fetched_at,
            signals,
        };
        if read == PageRead::Chat {
            self.page_cache
                .remember_page(url, &cached_page(&page))
                .await;
        }
        page
    }

    /// Extract main text content from HTML
    ///
    /// Attempts to extract readable article text by:
    /// 1. Finding <article> elements
    /// 2. Extracting text from paragraphs, headings, lists, tables, code,
    ///    quotes and definitions ([`READ_BLOCKS`])
    /// 3. Cleaning whitespace
    #[cfg(test)]
    pub(super) fn extract_article_text(&self, html: &str) -> String {
        page_parts(&Html::parse_document(html)).1
    }

    /// Extract title from HTML. Kept for the focused parser tests.
    #[cfg(test)]
    pub(super) fn extract_title(&self, html: &str) -> Option<String> {
        page_parts(&Html::parse_document(html)).0
    }
}

/// What a browser read produced, before it is kept.
struct BrowserRead {
    final_url: String,
    title: Option<String>,
    content: String,
    provenance: PageProvenance,
}

#[path = "page_archive.rs"]
mod archive;

#[path = "page_body.rs"]
mod body;

#[cfg(test)]
#[path = "page_tests.rs"]
mod archive_tests;
