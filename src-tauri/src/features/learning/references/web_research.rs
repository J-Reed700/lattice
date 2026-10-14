//! Web search and page capture shared by outline reference gathering and
//! lesson research. Search snippets never become evidence: only a complete
//! capture of a page, validated for full text, is returned.
use crate::application::contracts::web::{WebSearchInput, WebSearchResult};
use crate::features::learning::source_library::CapturedLearningSource;
use crate::features::web::traits::WebServiceTrait;
use crate::shared::error::{AppError, Result};

/// Search providers may ignore site operators (including enrichment providers).
/// Enforce the requested host/path ourselves, again after any fetch redirect.
pub(in crate::features::learning) fn within_search_scope(query: &str, address: &str) -> bool {
    let scopes: Vec<_> = query
        .split_whitespace()
        .filter_map(|term| term.strip_prefix("site:"))
        .map(|scope| scope.trim_matches(['\'', '"', '(', ')']))
        .collect();
    if scopes.is_empty() {
        return true;
    }
    let Ok(address) = url::Url::parse(address) else {
        return false;
    };
    scopes.iter().any(|scope| {
        let normalized = if scope.contains("://") {
            scope.to_string()
        } else {
            format!("https://{scope}")
        };
        let Ok(scope) = url::Url::parse(&normalized) else {
            return false;
        };
        let (Some(host), Some(expected)) = (address.host_str(), scope.host_str()) else {
            return false;
        };
        let path = scope.path().trim_end_matches('/');
        (host == expected || host.ends_with(&format!(".{expected}")))
            && (path.is_empty()
                || address.path() == path
                || address.path().starts_with(&format!("{path}/")))
    })
}

/// One public search. Results outside a `site:` scope in the query are dropped.
/// A provider failure is returned so the caller can log it and move on.
pub(in crate::features::learning) async fn search(
    web: &dyn WebServiceTrait,
    query: &str,
) -> Result<Vec<WebSearchResult>> {
    let input: WebSearchInput = serde_json::from_value(serde_json::json!({
        "query":query,"max_results":12,"depth":1,
        "providers":["duckduckgo","bing"],"include_wikipedia":false
    }))?;
    let output = web.search_web(&input).await?;
    Ok(output
        .results
        .into_iter()
        .filter(|result| within_search_scope(query, &result.url))
        .collect())
}

/// Fetch one selected result as a complete reference capture.
///
/// `Err` is a transport failure (the page could not be fetched at all);
/// `Ok(None)` is a page that was fetched but is unusable as evidence: it left
/// the query's `site:` scope through a redirect, has no readable text, or was
/// truncated by the extractor.
pub(in crate::features::learning) async fn capture(
    web: &dyn WebServiceTrait,
    result: &WebSearchResult,
    query: &str,
) -> Result<Option<CapturedLearningSource>> {
    let article = web
        .fetch_reference_content(&result.url)
        .await
        .map_err(|error| AppError::Network(format!("{}: {error}", result.url)))?;
    if !within_search_scope(query, &article.url) {
        return Ok(None);
    }
    let captured = CapturedLearningSource {
        title: article
            .title
            .unwrap_or_else(|| result.title.clone())
            .chars()
            .take(180)
            .collect(),
        publisher: None,
        requested_url: Some(result.url.clone()),
        resolved_url: Some(article.url),
        text: article.content,
        truncated: article.content_truncated,
        extraction_version: crate::features::web::services::REFERENCE_TEXT_EXTRACTION_VERSION
            .into(),
    };
    if crate::features::learning::sources::validate_capture(&captured).is_err() {
        return Ok(None);
    }
    Ok(Some(captured))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn site_scopes_are_enforced_by_host_and_path_after_redirects() {
        assert!(within_search_scope(
            "site:docs.example.org/manual indexes",
            "https://docs.example.org/manual/indexes.html"
        ));
        assert!(within_search_scope(
            "site:example.org indexes",
            "https://docs.example.org/anything"
        ));
        assert!(!within_search_scope(
            "site:docs.example.org/manual indexes",
            "https://docs.example.org/blog/indexes.html"
        ));
        assert!(!within_search_scope(
            "site:example.org indexes",
            "https://example.org.evil.test/page"
        ));
        assert!(within_search_scope("plain query", "https://anywhere.test/"));
    }
}
