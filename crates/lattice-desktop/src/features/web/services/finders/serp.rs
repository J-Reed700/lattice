//! Reading a search engine's HTML result page: result links, their titles
//! and snippets, the engines' click-tracking redirects, and the challenge
//! pages an engine serves a client it suspects is a bot.

use std::collections::HashSet;

use base64::prelude::{Engine as _, BASE64_URL_SAFE_NO_PAD};
use lattice_core::domain::sourcing::canonical::{self, CanonicalKey};
use lattice_core::domain::sourcing::{FinderId, Lead};
use scraper::{ElementRef, Html, Selector};
use url::Url;

/// One result as the page lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SerpHit {
    pub title: String,
    pub url: Url,
    pub snippet: String,
}

impl SerpHit {
    /// The hit as a lead `finder` found at `rank`.
    pub(crate) fn into_lead(self, finder: FinderId, rank: usize) -> Lead {
        Lead {
            title: Some(self.title),
            snippet: (!self.snippet.is_empty()).then_some(self.snippet),
            ..Lead::page(self.url, finder, rank)
        }
    }
}

/// A result title shorter than this, found only by the any-anchor fallback,
/// is navigation ("Next", "Images"), not a result.
const MIN_FALLBACK_TITLE_CHARS: usize = 6;

/// True when the page is an engine's bot challenge rather than results:
/// DuckDuckGo's "anomaly" modal or Bing's automated-query refusal. Such a
/// page is never retried in-process (retrying a challenge is what gets an
/// address blocked); the finder reports `Challenge` and its breaker decides.
pub(crate) fn is_challenge_page(html: &str) -> bool {
    let lower = html.to_ascii_lowercase();
    (lower.contains("anomaly-modal") && lower.contains("bots use duckduckgo"))
        || (lower.contains("403 - forbidden") && lower.contains("automated queries"))
}

/// Normalizes an extracted search-result href into the URL of the page it
/// points at, in two steps: make it absolute, then unwrap a search engine's
/// redirect, so a protocol-relative `//duckduckgo.com/l/?uddg=...` is
/// unwrapped too.
pub(crate) fn normalize_search_url(raw: &str) -> Option<String> {
    let absolute = absolutize_search_url(raw.trim())?;
    Some(unwrap_redirect_url(&absolute).unwrap_or(absolute))
}

/// Turn an href into an absolute URL, without interpreting it further.
fn absolutize_search_url(trimmed: &str) -> Option<String> {
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return Some(trimmed.to_string());
    }
    if let Some(rest) = trimmed.strip_prefix("//") {
        return Some(format!("https://{}", rest));
    }
    if trimmed.starts_with("/ck/a?") {
        return Some(format!("https://www.bing.com{}", trimmed));
    }
    if trimmed.starts_with("/l/?") {
        return Some(format!("https://duckduckgo.com{}", trimmed));
    }
    // Host/path-like values.
    if trimmed.contains('.') && !trimmed.contains(' ') && !trimmed.starts_with('/') {
        return Some(format!(
            "https://{}",
            trimmed.trim_start_matches('/').trim_end_matches('/')
        ));
    }
    None
}

/// The destination behind a search engine's click-tracking redirect, or
/// `None` for an ordinary URL. DuckDuckGo carries it in `uddg` as plain text
/// and Bing in `u` as base64url behind a two-character prefix, so no request
/// is spent on the wrapper.
fn unwrap_redirect_url(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str()?;

    if host.ends_with("duckduckgo.com") && parsed.path() == "/l/" {
        return parsed
            .query_pairs()
            .find(|(key, _)| key == "uddg")
            .map(|(_, value)| value.into_owned())
            .filter(|value| value.starts_with("http://") || value.starts_with("https://"));
    }

    if host.ends_with("bing.com") && parsed.path() == "/ck/a" {
        let raw = parsed
            .query_pairs()
            .find(|(key, _)| key == "u")
            .map(|(_, value)| value.into_owned())?;
        // The payload is base64url with the padding stripped and a short
        // marker in front ("a1" today). Decode what follows the marker and
        // accept it only if it really is a URL.
        let encoded = raw.get(2..)?;
        let decoded = BASE64_URL_SAFE_NO_PAD.decode(encoded).ok()?;
        let decoded = String::from_utf8(decoded).ok()?;
        return Some(decoded)
            .filter(|value| value.starts_with("http://") || value.starts_with("https://"));
    }

    None
}

