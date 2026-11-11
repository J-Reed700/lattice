//! The HTTP capture flow (§6.2 to §6.5): guard and fetch the document,
//! decode it, pass 1, fetch the subresources, pass 2, assemble the page.
//! The whole capture runs under one deadline and stops on cancellation.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use encoding_rs::{Encoding, UTF_16BE, UTF_16LE, UTF_8};
use reqwest::Client;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use url::Url;

use super::resources::{
    charset_label, content_type, mime_essence, read_capped, Fetcher, ReadError,
};
use super::sanitize::{collect, sanitize_with_resources};
use super::{CAPTURE_DEADLINE, MAX_DOCUMENT_BYTES};
use crate::features::web::services::identity::{self, BrowserProfile};
use crate::features::web::services::web::{fetchable_url, validate_url_resolved};
use crate::features::web::traits::page_snapshot::{
    CaptureError, CaptureMethod, CaptureProgress, CaptureRequest, CapturedPage, PageCapturePort,
    ProgressSink,
};

/// How far into the document to look for `<meta charset>` (the HTML prescan window).
const META_SNIFF_BYTES: usize = 1024;

/// Captures over plain HTTP through the guarded public web client.
pub(super) struct HttpPageCapture {
    client: Client,
    browser: OnceLock<Arc<crate::features::web::services::browser_reader::BrowserReader>>,
}

impl HttpPageCapture {
    pub(super) fn new(client: Client) -> Self {
        Self {
            client,
            browser: OnceLock::new(),
        }
    }
}

#[async_trait]
impl PageCapturePort for HttpPageCapture {
    async fn capture(
        &self,
        request: CaptureRequest,
        progress: ProgressSink,
        cancel: CancellationToken,
    ) -> std::result::Result<CapturedPage, CaptureError> {
        let deadline = Instant::now() + CAPTURE_DEADLINE;
        tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(CaptureError::Cancelled),
            result = self.run(request, progress, deadline) => result,
        }
    }

    fn attach_browser(
        &self,
        browser: Arc<crate::features::web::services::browser_reader::BrowserReader>,
    ) {
        let _ = self.browser.set(browser);
    }
}

/// The decoded document and where it ended up after redirects.
struct Document {
    html: String,
    final_url: Url,
    status: u16,
}

