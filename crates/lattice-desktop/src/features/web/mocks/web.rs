//! Mock implementations for testing
//!
//! This module provides mock implementations of service traits.

#[cfg(test)]
use crate::features::web::traits::web::{WebIngestionResult, WebIngestionServiceTrait};
#[cfg(test)]
use async_trait::async_trait;
#[cfg(test)]
pub use lattice_core::application::ports::MockWebService;
#[cfg(test)]
use lattice_core::shared::error::Result;
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

    /// Configures the result `ingest_url` returns for `url`.
    pub fn set_result_for_url(&self, url: &str, result: WebIngestionResult) {
        self.mock_results
            .write()
            .expect("Mock lock poisoned")
            .insert(url.to_string(), result);
    }

    /// The URLs ingested so far, in order.
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
    owner: Option<lattice_core::domain::sourcing::Origin>,
    calls: std::sync::Mutex<Vec<(String, usize)>>,
}

/// A second keyless web engine beside DuckDuckGo, so a wave has another
/// finder to fall back on when one is refused.
#[cfg(test)]
pub fn second_web_engine() -> lattice_core::domain::sourcing::FinderDescriptor {
    lattice_core::domain::sourcing::FinderDescriptor {
        id: lattice_core::domain::sourcing::FinderId::from_static("second_engine"),
        name: "Second engine",
        ..crate::features::web::services::finders::duckduckgo::DESCRIPTOR
    }
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
            owner: None,
            calls: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Every lead asserts `owner`, as a registry or code host record does.
    pub fn asserting_owner(self, owner: lattice_core::domain::sourcing::Origin) -> Self {
        Self {
            owner: Some(owner),
            ..self
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
                    owner: self.owner.clone(),
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
