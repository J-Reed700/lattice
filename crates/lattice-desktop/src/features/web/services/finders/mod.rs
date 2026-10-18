//! Finders: one adapter per service, each with its descriptor (design §3.10).
//!
//! A finder answers one need for one page and never retries across
//! providers; the runner in `super::sourcing` decides what runs, when, and
//! what a failure means for the finder's breaker. Wave 0 holds the three
//! search providers that used to live in `web/providers.rs`.

pub(crate) mod bing_html;
pub(crate) mod duckduckgo;
pub(crate) mod serp;
pub(crate) mod wikipedia;

use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use lattice_core::application::ports::SourceFinder;
use lattice_core::domain::sourcing::{
    FailureCause, FinderDescriptor, FinderOutcome, FinderResult, Lead, SourceNeed,
};
use reqwest::header::{HeaderMap, RETRY_AFTER};
use reqwest::Client;
use tokio::sync::Semaphore;

use super::web::{parse_html, read_text_bounded, SitePacer};

/// What every finder shares with the page reader: the guarded public client,
/// the per-host pacer, the HTML parser admission, and the user's optional
/// polite-pool contact for declared-client calls.
#[derive(Clone)]
pub(crate) struct FinderHttp {
    pub client: Client,
    pub pacer: Arc<SitePacer>,
    pub parsers: Arc<Semaphore>,
    pub contact: Option<String>,
}

/// One response, as a finder needs it.
#[derive(Debug)]
pub(crate) enum Fetched {
    Body(String),
    /// A non-success status, with the host's `Retry-After` when it sent one.
    Status {
        code: u16,
        retry_after: Option<Duration>,
    },
    /// The request or its body read failed.
    Transport(String),
}

impl FinderHttp {
    /// One paced GET. The host's slot covers this request only, never a
    /// retry: what happens next is the runner's decision.
    pub(crate) async fn get(&self, url: &str, headers: HeaderMap, max_bytes: usize) -> Fetched {
        let _site_slot = self.pacer.acquire(url).await;
        let response = match self.client.get(url).headers(headers).send().await {
            Ok(response) => response,
            Err(error) => return Fetched::Transport(error.to_string()),
        };
        let status = response.status();
        if !status.is_success() {
            return Fetched::Status {
                code: status.as_u16(),
                retry_after: retry_after(response.headers(), Utc::now()),
            };
        }
        match read_text_bounded(response, max_bytes).await {
            Ok(text) => Fetched::Body(text),
            Err(error) => Fetched::Transport(error),
        }
    }
}

impl FinderHttp {
    /// Parse a result page under the shared parser admission.
    pub(crate) async fn parse<T, F>(&self, operation: F) -> lattice_core::shared::error::Result<T>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        parse_html(Arc::clone(&self.parsers), operation).await
    }
}

/// The host's `Retry-After`, in either form RFC 9110 §10.2.3 allows:
/// delay-seconds or an HTTP-date (a date already past means "now"). No
/// default and no ceiling: the breaker holds the finder for exactly what the
/// host asked, and the caller's deadline decides whether to wait for it.
pub(crate) fn retry_after(headers: &HeaderMap, now: DateTime<Utc>) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let date = DateTime::parse_from_rfc2822(value)
        .ok()?
        .with_timezone(&Utc);
    Some((date - now).to_std().unwrap_or(Duration::ZERO))
}

/// A registered finder and how many results its service serves per page,
/// which tells the runner whether a page was full (a short page means the
/// service has no more).
#[derive(Clone)]
pub(crate) struct FinderEntry {
    pub finder: Arc<dyn SourceFinder>,
    pub page_size: usize,
}

impl FinderEntry {
    pub(crate) fn descriptor(&self) -> &FinderDescriptor {
        self.finder.descriptor()
    }
}

/// The keyless finders, on by default (design §3.1 rule 6), in the order
/// their leads are listed when they tie.
pub(crate) fn keyless_registry(http: &FinderHttp) -> Vec<FinderEntry> {
    vec![
        FinderEntry {
            finder: Arc::new(duckduckgo::DuckDuckGo::new(http.clone())),
            page_size: duckduckgo::RESULTS_PER_PAGE,
        },
        FinderEntry {
            finder: Arc::new(bing_html::BingHtml::new(http.clone())),
            page_size: bing_html::RESULTS_PER_PAGE,
        },
        FinderEntry {
            finder: Arc::new(wikipedia::Wikipedia::new(http.clone())),
            page_size: wikipedia::RESULTS_PER_PAGE,
        },
    ]
}

/// The query an engine with a native `site:` filter is sent: the need's
/// terms, scoped to the step's origin when it has one.
pub(crate) fn scoped_query(need: &SourceNeed) -> String {
    match &need.scope {
        Some(scope) => format!("site:{scope} {}", need.query.trim()),
        None => need.query.trim().to_string(),
    }
}

/// The 1-based rank of the `index`-th lead on `page` (1-based): a page-2
/// lead ranks after every page-1 lead, so fusion reads it as deeper.
pub(crate) fn rank_of(page: usize, page_size: usize, index: usize) -> usize {
    page.saturating_sub(1)
        .saturating_mul(page_size)
        .saturating_add(index)
        .saturating_add(1)
}

/// A finder's outcome for the leads it parsed: answered with leads, or empty.
pub(crate) fn answered(
    descriptor: &FinderDescriptor,
    leads: Vec<Lead>,
    started: Instant,
) -> FinderOutcome {
    let result = if leads.is_empty() {
        FinderResult::Empty
    } else {
        FinderResult::Answered
    };
    FinderOutcome {
        finder: descriptor.id.clone(),
        leads,
        result,
        elapsed: started.elapsed(),
    }
}

/// A finder's outcome for a failure.
pub(crate) fn failed(
    descriptor: &FinderDescriptor,
    cause: FailureCause,
    started: Instant,
) -> FinderOutcome {
    FinderOutcome {
        finder: descriptor.id.clone(),
        leads: Vec::new(),
        result: FinderResult::Failed(cause),
        elapsed: started.elapsed(),
    }
}

/// The failure a non-body response stands for.
pub(crate) fn cause_of(fetched: Fetched) -> FailureCause {
    match fetched {
        Fetched::Status { code, retry_after } => FailureCause::Status { code, retry_after },
        Fetched::Transport(detail) => FailureCause::Transport(detail),
        Fetched::Body(_) => FailureCause::Parse("unexpected body".to_string()),
    }
}

#[cfg(test)]
mod tests;