impl HttpPageCapture {
    async fn run(
        &self,
        request: CaptureRequest,
        progress: ProgressSink,
        deadline: Instant,
    ) -> Result<CapturedPage, CaptureError> {
        let url = fetchable_url(&request.url).map_err(|e| CaptureError::Refused(e.to_string()))?;
        let profile = identity::random_profile();
        let doc_result =
            match tokio::time::timeout_at(deadline, self.fetch_document(&url, profile)).await {
                Ok(res) => res,
                Err(_) => Err(CaptureError::Timeout),
            };

        let should_try_browser = match &doc_result {
            Ok(doc) => needs_browser_escalation(doc),
            Err(
                CaptureError::Blocked { .. } | CaptureError::Timeout | CaptureError::Network(_),
            ) => true,
            Err(_) => false,
        };

        let (html, final_url, method, dom_title) = if should_try_browser {
            if let Some(browser) = self.browser.get() {
                report(&progress, 0, 0, "Rendering in browser…");
                let now = Instant::now();
                let remaining = if deadline > now {
                    deadline - now
                } else {
                    Duration::from_secs(0)
                };
                let remaining = remaining
                    .max(Duration::from_secs(5))
                    .min(crate::features::web::services::browser_reader::DEFAULT_READ_TIMEOUT);
                let dom_outcome = browser.read_dom(url.as_str(), remaining).await;
                match dom_outcome {
                    Ok(dom_page) => {
                        if let Ok(final_url) = Url::parse(&dom_page.final_url) {
                            (
                                dom_page.html,
                                final_url,
                                CaptureMethod::Browser,
                                dom_page.title,
                            )
                        } else {
                            match doc_result {
                                Ok(doc) => {
                                    if doc.html.trim().is_empty() {
                                        return Err(CaptureError::Blocked { status: doc.status });
                                    }
                                    (doc.html, doc.final_url, CaptureMethod::Http, None)
                                }
                                Err(err) => return Err(err),
                            }
                        }
                    }
                    Err(_) => match doc_result {
                        Ok(doc) => {
                            if doc.html.trim().is_empty() {
                                return Err(CaptureError::Blocked { status: doc.status });
                            }
                            (doc.html, doc.final_url, CaptureMethod::Http, None)
                        }
                        Err(err) => return Err(err),
                    },
                }
            } else {
                match doc_result {
                    Ok(doc) => {
                        if doc.status == 202 && needs_browser_escalation(&doc) {
                            return Err(CaptureError::Blocked { status: 202 });
                        }
                        if doc.html.trim().is_empty() {
                            return Err(CaptureError::Blocked { status: doc.status });
                        }
                        (doc.html, doc.final_url, CaptureMethod::Http, None)
                    }
                    Err(err) => return Err(err),
                }
            }
        } else {
            let doc = doc_result?;
            (doc.html, doc.final_url, CaptureMethod::Http, None)
        };

        let collected = {
            let (html, final_url) = (html.clone(), final_url.clone());
            blocking(move || collect(&html, &final_url)).await?
        };
        report(&progress, 0, collected.refs.len() as u32, "Read the page");
        let fetcher = Fetcher::new(
            &self.client,
            profile,
            final_url.to_string(),
            progress.clone(),
        );
        let favicon_url = collected
            .favicon
            .clone()
            .or_else(|| default_favicon(&final_url));
        let (outcome, favicon) = tokio::join!(
            fetcher.fetch_all(collected.refs, deadline),
            fetcher.fetch_favicon(favicon_url, deadline),
        );
        let total = outcome.resources.report();
        report(
            &progress,
            total.inlined,
            total.inlined + total.dropped.len() as u32,
            "Freezing the page",
        );

        let captured_at = Utc::now();
        let sanitized = {
            let (request, final_url) = (request.clone(), final_url.clone());
            let resources = outcome.resources;
            blocking(move || {
                sanitize_with_resources(&html, &request, &final_url, &resources, captured_at)
            })
            .await?
        };
        let mut partial_reasons = outcome.partial_reasons;
        partial_reasons.extend(sanitized.partial_reasons);
        partial_reasons.sort();
        partial_reasons.dedup();
        let title = dom_title.or(collected.title);
        Ok(CapturedPage {
            requested_url: request.url,
            final_url: final_url.to_string(),
            title,
            html: sanitized.html,
            favicon,
            method,
            resources: total,
            partial: !partial_reasons.is_empty(),
            partial_reasons,
            captured_at,
        })
    }

    /// §6.2: the guard, then a GET; only 2xx HTML is captured, read as a
    /// stream up to 16 MiB and decoded to UTF-8.
    async fn fetch_document(
        &self,
        url: &Url,
        profile: &'static BrowserProfile,
    ) -> Result<Document, CaptureError> {
        validate_url_resolved(url.as_str())
            .await
            .map_err(|e| CaptureError::Refused(e.to_string()))?;
        let mut response = self
            .client
            .get(url.clone())
            .headers(identity::browser_headers(profile, None))
            .send()
            .await
            .map_err(send_error)?;
        if response.status().as_u16() == 429 {
            let delay_secs = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok())
                .unwrap_or(2)
                .min(5);
            tokio::time::sleep(Duration::from_secs(delay_secs)).await;
            response = self
                .client
                .get(url.clone())
                .headers(identity::browser_headers(profile, None))
                .send()
                .await
                .map_err(send_error)?;
        }
        let status = response.status().as_u16();
        check_status(status)?;
        let declared = content_type(&response);
        let mime = mime_essence(declared.as_deref());
        if mime != "text/html" && mime != "application/xhtml+xml" {
            let mime = if mime.is_empty() {
                "unknown".to_string()
            } else {
                mime
            };
            return Err(CaptureError::UnsupportedContentType { mime });
        }
        let final_url = response.url().clone();
        let body = read_capped(response, MAX_DOCUMENT_BYTES)
            .await
            .map_err(|e| match e {
                ReadError::TooLarge => CaptureError::TooLarge,
                ReadError::Network(message) => CaptureError::Network(message),
            })?;
        Ok(Document {
            html: decode_document(&body, declared.as_deref()),
            final_url,
            status,
        })
    }
}

fn strip_tags_and_scripts(html: &str) -> String {
    // ASCII lowercasing keeps every byte offset, so `html`'s char offsets
    // index `lower` at the same boundaries.
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len() / 2);
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;
    for (i, ch) in html.char_indices() {
        let opens =
            |marker: &str| ch == '<' && lower.get(i..).is_some_and(|rest| rest.starts_with(marker));
        if in_script {
            if opens("</script") {
                in_script = false;
                in_tag = true;
            }
            continue;
        }
        if in_style {
            if opens("</style") {
                in_style = false;
                in_tag = true;
            }
            continue;
        }
        if ch == '<' {
            if opens("<script") {
                in_script = true;
            } else if opens("<style") {
                in_style = true;
            }
            in_tag = true;
            continue;
        }
        if in_tag {
            if ch == '>' {
                in_tag = false;
            }
            continue;
        }
        out.push(ch);
    }
    out
}

