//! Mock implementations for testing
//!
//! This module provides mock implementations of service traits.

#[cfg(test)]
use crate::features::web::traits::web::{WebIngestionResult, WebIngestionServiceTrait};
#[cfg(test)]
use async_trait::async_trait;
#[cfg(test)]
use lattice_core::application::contracts::web::{
    FetchUrlContentOutput, FinderOutcomeDto, ProvenanceSignals, ReferencePage, WebSearchInput,
    WebSearchOutput, WebSearchResult,
};
#[cfg(test)]
use lattice_core::application::ports::web_service::NO_PROVENANCE_READER;
#[cfg(test)]
use lattice_core::application::ports::WebServiceTrait;
#[cfg(test)]
use lattice_core::shared::error::Result;
#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::sync::{Arc, RwLock};

#[cfg(test)]
/// Mock web ingestion service for testing
///
/// Simulates web content ingestion without actual HTTP requests.
/// Allows configuration of responses and tracking of ingestion attempts.
/// All operations are deterministic and thread-safe.
pub struct MockWebIngestionService {
    ingested_urls: Arc<RwLock<Vec<String>>>,
    mock_results: Arc<RwLock<std::collections::HashMap<String, WebIngestionResult>>>,
}

#[cfg(test)]
impl MockWebIngestionService {
    /// Create new mock service with empty state
    pub fn new() -> Self {
        Self {
            ingested_urls: Arc::new(RwLock::new(Vec::new())),
            mock_results: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Configure mock to return specific result for a URL
    ///
    /// When `ingest_url` is called with this URL, the configured result will be returned.
    ///
    /// # Arguments
    /// * `url` - The URL to match
    /// * `result` - The WebIngestionResult to return
    ///
    /// # Example
    /// ```rust
    /// let service = MockWebIngestionService::new();
    /// service.set_result_for_url(
    ///     "https://example.com",
    ///     WebIngestionResult {
    ///         document_id: "custom-id".to_string(),
    ///         url: "https://example.com".to_string(),
    ///         title: "Custom Title".to_string(),
    ///         word_count: 500,
    ///         chunks_created: 10,
    ///         site_name: Some("Example".to_string()),
    ///         author: None,
    ///         reading_time_minutes: Some(3),
    ///     }
    /// );
    /// ```
    pub fn set_result_for_url(&self, url: &str, result: WebIngestionResult) {
        self.mock_results
            .write()
            .expect("Mock lock poisoned")
            .insert(url.to_string(), result);
    }

    /// Get list of URLs that were ingested
    ///
    /// Returns a clone of all URLs that have been ingested via `ingest_url`.
    /// Useful for verifying that expected URLs were processed.
    ///
    /// # Returns
    /// Vector of URLs in the order they were ingested
    pub fn get_ingested_urls(&self) -> Vec<String> {
        self.ingested_urls
            .read()
            .expect("Mock lock poisoned")
            .clone()
    }

    /// Clear all ingested URLs and configured results
    ///
    /// Resets the mock to initial empty state.
    pub fn clear(&self) {
        self.ingested_urls
            .write()
            .expect("Mock lock poisoned")
            .clear();
        self.mock_results
            .write()
            .expect("Mock lock poisoned")
            .clear();
    }
}

#[cfg(test)]
impl Default for MockWebIngestionService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
#[cfg(test)]
impl WebIngestionServiceTrait for MockWebIngestionService {
    async fn ingest_url(&self, url: &str) -> Result<WebIngestionResult> {
        // Track ingestion attempt
        self.ingested_urls
            .write()
            .expect("Mock lock poisoned")
            .push(url.to_string());

        let results = self.mock_results.read().expect("Mock lock poisoned");
        if let Some(result) = results.get(url) {
            Ok(result.clone())
        } else {
            // Default mock result
            Ok(WebIngestionResult {
                document_id: format!("mock-doc-{}", uuid::Uuid::new_v4()),
                url: url.to_string(),
                title: "Mock Web Document".to_string(),
                word_count: 250,
                chunks_created: 5,
                site_name: Some("Mock Site".to_string()),
                author: Some("Mock Author".to_string()),
                reading_time_minutes: Some(2),
            })
        }
    }
}

/// Mock search enrichment service for testing
///
/// Simulates search result enrichment without database queries.
/// Allows configuration of metadata and tracking of enrichment requests.
/// All operations are deterministic and thread-safe.
#[cfg(test)]
/// Mock web service for testing
pub struct MockWebService {
    search_results: Arc<RwLock<Vec<WebSearchResult>>>,
    search_outcomes: Arc<RwLock<Vec<FinderOutcomeDto>>>,
    url_contents: Arc<RwLock<HashMap<String, FetchUrlContentOutput>>>,
    page_signals: Arc<RwLock<HashMap<String, ProvenanceSignals>>>,
}

#[cfg(test)]
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

#[cfg(test)]
impl Default for MockWebService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
#[cfg(test)]
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
            return Err(lattice_core::shared::error::AppError::InvalidUrl(
                "localhost not allowed".to_string(),
            ));
        }
        Ok(())
    }
}

