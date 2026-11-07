//! A scripted web service for tests.

#![allow(clippy::expect_used)]

use super::web_service::NO_PROVENANCE_READER;
use super::WebServiceTrait;
use crate::application::contracts::web::{
    FetchUrlContentOutput, FinderOutcomeDto, ProvenanceSignals, ReferencePage, WebSearchInput,
    WebSearchOutput, WebSearchResult,
};
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Mock web service for testing
pub struct MockWebService {
    search_results: Arc<RwLock<Vec<WebSearchResult>>>,
    search_outcomes: Arc<RwLock<Vec<FinderOutcomeDto>>>,
    url_contents: Arc<RwLock<HashMap<String, FetchUrlContentOutput>>>,
    page_signals: Arc<RwLock<HashMap<String, ProvenanceSignals>>>,
}

impl MockWebService {
    pub fn new() -> Self {
        Self {
            search_results: Arc::new(RwLock::new(Vec::new())),
            search_outcomes: Arc::new(RwLock::new(Vec::new())),
            url_contents: Arc::new(RwLock::new(HashMap::new())),
            page_signals: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set the provenance signals `fetch_reference_page` returns for `url`.
    /// A URL without any gets `NotExtracted`, as a fetcher without a
    /// metadata reader would.
    pub fn set_page_signals(&self, url: impl Into<String>, signals: ProvenanceSignals) {
        self.page_signals
            .write()
            .expect("Mock lock poisoned")
            .insert(url.into(), signals);
    }

    /// Set canned web search results
    pub fn set_search_results(&self, results: Vec<WebSearchResult>) {
        *self.search_results.write().expect("Mock lock poisoned") = results;
    }

    /// Set the finder outcomes every search reports, as a search whose
    /// providers partly failed would.
    pub fn set_search_outcomes(&self, outcomes: Vec<FinderOutcomeDto>) {
        *self.search_outcomes.write().expect("Mock lock poisoned") = outcomes;
    }

    /// Set canned URL content
    pub fn set_url_content(&self, url: impl Into<String>, content: FetchUrlContentOutput) {
        self.url_contents
            .write()
            .expect("Mock lock poisoned")
            .insert(url.into(), content);
    }
}

impl Default for MockWebService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WebServiceTrait for MockWebService {
    async fn search_web(&self, input: &WebSearchInput) -> Result<WebSearchOutput> {
        let results = self
            .search_results
            .read()
            .expect("Mock lock poisoned")
            .clone();
        let page = input.page.max(1);
        let max_results = input.max_results.max(1);
        let effective_offset = input
            .offset
            .saturating_add(page.saturating_sub(1).saturating_mul(max_results));
        let total_results = results.len();
        let start = effective_offset.min(total_results);
        let end = (start + max_results).min(total_results);
        let page_results = results.into_iter().skip(start).take(end - start).collect();

        Ok(WebSearchOutput {
            results: page_results,
            query: input.query.clone(),
            result_count: end.saturating_sub(start),
            page,
            offset: effective_offset,
            total_results,
            has_more: end < total_results,
            providers_used: if input.providers.is_empty() {
                vec!["mock".to_string()]
            } else {
                input.providers.clone()
            },
            unique_query_count: 1,
            unique_url_count: total_results,
            unique_domain_count: total_results,
            followup_queries: Vec::new(),
            outcomes: self
                .search_outcomes
                .read()
                .expect("Mock lock poisoned")
                .clone(),
            owner: None,
        })
    }

    async fn fetch_url_content(&self, url: &str) -> Result<FetchUrlContentOutput> {
        let contents = self.url_contents.read().expect("Mock lock poisoned");
        if let Some(content) = contents.get(url) {
            Ok(content.clone())
        } else {
            Ok(FetchUrlContentOutput {
                url: url.to_string(),
                title: Some("Mock Page".to_string()),
                content: "Mock content for testing".to_string(),
                content_truncated: false,
                word_count: 4,
                fetch_time_ms: 10.0,
                content_type: Some("text/html".to_string()),
                from_cache: false,
                read_via: Default::default(),
            })
        }
    }

    async fn fetch_reference_page(&self, url: &str) -> Result<ReferencePage> {
        let output = self.fetch_reference_content(url).await?;
        let signals = self
            .page_signals
            .read()
            .expect("Mock lock poisoned")
            .get(url)
            .cloned()
            .unwrap_or_else(|| ProvenanceSignals::NotExtracted {
                reason: NO_PROVENANCE_READER.to_string(),
            });
        Ok(ReferencePage {
            output,
            signals,
            fetched_at: chrono::Utc::now().timestamp(),
        })
    }

    fn validate_url(&self, url: &str) -> Result<()> {
        if url.contains("localhost") || url.contains("127.0.0.1") {
            return Err(AppError::InvalidUrl(
                "localhost not allowed".to_string(),
            ));
        }
        Ok(())
    }
}