fn needs_browser_escalation(doc: &Document) -> bool {
    if doc.html.trim().is_empty() {
        return true;
    }
    let vis_len = {
        let text = strip_tags_and_scripts(&doc.html);
        text.split_whitespace()
            .map(|w| w.len() + 1)
            .sum::<usize>()
            .saturating_sub(1)
    };
    if doc.status == 202 && vis_len < 400 {
        return true;
    }
    let lower_html = doc.html.to_ascii_lowercase();
    let challenge_markers = [
        "just a moment",
        "attention required",
        "checking your browser",
        "verify you are human",
        "cf-browser-verification",
        "id=\"challenge-running\"",
        "id='challenge-running'",
        "name=\"cf-turnstile-response\"",
        "name='cf-turnstile-response'",
    ];
    let has_challenge_marker = challenge_markers
        .iter()
        .any(|marker| lower_html.contains(marker));
    if has_challenge_marker && vis_len < 1200 {
        return true;
    }

    let spa_roots = [
        "id=\"__next\"",
        "id='__next'",
        "id=\"root\"",
        "id='root'",
        "id=\"app\"",
        "id='app'",
        "data-reactroot",
    ];
    let has_spa_root = spa_roots.iter().any(|root| lower_html.contains(root));
    if has_spa_root && lower_html.contains("<script") && vis_len < 80 {
        return true;
    }

    false
}

/// 403, 429 and 503 are `Blocked` (a browser may pass); other non-2xx is refused.
pub(super) fn check_status(status: u16) -> Result<(), CaptureError> {
    match status {
        200..=299 => Ok(()),
        403 | 429 | 503 => Err(CaptureError::Blocked { status }),
        _ => Err(CaptureError::Refused(format!("the site answered {status}"))),
    }
}

fn send_error(error: reqwest::Error) -> CaptureError {
    if error.is_timeout() {
        CaptureError::Timeout
    } else if error.is_redirect() {
        CaptureError::Refused(error.to_string())
    } else {
        CaptureError::Network(error.to_string())
    }
}

/// Decode with the BOM, else the `Content-Type` charset, else a
/// `<meta charset>` / `http-equiv` prescan, else UTF-8 (lossy).
pub(super) fn decode_document(bytes: &[u8], declared: Option<&str>) -> String {
    let encoding = charset_label(declared)
        .or_else(|| {
            sniff_meta_charset(
                bytes
                    .get(..bytes.len().min(META_SNIFF_BYTES))
                    .unwrap_or(bytes),
            )
        })
        .map(|encoding| {
            if encoding == UTF_16LE || encoding == UTF_16BE {
                UTF_8
            } else {
                encoding
            }
        })
        .unwrap_or(UTF_8);
    // `decode` lets a BOM override the label.
    encoding.decode(bytes).0.into_owned()
}

/// The first `charset=` inside a `<meta …>` tag in `head`.
fn sniff_meta_charset(head: &[u8]) -> Option<&'static Encoding> {
    let text = String::from_utf8_lossy(head).to_ascii_lowercase();
    text.match_indices("<meta").find_map(|(start, _)| {
        let tag = &text[start..];
        let tag = &tag[..tag.find('>').unwrap_or(tag.len())];
        let value = tag[tag.find("charset")? + 7..]
            .trim_start()
            .strip_prefix('=')?
            .trim_start();
        let value = value.trim_start_matches(['"', '\'']);
        let end = value
            .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '.')))
            .unwrap_or(value.len());
        Encoding::for_label(value.get(..end).unwrap_or(value).as_bytes())
    })
}

/// `/favicon.ico` at the page's origin, when no icon link names one.
fn default_favicon(page: &Url) -> Option<String> {
    page.join("/favicon.ico").ok().map(|url| url.to_string())
}

fn report(progress: &ProgressSink, done: u32, total: u32, message: &str) {
    progress(CaptureProgress {
        done,
        total,
        message: message.to_string(),
    });
}

/// Run CPU-bound lol_html work off the async workers.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, CaptureError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| CaptureError::Refused(format!("the sanitizer stopped: {e}")))
}
