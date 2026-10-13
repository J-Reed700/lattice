//! Parsing search-result pages into canonical, deduplicated result URLs.
use super::page::run_html_parser;
use super::{WebSearchResult, WebService};
use crate::shared::error::Result;
use base64::prelude::{Engine as _, BASE64_URL_SAFE_NO_PAD};
use scraper::{Html, Selector};
use std::collections::HashSet;
use url::{form_urlencoded, Url};

impl WebService {
    pub(super) async fn parse_search_results_bounded(
        &self,
        html: String,
        max_results: usize,
    ) -> Result<Vec<WebSearchResult>> {
        run_html_parser(self.html_parsers.clone(), move || {
            Self::parse_search_results(&html, max_results)
        })
        .await
    }

    /// Normalize an extracted search-result href into the URL of the page it
    /// actually points at.
    ///
    /// Made of two steps on purpose. Making the href absolute and unwrapping a
    /// search engine's redirect used to be tangled together, and a
    /// protocol-relative `//duckduckgo.com/l/?uddg=...` — which is exactly what
    /// DuckDuckGo's HTML endpoint emits — matched the "starts with //" arm
    /// first and was returned as the result, redirect and all. Every fetch of
    /// one of those comes back as an empty page, so the model is handed a
    /// result it can never read and goes looking for the same page again.
    pub(super) fn normalize_search_url(raw: &str) -> Option<String> {
        let absolute = Self::absolutize_search_url(raw.trim())?;
        Some(Self::unwrap_redirect_url(&absolute).unwrap_or(absolute))
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
        // Fallback for host/path-like values.
        if trimmed.contains('.') && !trimmed.contains(' ') && !trimmed.starts_with('/') {
            return Some(format!(
                "https://{}",
                trimmed.trim_start_matches('/').trim_end_matches('/')
            ));
        }
        None
    }

    /// The destination behind a search engine's click-tracking redirect, when
    /// the URL is one. Returns `None` for an ordinary URL, so a caller can keep
    /// what it already had.
    ///
    /// Both engines carry the destination in the link itself, so there is no
    /// reason to spend a request finding out where it goes: DuckDuckGo puts it
    /// in `uddg` as plain text, Bing in `u` as base64url behind a two-character
    /// prefix. Fetching the wrapper instead returns a page of script with no
    /// article text in it.
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

    pub(super) fn is_bot_challenge_page(&self, html: &str) -> bool {
        let lower = html.to_ascii_lowercase();
        (lower.contains("anomaly-modal") && lower.contains("bots use duckduckgo"))
            || (lower.contains("403 - forbidden") && lower.contains("automated queries"))
    }

    fn is_search_result_candidate_url(url: &str) -> bool {
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

    pub(super) fn canonicalize_url_for_dedup(raw: &str) -> String {
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

    pub(super) fn normalized_domain_key(&self, raw_url: &str) -> Option<String> {
        let parsed = Url::parse(raw_url).ok()?;
        let host = parsed.host_str()?.to_ascii_lowercase();
        Some(host.trim_start_matches("www.").to_string())
    }

    pub(super) fn reorder_results_for_domain_diversity(
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

    pub(super) fn parse_search_results(html: &str, max_results: usize) -> Vec<WebSearchResult> {
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
                let Some(url) = Self::normalize_search_url(href) else {
                    continue;
                };
                let canonical = Self::canonicalize_url_for_dedup(&url);
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
                let Some(url) = Self::normalize_search_url(href) else {
                    continue;
                };
                let canonical = Self::canonicalize_url_for_dedup(&url);
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
                let Some(url) = Self::normalize_search_url(href) else {
                    continue;
                };
                let canonical = Self::canonicalize_url_for_dedup(&url);
                if !Self::is_search_result_candidate_url(&url) || !seen.insert(canonical) {
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

    pub(super) fn parse_wikipedia_results(
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
}