/// Whether an anchor found by the any-link fallback can be a result: not the
/// engine's own navigation, but a Bing click-through is (it wraps a result).
fn is_result_candidate(url: &Url) -> bool {
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    if host.contains("duckduckgo.com") {
        return false;
    }
    if host.contains("bing.com") {
        return url.path().starts_with("/ck/a");
    }
    true
}

fn collapsed_text(element: ElementRef<'_>) -> String {
    element
        .text()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The results a page lists, at most `max_results`, in page order and
/// without repeats: the engines' result containers first, then their result
/// links anywhere, then any outbound link that looks like a result.
pub(crate) fn parse_results(html: &str, max_results: usize) -> Vec<SerpHit> {
    let document = Html::parse_document(html);
    let container_selector =
        Selector::parse(".result, .results_links, article, .b_algo, li.b_algo").ok();
    let link_selector = Selector::parse(
        "a.result__a, .result__title a, a.result-link, a[data-testid='result-title-a'], .b_algo h2 a",
    )
    .ok();
    let snippet_selector = Selector::parse(
        ".result__snippet, .result-snippet, .result__body, .b_caption p, .b_snippet",
    )
    .ok();

    let mut found = Collector {
        hits: Vec::new(),
        seen: HashSet::new(),
        max_results,
    };

    if let (Some(containers), Some(links)) = (container_selector.as_ref(), link_selector.as_ref()) {
        for element in document.select(containers) {
            if found.is_full() {
                return found.hits;
            }
            let Some(link) = element.select(links).next() else {
                continue;
            };
            let Some(href) = link.value().attr("href") else {
                continue;
            };
            let snippet = snippet_selector
                .as_ref()
                .and_then(|selector| element.select(selector).next())
                .map(collapsed_text)
                .unwrap_or_default();
            found.offer(collapsed_text(link), href, snippet, false);
        }
    }

    if let Some(links) = link_selector.as_ref() {
        for link in document.select(links) {
            if found.is_full() {
                return found.hits;
            }
            if let Some(href) = link.value().attr("href") {
                found.offer(collapsed_text(link), href, String::new(), false);
            }
        }
    }

    if let Ok(any_link) = Selector::parse("a[href]") {
        for link in document.select(&any_link) {
            if found.is_full() {
                return found.hits;
            }
            if let Some(href) = link.value().attr("href") {
                found.offer(collapsed_text(link), href, String::new(), true);
            }
        }
    }

    found.hits
}

/// The results gathered so far, deduplicated by canonical key.
struct Collector {
    hits: Vec<SerpHit>,
    seen: HashSet<CanonicalKey>,
    max_results: usize,
}

impl Collector {
    fn is_full(&self) -> bool {
        self.hits.len() >= self.max_results
    }

    /// Keep a result unless it is unreadable, untitled, a repeat, or (from
    /// the any-link fallback) the engine's own navigation.
    fn offer(&mut self, title: String, href: &str, snippet: String, fallback: bool) {
        let Some(url) = normalize_search_url(href).and_then(|url| Url::parse(&url).ok()) else {
            return;
        };
        if fallback
            && (!is_result_candidate(&url) || title.chars().count() < MIN_FALLBACK_TITLE_CHARS)
        {
            return;
        }
        if title.is_empty() || !self.seen.insert(canonical::key(&url)) {
            return;
        }
        self.hits.push(SerpHit {
            title,
            url,
            snippet,
        });
    }
}

#[cfg(test)]
#[path = "serp_tests.rs"]
mod tests;
