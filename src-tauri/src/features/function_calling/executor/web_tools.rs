//! Web-facing tool handlers: `web_search`, `fetch_url_content`, `wiki_search`
//! and `wiki_summary`.

use super::{FunctionExecutor, WIKIPEDIA_USER_AGENT};
use crate::features::function_calling::domain::FunctionResult;
use crate::features::function_calling::dto::*;
use crate::shared::error::{AppError, Result};
use reqwest::header::{ACCEPT, ACCEPT_LANGUAGE, USER_AGENT};
use tracing::{debug, info};

impl FunctionExecutor {
    /// Execute the web search function.
    ///
    /// Boxed to prevent stack overflow in execute() match dispatch.
    pub(super) fn handle_web_search<'a>(
        &'a self,
        args: &'a serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FunctionResult>> + Send + 'a>>
    {
        Box::pin(async move {
            let input: WebSearchInput = serde_json::from_value(args.clone())
                .map_err(|e| AppError::InvalidInput(format!("Invalid arguments: {}", e)))?;

            debug!(
                "Web search: query='{}', max_results={}, page={}, offset={}, providers={:?}, include_wikipedia={}, depth={}, branch_queries={}",
                input.query,
                input.max_results,
                input.page,
                input.offset,
                input.providers,
                input.include_wikipedia,
                input.depth,
                input.branch_queries
            );

            let output = self.web_service.search_web(&input).await?;

            info!("Web search completed: {} results", output.result_count);

            Ok(FunctionResult::success(serde_json::to_value(output)?))
        }) // Box::pin handle_web_search
    }

    /// Execute the URL-content function.
    ///
    /// Boxed to prevent stack overflow in execute() match dispatch.
    pub(super) fn handle_fetch_url<'a>(
        &'a self,
        args: &'a serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FunctionResult>> + Send + 'a>>
    {
        Box::pin(async move {
            let input: FetchUrlContentInput = serde_json::from_value(args.clone())
                .map_err(|e| AppError::InvalidInput(format!("Invalid arguments: {}", e)))?;

            debug!("Fetch URL: url='{}'", input.url);

            let output = self.web_service.fetch_url_content(&input.url).await?;

            info!(
                "Fetch URL completed: {} words in {:.2}ms",
                output.word_count, output.fetch_time_ms
            );

            Ok(FunctionResult::success(serde_json::to_value(output)?))
        }) // Box::pin handle_fetch_url
    }

    fn strip_html_tags(input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        let mut in_tag = false;
        for ch in input.chars() {
            match ch {
                '<' => in_tag = true,
                '>' => in_tag = false,
                _ if !in_tag => out.push(ch),
                _ => {}
            }
        }
        out.replace("&quot;", "\"")
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&#39;", "'")
    }

    /// Execute wiki_search function.
    pub(super) fn handle_wiki_search<'a>(
        &'a self,
        args: &'a serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FunctionResult>> + Send + 'a>>
    {
        Box::pin(async move {
            let input: WikiSearchInput = serde_json::from_value(args.clone())
                .map_err(|e| AppError::InvalidInput(format!("Invalid arguments: {}", e)))?;

            debug!(
                "Wikipedia search: query='{}', max_results={}",
                input.query, input.max_results
            );

            let limit = input.max_results.clamp(1, 10);
            let api_url = format!(
                "https://en.wikipedia.org/w/api.php?action=query&format=json&utf8=1&list=search&srsearch={}&srlimit={}&srprop=snippet",
                urlencoding::encode(input.query.trim()),
                limit
            );

            self.web_service.validate_url(&api_url)?;

            let response = self
                .http_client
                .get(&api_url)
                .header(ACCEPT, "application/json")
                .header(ACCEPT_LANGUAGE, "en-US,en;q=0.9")
                .header(USER_AGENT, WIKIPEDIA_USER_AGENT)
                .send()
                .await
                .map_err(|e| AppError::Network(format!("Failed to call Wikipedia API: {}", e)))?;

            if !response.status().is_success() {
                return Err(AppError::Network(format!(
                    "Wikipedia API returned HTTP {}",
                    response.status()
                )));
            }

            let payload = response
                .json::<serde_json::Value>()
                .await
                .map_err(|e| AppError::Network(format!("Invalid Wikipedia API response: {}", e)))?;

            let mut results = Vec::new();
            if let Some(items) = payload
                .get("query")
                .and_then(|v| v.get("search"))
                .and_then(|v| v.as_array())
            {
                for item in items.iter().take(limit) {
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
                    let snippet = Self::strip_html_tags(snippet_raw)
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ");
                    let url = format!("https://en.wikipedia.org/wiki/{}", title.replace(' ', "_"));

                    results.push(WikiSearchResult {
                        title,
                        url,
                        snippet,
                    });
                }
            }

            let output = WikiSearchOutput {
                query: input.query,
                result_count: results.len(),
                results,
            };

            Ok(FunctionResult::success(serde_json::to_value(output)?))
        })
    }

    /// Execute wiki_summary function.
    pub(super) fn handle_wiki_summary<'a>(
        &'a self,
        args: &'a serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FunctionResult>> + Send + 'a>>
    {
        Box::pin(async move {
            let input: WikiSummaryInput = serde_json::from_value(args.clone())
                .map_err(|e| AppError::InvalidInput(format!("Invalid arguments: {}", e)))?;

            let title = input.title.trim();
            if title.is_empty() {
                return Err(AppError::InvalidInput(
                    "Wikipedia title cannot be empty".to_string(),
                ));
            }

            debug!("Wikipedia summary: title='{}'", title);

            let encoded_title = title.replace(' ', "_");
            let api_url = format!(
                "https://en.wikipedia.org/api/rest_v1/page/summary/{}",
                urlencoding::encode(&encoded_title)
            );

            self.web_service.validate_url(&api_url)?;

            let response = self
                .http_client
                .get(&api_url)
                .header(ACCEPT, "application/json")
                .header(ACCEPT_LANGUAGE, "en-US,en;q=0.9")
                .header(USER_AGENT, WIKIPEDIA_USER_AGENT)
                .send()
                .await
                .map_err(|e| AppError::Network(format!("Failed to call Wikipedia API: {}", e)))?;

            if !response.status().is_success() {
                return Err(AppError::Network(format!(
                    "Wikipedia summary returned HTTP {}",
                    response.status()
                )));
            }

            let payload = response
                .json::<serde_json::Value>()
                .await
                .map_err(|e| AppError::Network(format!("Invalid Wikipedia API response: {}", e)))?;

            let page_title = payload
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or(title)
                .to_string();
            let page_url = payload
                .get("content_urls")
                .and_then(|v| v.get("desktop"))
                .and_then(|v| v.get("page"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let extract = payload
                .get("extract")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let language = payload
                .get("lang")
                .and_then(|v| v.as_str())
                .unwrap_or("en")
                .to_string();

            let canonical_url = if page_url.is_empty() {
                format!("https://en.wikipedia.org/wiki/{}", encoded_title)
            } else {
                page_url
            };

            let output = WikiSummaryOutput {
                title: page_title,
                url: canonical_url,
                extract,
                language,
            };

            Ok(FunctionResult::success(serde_json::to_value(output)?))
        })
    }
}