/// A finder that answers from a script, for the sourcing runner's tests:
/// `per_page` leads on every page (on its own host, ranked as a real engine
/// ranks them), after any queued failures, or one failure on every call.
#[cfg(test)]
pub struct ScriptedFinder {
    descriptor: lattice_core::domain::sourcing::FinderDescriptor,
    per_page: usize,
    queued:
        std::sync::Mutex<std::collections::VecDeque<lattice_core::domain::sourcing::FailureCause>>,
    always: Option<lattice_core::domain::sourcing::FailureCause>,
    calls: std::sync::Mutex<Vec<(String, usize)>>,
}

#[cfg(test)]
impl ScriptedFinder {
    /// Answers `per_page` leads on every page.
    pub fn answering(
        descriptor: lattice_core::domain::sourcing::FinderDescriptor,
        per_page: usize,
    ) -> Self {
        Self {
            descriptor,
            per_page,
            queued: std::sync::Mutex::new(std::collections::VecDeque::new()),
            always: None,
            calls: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Fails every call with `cause`.
    pub fn failing(
        descriptor: lattice_core::domain::sourcing::FinderDescriptor,
        cause: lattice_core::domain::sourcing::FailureCause,
    ) -> Self {
        Self {
            always: Some(cause),
            ..Self::answering(descriptor, 0)
        }
    }

    /// Fails the next calls with these causes, one each, before answering.
    pub fn failing_first(self, causes: Vec<lattice_core::domain::sourcing::FailureCause>) -> Self {
        *self.queued.lock().expect("Mock lock poisoned") = causes.into();
        self
    }

    /// Every call so far: the query it was sent and the page asked for.
    pub fn calls(&self) -> Vec<(String, usize)> {
        self.calls.lock().expect("Mock lock poisoned").clone()
    }
}

#[cfg(test)]
#[async_trait]
impl lattice_core::application::ports::SourceFinder for ScriptedFinder {
    fn descriptor(&self) -> &lattice_core::domain::sourcing::FinderDescriptor {
        &self.descriptor
    }

    async fn find(
        &self,
        need: &lattice_core::domain::sourcing::SourceNeed,
        page: usize,
        _budget: &lattice_core::application::ports::Budget,
    ) -> lattice_core::domain::sourcing::FinderOutcome {
        use lattice_core::domain::sourcing::{FinderOutcome, FinderResult, Lead};
        self.calls
            .lock()
            .expect("Mock lock poisoned")
            .push((need.query.clone(), page));
        let id = self.descriptor.id.clone();
        let queued = self.queued.lock().expect("Mock lock poisoned").pop_front();
        if let Some(cause) = queued.or_else(|| self.always.clone()) {
            return FinderOutcome {
                finder: id,
                leads: Vec::new(),
                result: FinderResult::Failed(cause),
                elapsed: std::time::Duration::ZERO,
            };
        }
        let slug: String = need
            .query
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let leads: Vec<Lead> = (0..self.per_page)
            .map(|index| {
                let rank = (page - 1) * self.per_page + index + 1;
                let url = url::Url::parse(&format!("https://{id}.example/{slug}/{rank}"))
                    .expect("scripted lead URL");
                Lead {
                    title: Some(format!("{id} {rank}")),
                    ..Lead::page(url, id.clone(), rank)
                }
            })
            .collect();
        FinderOutcome {
            finder: id,
            result: if leads.is_empty() {
                FinderResult::Empty
            } else {
                FinderResult::Answered
            },
            leads,
            elapsed: std::time::Duration::ZERO,
        }
    }
}
